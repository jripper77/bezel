// The editor store: the theme being edited (theme.json shape), the
// selection, and an undo/redo history. Every change goes through a command;
// commands are pure functions returning a new theme, so history is just a
// list of snapshots. A gesture (a drag) groups its commands into one step.

import { ORIENTATIONS, isHorizontal, relayoutBox, roundBox, unionBox } from './geometry.js';
import { createWidget, widgetOf } from './widgets.js';

/** Most undo steps kept. */
export const HISTORY_LIMIT = 200;

const clone = (v) => structuredClone(v);

function nextId(theme) {
  return theme.elements.reduce((max, e) => Math.max(max, e.id), 0) + 1;
}

function uniqueName(theme, base) {
  const names = new Set(theme.elements.map((e) => e.name));
  if (!names.has(base)) return base;
  for (let i = 2; ; i += 1) {
    const candidate = `${base} ${i}`;
    if (!names.has(candidate)) return candidate;
  }
}

function mapElements(theme, ids, fn) {
  const set = new Set(ids);
  return { ...theme, elements: theme.elements.map((e) => (set.has(e.id) ? fn(e) : e)) };
}

/** Deep merge of plain objects; arrays and other values replace. */
export function merge(target, patch) {
  if (patch === null || typeof patch !== 'object' || Array.isArray(patch)) return patch;
  const out = { ...(target && typeof target === 'object' && !Array.isArray(target) ? target : {}) };
  for (const [k, v] of Object.entries(patch)) out[k] = v === undefined ? out[k] : merge(out[k], v);
  return out;
}

// ------------------------------------------------------------- commands ----

/** Pure theme commands. Each returns `{theme, selection?}`. */
export const commands = {
  add(theme, { widget, x, y, sensor, name, names = ENGLISH }) {
    const made = createWidget(widget, theme.canvas, sensor);
    const id = nextId(theme);
    const frame = roundBox({ x: x - made.width / 2, y: y - made.height / 2, width: made.width, height: made.height });
    const element = { id, name: uniqueName(theme, name ?? sensor?.label ?? names.widget(widget)), frame, opacity: 1, visible: true, locked: false, kind: made.kind };
    return { theme: { ...theme, elements: [...theme.elements, element] }, selection: [id] };
  },

  remove(theme, { ids }) {
    const set = new Set(ids);
    return { theme: { ...theme, elements: theme.elements.filter((e) => !set.has(e.id)) }, selection: [] };
  },

  duplicate(theme, { ids, offset = 16, names = ENGLISH }) {
    let next = theme;
    const created = [];
    for (const e of theme.elements.filter((el) => ids.includes(el.id))) {
      const id = nextId(next);
      const copy = { ...clone(e), id, name: uniqueName(next, names.copy(e.name)), locked: false, frame: { ...e.frame, x: e.frame.x + offset, y: e.frame.y + offset } };
      next = { ...next, elements: [...next.elements, copy] };
      created.push(id);
    }
    return { theme: next, selection: created };
  },

  move(theme, { ids, dx, dy }) {
    return {
      theme: mapElements(theme, ids, (e) => (e.locked ? e : { ...e, frame: { ...e.frame, x: Math.round(e.frame.x + dx), y: Math.round(e.frame.y + dy) } })),
    };
  },

  setFrame(theme, { id, frame }) {
    return { theme: mapElements(theme, [id], (e) => (e.locked ? e : { ...e, frame: roundBox(frame) })) };
  },

  update(theme, { id, patch }) {
    return { theme: mapElements(theme, [id], (e) => merge(e, patch)) };
  },

  /** Replaces an element's kind (e.g. show a sensor as a ring instead of text). */
  setKind(theme, { id, kind }) {
    return { theme: mapElements(theme, [id], (e) => ({ ...e, kind: clone(kind) })) };
  },

  /** Merges theme properties; a new background replaces the old one whole. */
  setTheme(theme, { patch }) {
    const next = merge(theme, patch);
    if (patch.background) next.background = clone(patch.background);
    return { theme: next };
  },

  /**
   * Turns the theme to `orientation`. Between vertical and horizontal the
   * canvas swaps its sides and every element (locked ones too) keeps its
   * size while its center keeps its relative place, inside the canvas; a 180°
   * turn keeps the layout. One command, so one undo step.
   */
  setOrientation(theme, { orientation }) {
    if (!ORIENTATIONS.includes(orientation) || orientation === theme.orientation) return { theme };
    if (isHorizontal(orientation) === isHorizontal(theme.orientation)) return { theme: { ...theme, orientation } };
    const canvas = { width: theme.canvas.height, height: theme.canvas.width };
    const elements = theme.elements.map((e) => ({ ...e, frame: relayoutBox(e.frame, theme.canvas, canvas) }));
    return { theme: { ...theme, orientation, canvas, elements } };
  },

  /** Moves one element to `index` in the z-order (0 = bottom). */
  reorder(theme, { id, index }) {
    const from = theme.elements.findIndex((e) => e.id === id);
    if (from < 0) return { theme };
    const elements = [...theme.elements];
    const [e] = elements.splice(from, 1);
    elements.splice(Math.max(0, Math.min(index, elements.length)), 0, e);
    return { theme: { ...theme, elements } };
  },

  /** Aligns boxes to the selection's bounds (or the canvas with one element). */
  align(theme, { ids, edge }) {
    const targets = theme.elements.filter((e) => ids.includes(e.id) && !e.locked);
    const bounds = targets.length > 1 ? unionBox(targets.map((e) => e.frame)) : { x: 0, y: 0, ...theme.canvas };
    if (!bounds) return { theme };
    const place = (f) => {
      switch (edge) {
        case 'left': return { ...f, x: bounds.x };
        case 'right': return { ...f, x: bounds.x + bounds.width - f.width };
        case 'centerX': return { ...f, x: Math.round(bounds.x + (bounds.width - f.width) / 2) };
        case 'top': return { ...f, y: bounds.y };
        case 'bottom': return { ...f, y: bounds.y + bounds.height - f.height };
        case 'centerY': return { ...f, y: Math.round(bounds.y + (bounds.height - f.height) / 2) };
        default: return f;
      }
    };
    return { theme: mapElements(theme, targets.map((e) => e.id), (e) => ({ ...e, frame: place(e.frame) })) };
  },

  /** Spreads three or more boxes evenly between the outermost two. */
  distribute(theme, { ids, axis }) {
    const pos = axis === 'x' ? 'x' : 'y';
    const len = axis === 'x' ? 'width' : 'height';
    const targets = theme.elements.filter((e) => ids.includes(e.id) && !e.locked).sort((a, b) => a.frame[pos] - b.frame[pos]);
    if (targets.length < 3) return { theme };
    const first = targets[0].frame;
    const last = targets[targets.length - 1].frame;
    const total = targets.reduce((s, e) => s + e.frame[len], 0);
    const gap = (last[pos] + last[len] - first[pos] - total) / (targets.length - 1);
    let cursor = first[pos];
    const placed = new Map();
    for (const e of targets) {
      placed.set(e.id, Math.round(cursor));
      cursor += e.frame[len] + gap;
    }
    return { theme: mapElements(theme, [...placed.keys()], (e) => ({ ...e, frame: { ...e.frame, [pos]: placed.get(e.id) } })) };
  },
};

/**
 * How new elements are named: after their widget, and a copy after the
 * original. The UI passes names in its language; these are the defaults.
 */
const ENGLISH = Object.freeze({
  widget: (widget) => widget.charAt(0).toUpperCase() + widget.slice(1),
  copy: (name) => `${name} copy`,
});

// ---------------------------------------------------------------- store ----

/**
 * Creates a store around a theme.
 * @param {object} theme theme.json object
 * @param {{names?: {widget: (w: string) => string, copy: (name: string) => string}}} [options]
 *   how new elements and copies are named (English by default)
 */
export function createStore(theme, { names = ENGLISH } = {}) {
  let state = { theme: clone(theme), selection: [] };
  let past = [];
  let future = [];
  let gesture = null;
  // Themes are immutable, so "dirty" is "not the theme last saved or loaded":
  // undoing back to it makes the editor clean again.
  let saved = state.theme;
  const listeners = new Set();

  const emit = (reason) => {
    for (const fn of listeners) fn(state, reason);
  };

  const record = (before) => {
    past.push(before);
    if (past.length > HISTORY_LIMIT) past = past.slice(past.length - HISTORY_LIMIT);
    future = [];
  };

  return {
    getState: () => state,
    isDirty: () => state.theme !== saved,
    markSaved() {
      saved = state.theme;
      emit('saved');
    },
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },

    /** Applies a command by name; returns the new state. */
    dispatch(name, args = {}) {
      const command = commands[name];
      if (!command) throw new Error(`unknown command ${name}`);
      const before = state.theme;
      const result = command(state.theme, { names, ...args });
      if (result.theme === before && result.selection === undefined) return state;
      if (gesture) {
        if (!gesture.recorded) {
          record(gesture.start);
          gesture.recorded = true;
        }
      } else if (result.theme !== before) {
        record(before);
      }
      state = { theme: result.theme, selection: result.selection ?? state.selection.filter((id) => result.theme.elements.some((e) => e.id === id)) };
      emit(name);
      return state;
    },

    /** Whether a gesture (a drag) is going on. */
    isGesturing: () => gesture !== null,

    /** Groups the next commands (e.g. a drag) into one undo step. */
    beginGesture() {
      gesture = { start: state.theme, recorded: false };
    },
    endGesture() {
      // A drag that ends where it started leaves no undo step behind.
      if (gesture?.recorded && JSON.stringify(state.theme) === JSON.stringify(gesture.start)) past.pop();
      gesture = null;
      emit('gesture-end');
    },

    select(ids) {
      const valid = ids.filter((id) => state.theme.elements.some((e) => e.id === id));
      state = { ...state, selection: valid };
      emit('select');
      return state;
    },

    canUndo: () => past.length > 0,
    canRedo: () => future.length > 0,
    undo() {
      if (past.length === 0) return state;
      future.push(state.theme);
      const theme = past.pop();
      state = { theme, selection: state.selection.filter((id) => theme.elements.some((e) => e.id === id)) };
      emit('undo');
      return state;
    },
    redo() {
      if (future.length === 0) return state;
      past.push(state.theme);
      const theme = future.pop();
      state = { theme, selection: state.selection.filter((id) => theme.elements.some((e) => e.id === id)) };
      emit('redo');
      return state;
    },

    /** Replaces the whole theme (open/import): clears history. */
    load(next) {
      state = { theme: clone(next), selection: [] };
      past = [];
      future = [];
      saved = state.theme;
      emit('load');
      return state;
    },
  };
}

/** Elements of the selection, in z-order. */
export function selectedElements(state) {
  const set = new Set(state.selection);
  return state.theme.elements.filter((e) => set.has(e.id));
}

/** A short description for the layer list. */
export function layerLabel(element) {
  return { name: element.name, widget: widgetOf(element) };
}

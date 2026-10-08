// The editor store: the theme being edited (theme.json shape), the
// selection, and an undo/redo history. Every change goes through a command;
// commands are pure functions returning a new theme, so history is just a
// list of snapshots. A gesture (a drag) groups its commands into one step.

import { ORIENTATIONS, isHorizontal, relayoutBox, roundBox, unionBox } from './geometry.js';
import { createWidget, widgetOf } from './widgets.js';
import { withChildren, transformCard, selectionRoots, owners, syncGroupBounds, canGroup } from './cards.js';
import { layerDropIndex } from './layers.js';

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
  add(theme, { widget, x, y, sensor, name, cardMember, groupParent, names = ENGLISH }) {
    const made = createWidget(widget, theme.canvas, sensor);
    const id = nextId(theme);
    const frame = roundBox({ x: x - made.width / 2, y: y - made.height / 2, width: made.width, height: made.height });
    if (made.card) made.card.faces = [names.face?.(1) ?? 'Face 1'];
    const element = { ...(groupParent != null ? { groupParent } : {}), ...(cardMember && !made.card ? { cardMember } : {}), id, name: uniqueName(theme, name ?? sensor?.label ?? names.widget(widget)), frame, opacity: 1, visible: true, locked: false, kind: made.kind, ...(made.card ? { card: made.card } : {}) };
    return { theme: { ...theme, elements: [...theme.elements, element] }, selection: [id] };
  },

  cardDemo(theme, { x, y, title = 'METRICS', alternate = 'DETAILS', names = ENGLISH }) {
    const id = nextId(theme), size = Math.min(theme.canvas.width, theme.canvas.height);
    const width = Math.round(size * 0.85), height = Math.round(size * 0.85);
    const frame = { x: Math.round(x - width / 2), y: Math.round(y - height / 2), width, height };
    const shape = createWidget('card', theme.canvas);
    const parent = { id, name: uniqueName(theme, names.widget('card')), frame, visible: true, locked: false, opacity: 1, kind: { ...shape.kind, radius: 18 }, card: { faces: [title, alternate], activeFace: 0, transition: { effect: 'flip', direction: 'left', durationMs: 650, includeBase: true } } };
    const objects = [parent];
    for (let face = 0; face < 2; face++) {
      const heading = createWidget('text', theme.canvas), ring = createWidget('ring', theme.canvas);
      objects.push({ id: id + objects.length, name: face ? alternate : title, frame: { x: frame.x + width * 0.08, y: frame.y + height * 0.08, width: width * 0.84, height: height * 0.16 }, opacity: 1, visible: true, locked: false, cardMember: { parent: id, face }, kind: { ...heading.kind, content: { type: 'static', text: face ? alternate : title }, style: { ...heading.kind.style, size: Math.round(size * 0.05), align: 'center', color: face ? '#c4b5fdff' : '#67e8f9ff' } } });
      objects.push({ id: id + objects.length, name: `${face ? 'RAM' : 'CPU'} Ring`, frame: { x: frame.x + width * 0.25, y: frame.y + height * 0.3, width: width * 0.5, height: width * 0.5 }, opacity: 1, visible: true, locked: false, cardMember: { parent: id, face }, kind: { ...ring.kind, binding: { ...ring.kind.binding, key: face ? 'memory.usage' : 'cpu.usage' }, fill: face ? '#a78bfaff' : '#22d3eeff', testFull: true, thickness: Math.round(size * 0.035) } });
    }
    return { theme: { ...theme, elements: [...theme.elements, ...objects] }, selection: [id] };
  },

  remove(theme, { ids }) {
    const set = new Set(withChildren(theme, ids));
    return { theme: { ...theme, elements: theme.elements.filter((e) => !set.has(e.id)) }, selection: [] };
  },

  duplicate(theme, { ids, offset = 16, names = ENGLISH }) {
    return commands.paste(theme, { elements: theme.elements.filter((e) => withChildren(theme, ids).includes(e.id)), offset, names });
  },

  paste(theme, { elements, offset = 16, names = ENGLISH }) {
    let next = theme;
    const created = [];
    const mapping = new Map(elements.map((e, index) => [e.id, nextId(theme) + index]));
    for (const e of elements) {
      const id = nextId(next);
      const copy = { ...clone(e), id, name: uniqueName(next, names.copy(e.name)), locked: false, frame: { ...e.frame, x: e.frame.x + offset, y: e.frame.y + offset } };
      if (copy.groupParent != null) copy.groupParent = mapping.get(copy.groupParent) ?? null;
      if (copy.cardMember) copy.cardMember = mapping.has(copy.cardMember.parent) ? { ...copy.cardMember, parent: mapping.get(copy.cardMember.parent) } : null;
      next = { ...next, elements: [...next.elements, copy] };
      created.push(id);
    }
    return { theme: next, selection: created };
  },

  move(theme, { ids, dx, dy }) {
    const parents = new Set(selectionRoots(theme, ids).filter(e => !owners(theme, e).some(p => p.locked)).map(e => e.id));
    ids = withChildren(theme, [...parents]);
    return {
      theme: mapElements(theme, ids, (e) => (e.locked && !owners(theme, e).slice(1).some(p => parents.has(p.id)) ? e : { ...e, frame: { ...e.frame, x: Math.round(e.frame.x + dx), y: Math.round(e.frame.y + dy) } })),
    };
  },

  setFrame(theme, { id, frame }) {
    const e = theme.elements.find(e => e.id === id);
    return { theme: e && !owners(theme, e).some(p => p.locked) ? transformCard(theme, id, roundBox(frame)) : theme };
  },

  cardFace(theme, { id, face }) {
    return { theme: mapElements(theme, [id], e => e.card && Number.isInteger(face) && face >= 0 && face < e.card.faces.length ? { ...e, card: { ...e.card, activeFace: face } } : e), selection: [id] };
  },
  addCardFace(theme, { id, name }) {
    return { theme: mapElements(theme, [id], e => e.card && typeof name === 'string' && name.trim() && e.card.faces.length < 16 ? { ...e, card: { ...e.card, faces: [...e.card.faces, Array.from(name.trim()).slice(0, 32).join('')], activeFace: e.card.faces.length } } : e), selection: [id] };
  },
  duplicateCardFace(theme, { id, names = ENGLISH }) {
    const parent = theme.elements.find(e => e.id === id);
    if (!parent?.card || parent.card.faces.length >= 16) return { theme };
    const face = parent.card.activeFace, nextFace = parent.card.faces.length;
    const copies = theme.elements.filter(e => e.cardMember?.parent === id && e.cardMember.face === face);
    const result = commands.paste(theme, { elements: copies, offset: 0, names });
    const copied = new Set(result.selection);
    const next = mapElements(result.theme, [...copied], e => ({ ...e, cardMember: { parent: id, face: nextFace } }));
    return { theme: mapElements(next, [id], e => ({ ...e, card: { ...e.card, faces: [...e.card.faces, Array.from(names.copy(e.card.faces[face])).slice(0, 32).join('')], activeFace: nextFace } })), selection: [id] };
  },
  removeCardFace(theme, { id }) {
    const parent = theme.elements.find(e => e.id === id);
    if (!parent?.card || parent.card.faces.length <= 1) return { theme };
    const face = parent.card.activeFace;
    const elements = theme.elements.filter(e => !(e.cardMember?.parent === id && e.cardMember.face === face)).map(e => {
      if (e.id === id) return { ...e, card: { ...e.card, faces: e.card.faces.filter((_, i) => i !== face), activeFace: Math.min(face, e.card.faces.length - 2) } };
      if (e.cardMember?.parent === id && e.cardMember.face !== null && e.cardMember.face > face) return { ...e, cardMember: { ...e.cardMember, face: e.cardMember.face - 1 } };
      return e;
    });
    return { theme: { ...theme, elements }, selection: [id] };
  },
  reorderCardFace(theme, { id, direction }) {
    const parent = theme.elements.find(e => e.id === id);
    if (!parent?.card) return { theme };
    const from = parent.card.activeFace, to = from + direction;
    if (![-1, 1].includes(direction) || to < 0 || to >= parent.card.faces.length) return { theme };
    const swap = i => i === from ? to : i === to ? from : i;
    const elements = theme.elements.map(e => {
      if (e.id === id) { const faces = [...e.card.faces]; [faces[from], faces[to]] = [faces[to], faces[from]]; return { ...e, card: { ...e.card, faces, activeFace: to } }; }
      if (e.cardMember?.parent === id && e.cardMember.face !== null) return { ...e, cardMember: { ...e.cardMember, face: swap(e.cardMember.face) } };
      return e;
    });
    return { theme: { ...theme, elements }, selection: [id] };
  },
  attachCard(theme, { ids, parent, face = null }) {
    const card = theme.elements.find(e => e.id === parent && e.card);
    if (card && face !== null && (!Number.isInteger(face) || face < 0 || face >= card.card.faces.length)) return { theme };
    const roots = selectionRoots(theme, ids).filter(e => !e.card && e.id !== parent);
    const rootIds = new Set(roots.map(e => e.id));
    const members = new Set(withChildren(theme, [...rootIds]));
    // A card cannot itself be nested inside another card.
    if (theme.elements.some(e => members.has(e.id) && e.card)) return { theme };
    const next = mapElements(theme, [...members], e => ({ ...e,
      ...(rootIds.has(e.id) ? { groupParent: null } : {}),
      cardMember: card ? { parent, face } : null,
    }));
    if (!card) return { theme: next };
    const block = next.elements.filter(e => e.cardMember?.parent === parent);
    const elements = next.elements.filter(e => e.cardMember?.parent !== parent);
    elements.splice(elements.findIndex(e => e.id === parent) + 1, 0, ...block);
    return { theme: { ...next, elements } };
  },
  groupSelection(theme, { ids, names = ENGLISH }) {
    const members = selectionRoots(theme, ids).filter(e => !owners(theme, e).some(p => p.locked));
    if (!canGroup(theme, ids)) return { theme };
    const first = members[0];
    const id = nextId(theme);
    const group = { id, isGroup: true, groupParent: first.groupParent ?? null,
      cardMember: first.cardMember ?? null, name: uniqueName(theme, names.widget('group')),
      frame: unionBox(members.map(e => e.frame)), opacity: 1, visible: true, locked: false,
      kind: { type: 'shape', shape: 'rect', radius: 0, fill: null, stroke: null } };
    const direct = new Set(members.map(e => e.id)), all = new Set(withChildren(theme, [...direct]));
    const block = theme.elements.filter(e => all.has(e.id)).map(e => direct.has(e.id) ? { ...e, groupParent: id } : e);
    const elements = theme.elements.filter(e => !all.has(e.id));
    const anchor = theme.elements.findIndex(e => e.id === first.id);
    const index = theme.elements.slice(0, anchor).filter(e => !all.has(e.id)).length;
    elements.splice(index, 0, group, ...block);
    return { theme: { ...theme, elements }, selection: [id] };
  },
  ungroup(theme, { ids }) {
    const groups = theme.elements.filter(e => ids.includes(e.id) && e.isGroup && !owners(theme, e).some(p => p.locked));
    let elements = theme.elements, selection = [];
    for (const original of groups) {
      const group = elements.find(e => e.id === original.id);
      const members = elements.filter(e => e.groupParent === group.id);
      selection.push(...members.map(e => e.id));
      elements = elements.filter(e => e.id !== group.id).map(e => e.groupParent === group.id ? {
        ...e, groupParent: group.groupParent ?? null,
        opacity: (e.opacity ?? 1) * (group.opacity ?? 1), visible: e.visible !== false && group.visible !== false,
      } : e);
    }
    return groups.length ? { theme: { ...theme, elements }, selection: selection.filter(id => elements.some(e => e.id === id)) } : { theme };
  },
  cardFromSelection(theme, { ids, names = ENGLISH }) {
    const members = theme.elements.filter(e => ids.includes(e.id) && !e.card && !e.locked);
    if (!members.length) return { theme };
    const bounds = unionBox(members.map(e => e.frame));
    const made = createWidget('card', theme.canvas);
    made.card.faces = [names.face?.(1) ?? 'Face 1'];
    const id = nextId(theme);
    const card = { id, name: uniqueName(theme, names.widget('card')), frame: { x: bounds.x - 12, y: bounds.y - 12, width: bounds.width + 24, height: bounds.height + 24 }, opacity: 1, visible: true, locked: false, kind: made.kind, card: made.card };
    const elements = [...theme.elements];
    elements.splice(elements.findIndex(e => e.id === members[0].id), 0, card);
    const attached = commands.attachCard({ ...theme, elements }, { ids: members.map(e => e.id), parent: id, face: 0 });
    return { ...attached, selection: [id] };
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
    let next = { ...theme, orientation, canvas };
    for (const e of theme.elements.filter(e => !e.cardMember && e.groupParent == null)) next = transformCard(next, e.id, relayoutBox(e.frame, theme.canvas, canvas));
    return { theme: next };
  },

  /** Moves one element to `index` in the z-order (0 = bottom). */
  reorder(theme, { id, index }) {
    const from = theme.elements.findIndex((e) => e.id === id);
    if (from < 0) return { theme };
    const target = theme.elements[from];
    const moved = new Set(target.card || target.isGroup ? withChildren(theme, [id]) : [id]);
    const block = theme.elements.filter(e => moved.has(e.id));
    const elements = theme.elements.filter(e => !moved.has(e.id));
    const parent = target.groupParent ?? target.cardMember?.parent;
    const floor = parent != null ? elements.findIndex(e => e.id === parent) + 1 : 0;
    const siblings = target.groupParent != null ? new Set(withChildren({ elements }, [target.groupParent])) : null;
    const ceiling = siblings ? Math.max(floor, ...elements.flatMap((e, i) => siblings.has(e.id) ? [i + 1] : [])) : elements.length;
    elements.splice(Math.max(floor, Math.min(index, ceiling)), 0, ...block);
    return { theme: { ...theme, elements } };
  },

  /** Drag a layer relative to a sibling without changing card membership. */
  reorderLayer(theme, { id, target, before }) {
    const index = layerDropIndex(theme.elements, id, target, before);
    if (index === null) return { theme };
    const result = commands.reorder(theme, { id, index });
    return result.theme.elements.every((e, i) => e.id === theme.elements[i].id) ? { theme } : result;
  },

  /** Aligns boxes to the selection's bounds (or the canvas with one element). */
  align(theme, { ids, edge }) {
    const targets = selectionRoots(theme, ids).filter(e => !owners(theme, e).some(p => p.locked));
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
    let next = theme;
    for (const e of targets) next = transformCard(next, e.id, place(e.frame));
    return { theme: next };
  },

  /** Spreads three or more boxes evenly between the outermost two. */
  distribute(theme, { ids, axis }) {
    const pos = axis === 'x' ? 'x' : 'y';
    const len = axis === 'x' ? 'width' : 'height';
    const targets = selectionRoots(theme, ids).filter(e => !owners(theme, e).some(p => p.locked)).sort((a, b) => a.frame[pos] - b.frame[pos]);
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
    let next = theme;
    for (const e of targets) next = transformCard(next, e.id, { ...e.frame, [pos]: placed.get(e.id) });
    return { theme: next };
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
  let clipboard = [];
  let pasteCount = 0;
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
    capture: () => ({ state, past, future, saved, clipboard, pasteCount }),
    restore(document) {
      ({ state, past, future, saved, clipboard, pasteCount } = document);
      gesture = null;
      emit('load');
    },
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
      let cardMember, groupParent;
      if (name === 'add' && args.widget !== 'card' && state.selection.length === 1) {
        const selected = state.theme.elements.find(e => e.id === state.selection[0]);
        groupParent = selected?.isGroup ? selected.id : selected?.groupParent;
        cardMember = selected?.card ? { parent: selected.id, face: selected.card.activeFace } : selected?.cardMember;
      }
      const result = command(state.theme, { names, cardMember, groupParent, ...args });
      if (result.theme !== before) result.theme = syncGroupBounds(result.theme);
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

    copySelection() {
      const selected = state.theme.elements.filter(e => withChildren(state.theme, state.selection).includes(e.id));
      if (!selected.length) return;
      clipboard = clone(selected);
      pasteCount = 0;
      emit('copy');
    },
    canPaste: () => clipboard.length > 0,
    paste() {
      if (!clipboard.length) return state;
      pasteCount += 1;
      return this.dispatch('paste', { elements: clipboard, offset: 16 * pasteCount });
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
      clipboard = [];
      pasteCount = 0;
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

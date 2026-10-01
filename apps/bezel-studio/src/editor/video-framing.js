// The framing of a video background (D-2026-10-01-video-background-framing-2,
// -6): how the video is turned, fitted, zoomed and placed on the canvas.
// Pure functions over the theme's `background.framing` (the `.bezeltheme`
// JSON) for the inspector's controls and the canvas's framing mode. The
// backend resolves Auto (`video_auto`) and frames the real pictures; the
// geometry here mirrors the core's for pointer math and the demo.

/** The rotations a framing may ask for, clockwise degrees (absent: Auto). */
export const ROTATIONS = Object.freeze([0, 90, 180, 270]);
/** Fill (`cover`, the default) or Fit (`contain`). */
export const FITS = Object.freeze(['cover', 'contain']);
/** Zoom on top of the fit's scale: 100 % to 400 %. */
export const ZOOM_MIN = 1;
export const ZOOM_MAX = 4;
/** One step of the zoom slider and of the + and - keys: 5 %. */
export const ZOOM_STEP = 0.05;
/** An arrow key moves the picture 1 % of the position; with Shift, 10 %. */
export const NUDGE = 0.01;
export const NUDGE_LARGE = 0.1;
/** What Fit paints where the picture leaves the canvas uncovered: opaque black. */
export const DEFAULT_PAD = '#000000ff';
/** A wheel burst ends (one undo step) after this long without a turn, ms. */
export const WHEEL_IDLE_MS = 300;

/** The framing a video background has when it names none. */
export const DEFAULT_FRAMING = Object.freeze({
  rotation: null,
  fit: 'cover',
  zoom: 1,
  position: Object.freeze({ x: 0.5, y: 0.5 }),
  padColor: DEFAULT_PAD,
});

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const finite = (v, fallback) => (typeof v === 'number' && Number.isFinite(v) ? v : fallback);
const round = (v, digits) => Math.round(v * 10 ** digits) / 10 ** digits;
const EPSILON = 1e-9;

/** A zoom between 1 and 4, in hundredths (a whole percent). */
export function clampZoom(zoom) {
  return round(clamp(finite(zoom, 1), ZOOM_MIN, ZOOM_MAX), 2);
}

/** A position coordinate between 0 and 1, in thousandths. */
export function clampPosition(value) {
  return round(clamp(finite(value, 0.5), 0, 1), 3);
}

/** `#rrggbb[aa]` as an opaque `#rrggbbff` (the pad is opaque); anything else is black. */
export function opaqueColor(color) {
  return typeof color === 'string' && /^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(color) ? `${color.slice(0, 7).toLowerCase()}ff` : DEFAULT_PAD;
}

/**
 * The full framing of a background, every key at its default when absent:
 * `{rotation: 0|90|180|270|null (Auto), fit, zoom, position: {x, y}, padColor}`.
 * Out-of-range numbers are clamped, an unknown rotation is Auto.
 */
export function framingOf(background) {
  const f = background?.framing ?? {};
  return {
    rotation: ROTATIONS.includes(f.rotation) ? f.rotation : null,
    fit: FITS.includes(f.fit) ? f.fit : 'cover',
    zoom: clampZoom(f.zoom ?? 1),
    position: { x: clampPosition(f.position?.x ?? 0.5), y: clampPosition(f.position?.y ?? 0.5) },
    padColor: opaqueColor(f.padColor ?? DEFAULT_PAD),
  };
}

/**
 * What a theme keeps of a full framing: only the keys that differ from the
 * default (the position whole), `null` when nothing does.
 */
export function compactFraming(full) {
  const out = {};
  if (ROTATIONS.includes(full.rotation)) out.rotation = full.rotation;
  if (full.fit !== DEFAULT_FRAMING.fit) out.fit = full.fit;
  if (full.zoom !== DEFAULT_FRAMING.zoom) out.zoom = full.zoom;
  if (full.position.x !== 0.5 || full.position.y !== 0.5) out.position = { x: full.position.x, y: full.position.y };
  if (full.padColor !== DEFAULT_PAD) out.padColor = full.padColor;
  return Object.keys(out).length ? out : null;
}

/** Whether a framing changes nothing but (maybe) the rotation: Fill, 100 %, centered. */
export function isPlainFraming(full) {
  return full.fit === 'cover' && full.zoom === 1 && full.position.x === 0.5 && full.position.y === 0.5;
}

/**
 * The background with `patch` applied to its framing (`rotation: null` is
 * Auto; `position` may name one axis), checked and compacted: `framing` is
 * left out when everything is default.
 */
export function withFraming(background, patch) {
  const current = framingOf(background);
  const merged = { ...current, ...patch, position: { ...current.position, ...(patch.position ?? {}) } };
  const compact = compactFraming(framingOf({ framing: merged }));
  const { framing, ...rest } = background;
  return compact ? { ...rest, framing: compact } : rest;
}

/**
 * The background `patch` frames otherwise (see `withFraming`), or `null`
 * when the framing stays the same: nothing to record as an undo step.
 */
export function reframed(background, patch) {
  const next = withFraming(background, patch);
  return JSON.stringify(framingOf(next)) === JSON.stringify(framingOf(background)) ? null : next;
}

/** The background with its framing back to the default (no `framing` at all). */
export function withoutFraming(background) {
  const { framing, ...rest } = background;
  return rest;
}

/** The clockwise rotation the video shows with, degrees: the one chosen, else Auto's (0 while unknown). */
export function resolvedRotation(framing, auto) {
  if (ROTATIONS.includes(framing.rotation)) return framing.rotation;
  return ROTATIONS.includes(auto?.rotation) ? auto.rotation : 0;
}

/**
 * Where the turned picture lies on the canvas, in canvas pixels (it may
 * reach past the canvas): `source` turned `rotation` degrees clockwise,
 * scaled to cover or fit the canvas, times the zoom, and placed like CSS
 * `object-position` (on an axis where it overflows the position picks the
 * part shown, where it is smaller it places the picture). A source of
 * unknown size counts as already of the canvas's shape.
 * @param {{width: number, height: number}|null} source
 * @param {number} rotation
 * @param {ReturnType<typeof framingOf>} framing
 * @param {{width: number, height: number}} canvas
 */
export function pictureBox(source, rotation, framing, canvas) {
  const known = source?.width > 0 && source?.height > 0;
  const sideways = rotation % 180 === 90;
  const turned = !known ? canvas : sideways ? { width: source.height, height: source.width } : source;
  const sx = canvas.width / turned.width;
  const sy = canvas.height / turned.height;
  const scale = (framing.fit === 'contain' ? Math.min(sx, sy) : Math.max(sx, sy)) * framing.zoom;
  const width = turned.width * scale;
  const height = turned.height * scale;
  return { x: (canvas.width - width) * framing.position.x, y: (canvas.height - height) * framing.position.y, width, height };
}

/** The position that puts the picture at `offset` on an axis with `room` (canvas minus picture); `fallback` without room. */
function positionFor(offset, room, fallback) {
  return Math.abs(room) < EPSILON ? fallback : clampPosition(offset / room);
}

/**
 * The position after a drag of (dx, dy) canvas pixels from where it began:
 * the picture follows the pointer. `start` is the framing and `box` the
 * picture's box when the drag began; an axis with no room keeps its value.
 */
export function panned(start, box, canvas, dx, dy) {
  return {
    x: positionFor(box.x + dx, canvas.width - box.width, start.position.x),
    y: positionFor(box.y + dy, canvas.height - box.height, start.position.y),
  };
}

/**
 * The framing zoomed to `zoom` around `point` (canvas pixels): what is under
 * the point stays under it as far as the position allows.
 * @param {{source: object|null, rotation: number, canvas: {width: number, height: number}}} view
 */
export function zoomedAt(framing, view, zoom, point) {
  const next = { ...framing, zoom: clampZoom(zoom) };
  const before = pictureBox(view.source, view.rotation, framing, view.canvas);
  const after = pictureBox(view.source, view.rotation, next, view.canvas);
  const axis = (pt, off0, size0, size1, room, fallback) => positionFor(pt - ((pt - off0) / size0) * size1, room, fallback);
  return {
    ...next,
    position: {
      x: axis(point.x, before.x, before.width, after.width, view.canvas.width - after.width, framing.position.x),
      y: axis(point.y, before.y, before.height, after.height, view.canvas.height - after.height, framing.position.y),
    },
  };
}

/**
 * The framing after an arrow key: the picture moves the arrow's way by 1 %
 * of the position (10 % with Shift); an axis with no room stays.
 */
export function nudged(framing, view, dx, dy, large = false) {
  const step = large ? NUDGE_LARGE : NUDGE;
  const box = pictureBox(view.source, view.rotation, framing, view.canvas);
  const axis = (p, room, dir) => (dir === 0 || Math.abs(room) < EPSILON ? p : clampPosition(p + Math.sign(room) * Math.sign(dir) * step));
  return { ...framing, position: { x: axis(framing.position.x, view.canvas.width - box.width, dx), y: axis(framing.position.y, view.canvas.height - box.height, dy) } };
}

/** The zoom a wheel turn of `deltaY` pixels asks for (up zooms in), from `zoom`. */
export function wheelZoom(zoom, deltaY) {
  return clampZoom(zoom * Math.exp(-finite(deltaY, 0) / 600));
}

/** A wheel event's vertical travel in pixels (lines and pages turned to pixels). */
export function wheelPixels({ deltaY, deltaMode = 0 }) {
  return finite(deltaY, 0) * ([1, 16, 400][deltaMode] ?? 1);
}

/**
 * What a key does in the canvas's framing mode: `{type: 'nudge', dx, dy,
 * large}` (arrows, Shift for 10 %), `{type: 'zoom', step: 1|-1}` (+ and -),
 * `{type: 'reset'}` (0: 100 % and centered), `{type: 'leave'}` (Esc,
 * Enter), or `null` (the key is left to the rest of the editor: undo, save).
 */
export function framingKey(evt) {
  if (evt.ctrlKey || evt.metaKey || evt.altKey) return null;
  const arrows = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
  if (arrows[evt.key]) return { type: 'nudge', dx: arrows[evt.key][0], dy: arrows[evt.key][1], large: Boolean(evt.shiftKey) };
  if (evt.key === '+' || evt.key === '=') return { type: 'zoom', step: 1 };
  if (evt.key === '-' || evt.key === '_') return { type: 'zoom', step: -1 };
  if (evt.key === '0') return { type: 'reset' };
  if (evt.key === 'Escape' || evt.key === 'Enter') return { type: 'leave' };
  return null;
}

/**
 * The framing after a framing-mode key (`framingKey`'s action); `leave`
 * changes nothing. Zooming keeps the canvas's center where it is.
 */
export function applyFramingKey(framing, action, view) {
  switch (action.type) {
    case 'nudge': return nudged(framing, view, action.dx, action.dy, action.large);
    case 'zoom': {
      const center = { x: view.canvas.width / 2, y: view.canvas.height / 2 };
      return zoomedAt(framing, view, framing.zoom + action.step * ZOOM_STEP, center);
    }
    case 'reset': return { ...framing, zoom: 1, position: { x: 0.5, y: 0.5 } };
    default: return framing;
  }
}

/** Zoom and position as whole percents, for the readouts: `{zoom: 125, x: 50, y: 40}`. */
export function framingPercents(framing) {
  return { zoom: Math.round(framing.zoom * 100), x: Math.round(framing.position.x * 100), y: Math.round(framing.position.y * 100) };
}

/**
 * Groups a burst of events (wheel turns) into one gesture: the first
 * `touch()` begins it, and it ends `idleMs` after the last one, or at `flush()`.
 * @param {{begin: () => void, end: () => void, idleMs?: number, wait?: (fn: () => void, ms: number) => unknown, cancel?: (timer: unknown) => void}} deps
 */
export function createBurst({ begin, end, idleMs = WHEEL_IDLE_MS, wait = (fn, ms) => setTimeout(fn, ms), cancel = (timer) => clearTimeout(timer) }) {
  let timer = null;
  const finish = () => {
    timer = null;
    end();
  };
  return {
    touch() {
      if (timer === null) begin();
      else cancel(timer);
      timer = wait(finish, idleMs);
    },
    flush() {
      if (timer === null) return;
      cancel(timer);
      finish();
    },
    active: () => timer !== null,
  };
}

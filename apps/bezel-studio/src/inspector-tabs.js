// The inspector's tabs and its live preview, without the DOM: which tabs an
// object has, which one is open for each kind of object (kept through
// edits, re-renders and Undo), where the arrow keys move, and which part of
// the rendered frame the preview shows.

/** A card's tabs: its faces, how they change, what brings one forward, and how it looks. */
export const CARD_TABS = Object.freeze(['faces', 'motion', 'triggers', 'look']);

/** Every other object's tabs: what it shows and how it looks. */
export const OBJECT_TABS = Object.freeze(['data', 'look']);

/** The tabs of an object of `kind` (`card` or any widget name), in order. */
export function tabsOf(kind) {
  return kind === 'card' ? CARD_TABS : OBJECT_TABS;
}

/**
 * The tab open for each kind of object. A kind remembers the tab last chosen
 * for it; one never chosen, or no longer offered, opens the first offered.
 */
export function createTabMemory() {
  const chosen = new Map();
  return {
    /** The tab to open for `kind` among the `offered` ones (null when none is). */
    current(kind, offered) {
      const tab = chosen.get(kind);
      return offered.includes(tab) ? tab : offered[0] ?? null;
    },
    /** Remembers `tab` as the one open for `kind`. */
    choose(kind, tab) {
      chosen.set(kind, tab);
    },
  };
}

/**
 * The tab a key moves to from `current` (ARIA tabs pattern): ←/→ step and
 * wrap around, Home and End go to the ends. Null for any other key.
 */
export function tabAfterKey(tabs, current, key) {
  if (!tabs.length) return null;
  const at = Math.max(0, tabs.indexOf(current));
  if (key === 'ArrowRight') return tabs[(at + 1) % tabs.length];
  if (key === 'ArrowLeft') return tabs[(at - 1 + tabs.length) % tabs.length];
  if (key === 'Home') return tabs[0];
  if (key === 'End') return tabs[tabs.length - 1];
  return null;
}

/** The largest upscale of the preview: a tiny object is not blown into blur. */
export const PREVIEW_MAX_SCALE = 4;

/**
 * Where the preview copies from and how big it is drawn: the object's
 * `frame` cut to the `canvas` it is drawn on, scaled to fit a `box` (both
 * sides at most) and never more than {@link PREVIEW_MAX_SCALE} times.
 * Null when nothing of the object lies on the canvas or the box is empty.
 * @param {{x: number, y: number, width: number, height: number}} frame
 * @param {{width: number, height: number}} canvas
 * @param {{width: number, height: number}} box
 * @returns {{sx: number, sy: number, sw: number, sh: number, width: number, height: number}|null}
 */
export function previewCrop(frame, canvas, box) {
  const sx = Math.max(0, Math.floor(frame.x));
  const sy = Math.max(0, Math.floor(frame.y));
  const sw = Math.min(canvas.width, Math.ceil(frame.x + frame.width)) - sx;
  const sh = Math.min(canvas.height, Math.ceil(frame.y + frame.height)) - sy;
  if (!(sw > 0 && sh > 0 && box.width > 0 && box.height > 0)) return null;
  const scale = Math.min(box.width / sw, box.height / sh, PREVIEW_MAX_SCALE);
  return { sx, sy, sw, sh, width: Math.max(1, Math.round(sw * scale)), height: Math.max(1, Math.round(sh * scale)) };
}

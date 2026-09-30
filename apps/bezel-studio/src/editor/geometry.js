// Pure box geometry for the editor: hit testing, marquee, resize handles.
// Boxes are `{x, y, width, height}` in canvas pixels (the theme.json frame).

/** Smallest size an element can be resized to. */
export const MIN_SIZE = 4;

/** The eight resize handles, clockwise from the top-left corner. */
export const HANDLES = Object.freeze(['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w']);

/**
 * @param {{x:number,y:number,width:number,height:number}} b
 * @param {number} px
 * @param {number} py
 */
export function contains(b, px, py) {
  return px >= b.x && py >= b.y && px <= b.x + b.width && py <= b.y + b.height;
}

/** True when two boxes overlap (touching edges count). */
export function intersects(a, b) {
  return a.x <= b.x + b.width && b.x <= a.x + a.width && a.y <= b.y + b.height && b.y <= a.y + a.height;
}

/** The box spanned by two points (any order). */
export function boxFromPoints(x1, y1, x2, y2) {
  return { x: Math.min(x1, x2), y: Math.min(y1, y2), width: Math.abs(x2 - x1), height: Math.abs(y2 - y1) };
}

/** The smallest box containing all boxes, or null for none. */
export function unionBox(boxes) {
  if (boxes.length === 0) return null;
  const x = Math.min(...boxes.map((b) => b.x));
  const y = Math.min(...boxes.map((b) => b.y));
  const right = Math.max(...boxes.map((b) => b.x + b.width));
  const bottom = Math.max(...boxes.map((b) => b.y + b.height));
  return { x, y, width: right - x, height: bottom - y };
}

/**
 * The topmost visible element under a point (elements are bottom-to-top).
 * @param {Array<{id:number, frame:object, visible:boolean}>} elements
 */
export function hitTest(elements, px, py) {
  for (let i = elements.length - 1; i >= 0; i -= 1) {
    const e = elements[i];
    if (e.visible !== false && contains(e.frame, px, py)) return e.id;
  }
  return null;
}

/** Ids of visible elements whose box intersects the marquee. */
export function marqueeSelect(elements, marquee) {
  return elements.filter((e) => e.visible !== false && intersects(e.frame, marquee)).map((e) => e.id);
}

/** Center point of each handle of a box. */
export function handlePoints(b) {
  const cx = b.x + b.width / 2;
  const cy = b.y + b.height / 2;
  const r = b.x + b.width;
  const btm = b.y + b.height;
  return {
    nw: [b.x, b.y], n: [cx, b.y], ne: [r, b.y], e: [r, cy],
    se: [r, btm], s: [cx, btm], sw: [b.x, btm], w: [b.x, cy],
  };
}

/**
 * The box after dragging `handle` by (dx, dy). With `keepRatio` the corner
 * handles keep the original proportions. Sizes never go below MIN_SIZE and
 * the opposite edge stays put.
 */
export function resize(b, handle, dx, dy, keepRatio = false) {
  let { x, y, width, height } = b;
  const right = x + width;
  const bottom = y + height;
  if (handle.includes('w')) x = Math.min(x + dx, right - MIN_SIZE);
  if (handle.includes('e')) width = Math.max(width + dx, MIN_SIZE);
  if (handle.includes('n')) y = Math.min(y + dy, bottom - MIN_SIZE);
  if (handle.includes('s')) height = Math.max(height + dy, MIN_SIZE);
  if (handle.includes('w')) width = right - x;
  if (handle.includes('n')) height = bottom - y;
  if (keepRatio && handle.length === 2 && b.width > 0 && b.height > 0) {
    const ratio = b.width / b.height;
    if (width / height > ratio) width = height * ratio;
    else height = width / ratio;
    if (handle.includes('w')) x = right - width;
    if (handle.includes('n')) y = bottom - height;
  }
  return { x, y, width, height };
}

/** Rounds a box to whole pixels (what the file stores after an edit). */
export function roundBox(b) {
  const x = Math.round(b.x);
  const y = Math.round(b.y);
  return { x, y, width: Math.max(Math.round(b.x + b.width) - x, MIN_SIZE), height: Math.max(Math.round(b.y + b.height) - y, MIN_SIZE) };
}

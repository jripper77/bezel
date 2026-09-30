// Snapping: pull a moving box onto the canvas edges and center and onto the
// edges and centers of the other elements, and report the guide lines to draw.

/** Distance, in canvas pixels, under which a box snaps. */
export const SNAP_DISTANCE = 6;

function anchors(start, size) {
  return [start, start + size / 2, start + size];
}

function targets(canvasSize, others, axis) {
  const list = [0, canvasSize / 2, canvasSize];
  for (const o of others) {
    const start = axis === 'x' ? o.x : o.y;
    const size = axis === 'x' ? o.width : o.height;
    list.push(...anchors(start, size));
  }
  return list;
}

/** Best offset along one axis: the smallest correction under the threshold. */
function bestOffset(start, size, lines, threshold) {
  let best = null;
  for (const a of anchors(start, size)) {
    for (const t of lines) {
      const d = t - a;
      if (Math.abs(d) <= threshold && (best === null || Math.abs(d) < Math.abs(best.offset))) {
        best = { offset: d, line: t };
      }
    }
  }
  return best;
}

/**
 * Snaps a box being moved.
 * @param {{x:number,y:number,width:number,height:number}} box proposed position
 * @param {{width:number,height:number}} canvas
 * @param {Array<{x:number,y:number,width:number,height:number}>} others boxes of the other elements
 * @param {number} [threshold]
 * @returns {{box: object, guides: Array<{axis:'x'|'y', position:number}>}}
 */
export function snapMove(box, canvas, others, threshold = SNAP_DISTANCE) {
  const guides = [];
  const out = { ...box };
  const bx = bestOffset(box.x, box.width, targets(canvas.width, others, 'x'), threshold);
  if (bx) {
    out.x += bx.offset;
    guides.push({ axis: 'x', position: bx.line });
  }
  const by = bestOffset(box.y, box.height, targets(canvas.height, others, 'y'), threshold);
  if (by) {
    out.y += by.offset;
    guides.push({ axis: 'y', position: by.line });
  }
  return { box: out, guides };
}

/**
 * Snaps a single moving edge value (for resizing) to canvas and element lines.
 * @returns {{value:number, guide:number|null}}
 */
export function snapEdge(value, axis, canvas, others, threshold = SNAP_DISTANCE) {
  const lines = targets(axis === 'x' ? canvas.width : canvas.height, others, axis);
  let best = null;
  for (const t of lines) {
    const d = t - value;
    if (Math.abs(d) <= threshold && (best === null || Math.abs(d) < Math.abs(best - value))) best = t;
  }
  return best === null ? { value, guide: null } : { value: best, guide: best };
}

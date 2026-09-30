// The canvas: shows the rendered frame at a zoom level and handles direct
// manipulation on a DOM overlay (selection, eight resize handles, snapping
// guides, marquee). Everything is in canvas pixels until drawn.
import { el } from './dom.js';
import { HANDLES, boxFromPoints, handlePoints, hitTest, marqueeSelect, resize, unionBox } from '../editor/geometry.js';
import { snapEdge, snapMove } from '../editor/snap.js';

const MIN_ZOOM = 0.05;
const MAX_ZOOM = 8;
/** Pointer travel (screen px) before a press becomes a drag. */
const DRAG_THRESHOLD = 3;

/**
 * @param {object} opts
 * @param {ReturnType<import('../editor/store.js').createStore>} opts.store
 * @param {HTMLElement} opts.scroll scroll container of the stage
 * @param {HTMLElement} opts.box sized box holding canvas + overlay
 * @param {HTMLCanvasElement} opts.canvas
 * @param {HTMLElement} opts.overlay
 * @param {(zoom:number) => void} [opts.onZoom]
 * @param {(label:string) => string} [opts.describe] accessible label of an element box
 */
export function createCanvasView({ store, scroll, box, canvas, overlay, onZoom = () => {}, describe = (n) => n }) {
  const ctx = canvas.getContext('2d');
  let size = { width: canvas.width, height: canvas.height };
  let zoom = 1;
  let fitting = true;
  let guides = [];
  let marquee = null;
  let hoverId = null;
  let press = null;

  const theme = () => store.getState().theme;
  const selection = () => store.getState().selection;

  function layout() {
    box.style.width = `${Math.round(size.width * zoom)}px`;
    box.style.height = `${Math.round(size.height * zoom)}px`;
    drawOverlay();
  }

  function fit() {
    const pad = 96;
    const w = Math.max(scroll.clientWidth - pad, 50);
    const h = Math.max(scroll.clientHeight - pad, 50);
    zoom = Math.min(Math.max(Math.min(w / size.width, h / size.height), MIN_ZOOM), MAX_ZOOM);
    fitting = true;
    layout();
    onZoom(zoom);
  }

  function setZoom(z) {
    zoom = Math.min(Math.max(z, MIN_ZOOM), MAX_ZOOM);
    fitting = false;
    layout();
    onZoom(zoom);
  }

  function setCanvasSize(next) {
    if (next.width === size.width && next.height === size.height) return;
    size = { ...next };
    canvas.width = size.width;
    canvas.height = size.height;
    if (fitting) fit();
    else layout();
  }

  /** Draws a rendered frame (`{width, height, rgba}`). */
  function drawFrame(frame) {
    if (frame.width !== canvas.width || frame.height !== canvas.height) setCanvasSize(frame);
    ctx.putImageData(new ImageData(frame.rgba, frame.width, frame.height), 0, 0);
  }

  /** Canvas coordinates of a client point. */
  function point(clientX, clientY) {
    const r = box.getBoundingClientRect();
    return { x: (clientX - r.left) / zoom, y: (clientY - r.top) / zoom };
  }

  function containsClient(clientX, clientY) {
    const r = box.getBoundingClientRect();
    return clientX >= r.left && clientX <= r.right && clientY >= r.top && clientY <= r.bottom;
  }

  const px = (v) => `${v * zoom}px`;
  const boxStyle = (f) => ({ left: px(f.x), top: px(f.y), width: px(f.width), height: px(f.height) });

  function drawOverlay() {
    const t = theme();
    const sel = new Set(selection());
    const nodes = [];
    const hovered = t.elements.find((e) => e.id === hoverId && !sel.has(e.id) && e.visible !== false);
    if (hovered) nodes.push(el('div', { class: 'hover-box', style: boxStyle(hovered.frame) }));
    const chosen = t.elements.filter((e) => sel.has(e.id));
    for (const e of chosen) {
      nodes.push(el('div', { class: `sel-box${e.locked ? ' locked' : ''}`, style: boxStyle(e.frame), role: 'img', 'aria-label': describe(e.name) }));
    }
    if (chosen.length === 1 && !chosen[0].locked) {
      const pts = handlePoints(chosen[0].frame);
      for (const h of HANDLES) {
        nodes.push(el('div', { class: 'handle', dataset: { h }, style: { left: px(pts[h][0]), top: px(pts[h][1]) } }));
      }
    }
    for (const g of guides) {
      nodes.push(el('div', { class: `guide ${g.axis}`, style: g.axis === 'x' ? { left: px(g.position) } : { top: px(g.position) } }));
    }
    if (marquee) nodes.push(el('div', { class: 'marquee', style: boxStyle(marquee) }));
    overlay.replaceChildren(...nodes);
  }

  // ------------------------------------------------------ interactions ----

  function othersThan(ids) {
    const set = new Set(ids);
    return theme().elements.filter((e) => !set.has(e.id) && e.visible !== false).map((e) => e.frame);
  }

  function onPointerDown(evt) {
    if (evt.button !== 0) return;
    overlay.setPointerCapture?.(evt.pointerId);
    const start = point(evt.clientX, evt.clientY);
    const handle = evt.target?.dataset?.h;
    const state = store.getState();
    if (handle && state.selection.length === 1) {
      const e = state.theme.elements.find((x) => x.id === state.selection[0]);
      press = { mode: 'resize', id: e.id, handle, frame: { ...e.frame }, start, client: [evt.clientX, evt.clientY], started: false };
      return;
    }
    const id = hitTest(state.theme.elements, start.x, start.y);
    if (id === null) {
      if (!evt.shiftKey && !evt.ctrlKey && !evt.metaKey) store.select([]);
      press = { mode: 'marquee', start, base: evt.shiftKey ? [...state.selection] : [], client: [evt.clientX, evt.clientY], started: false };
      return;
    }
    let sel = state.selection;
    if (evt.shiftKey || evt.ctrlKey || evt.metaKey) {
      sel = sel.includes(id) ? sel.filter((x) => x !== id) : [...sel, id];
      store.select(sel);
    } else if (!sel.includes(id)) {
      sel = [id];
      store.select(sel);
    }
    const movable = state.theme.elements.filter((e) => sel.includes(e.id) && !e.locked);
    press = { mode: 'move', ids: movable.map((e) => e.id), frames: movable.map((e) => ({ ...e.frame })), start, applied: { x: 0, y: 0 }, client: [evt.clientX, evt.clientY], started: false };
  }

  function beginIfMoved(evt) {
    if (press.started) return true;
    const moved = Math.hypot(evt.clientX - press.client[0], evt.clientY - press.client[1]) >= DRAG_THRESHOLD;
    if (!moved) return false;
    press.started = true;
    if (press.mode !== 'marquee') store.beginGesture();
    return true;
  }

  function onMove(evt, p) {
    if (press.ids.length === 0) return;
    const dx = p.x - press.start.x;
    const dy = p.y - press.start.y;
    const union = unionBox(press.frames);
    const proposed = { ...union, x: union.x + dx, y: union.y + dy };
    const snapped = evt.altKey ? { box: proposed, guides: [] } : snapMove(proposed, size, othersThan(press.ids));
    guides = snapped.guides;
    const target = { x: Math.round(snapped.box.x - union.x), y: Math.round(snapped.box.y - union.y) };
    const change = { x: target.x - press.applied.x, y: target.y - press.applied.y };
    if (change.x !== 0 || change.y !== 0) {
      store.dispatch('move', { ids: press.ids, dx: change.x, dy: change.y });
      press.applied = target;
    }
  }

  function onResize(evt, p) {
    let next = resize(press.frame, press.handle, p.x - press.start.x, p.y - press.start.y, evt.shiftKey);
    guides = [];
    if (!evt.altKey) {
      const others = othersThan([press.id]);
      const h = press.handle;
      const snapX = (edge) => snapEdge(edge, 'x', size, others);
      const snapY = (edge) => snapEdge(edge, 'y', size, others);
      if (h.includes('e')) {
        const s = snapX(next.x + next.width);
        next = { ...next, width: s.value - next.x };
        if (s.guide !== null) guides.push({ axis: 'x', position: s.guide });
      } else if (h.includes('w')) {
        const s = snapX(next.x);
        next = { ...next, x: s.value, width: next.x + next.width - s.value };
        if (s.guide !== null) guides.push({ axis: 'x', position: s.guide });
      }
      if (h.includes('s')) {
        const s = snapY(next.y + next.height);
        next = { ...next, height: s.value - next.y };
        if (s.guide !== null) guides.push({ axis: 'y', position: s.guide });
      } else if (h.includes('n')) {
        const s = snapY(next.y);
        next = { ...next, y: s.value, height: next.y + next.height - s.value };
        if (s.guide !== null) guides.push({ axis: 'y', position: s.guide });
      }
    }
    store.dispatch('setFrame', { id: press.id, frame: next });
  }

  function onPointerMove(evt) {
    const p = point(evt.clientX, evt.clientY);
    if (!press) {
      const id = hitTest(theme().elements, p.x, p.y);
      if (id !== hoverId) {
        hoverId = id;
        drawOverlay();
      }
      return;
    }
    if (!beginIfMoved(evt)) return;
    if (press.mode === 'move') onMove(evt, p);
    else if (press.mode === 'resize') onResize(evt, p);
    else {
      marquee = boxFromPoints(press.start.x, press.start.y, p.x, p.y);
      const hits = marqueeSelect(theme().elements, marquee);
      store.select([...new Set([...press.base, ...hits])]);
    }
    drawOverlay();
  }

  function onPointerUp() {
    if (press?.started && press.mode !== 'marquee') store.endGesture();
    press = null;
    guides = [];
    marquee = null;
    drawOverlay();
  }

  overlay.addEventListener('pointerdown', onPointerDown);
  overlay.addEventListener('pointermove', onPointerMove);
  overlay.addEventListener('pointerup', onPointerUp);
  overlay.addEventListener('pointercancel', onPointerUp);
  overlay.addEventListener('pointerleave', () => {
    if (!press && hoverId !== null) {
      hoverId = null;
      drawOverlay();
    }
  });
  scroll.addEventListener('wheel', (evt) => {
    if (!evt.ctrlKey) return;
    evt.preventDefault();
    setZoom(zoom * (evt.deltaY < 0 ? 1.1 : 1 / 1.1));
  }, { passive: false });
  new ResizeObserver(() => { if (fitting) fit(); }).observe(scroll);

  return {
    fit,
    setZoom,
    zoom: () => zoom,
    setCanvasSize,
    drawFrame,
    drawOverlay,
    point,
    containsClient,
  };
}

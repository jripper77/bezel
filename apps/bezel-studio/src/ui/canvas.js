import { isShown, withChildren } from '../editor/cards.js';
// The canvas: shows the rendered frame at a zoom level and handles direct
// manipulation on a DOM overlay (selection, eight resize handles, snapping
// guides, marquee). Everything is in canvas pixels until drawn. In framing
// mode the overlay frames the video background instead
// (D-2026-10-01-video-background-framing-6): drag pans it, the wheel zooms
// around the pointer, keys nudge, zoom, reset and leave; each drag or wheel
// burst is one undo step, and no element can be selected meanwhile.
import { el } from './dom.js';
import { HANDLES, boxFromPoints, handlePoints, hitTest, marqueeSelect, resize, unionBox } from '../editor/geometry.js';
import { snapEdge, snapMove } from '../editor/snap.js';
import { applyFramingKey, createBurst, framingKey, framingOf, panned, pictureBox, reframed, resolvedRotation, wheelPixels, wheelZoom, zoomedAt } from '../editor/video-framing.js';

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
 * @param {() => void} [opts.onFrameRequest] a double click on the video background, off any element
 */
export function createCanvasView({ store, scroll, box, canvas, overlay, onZoom = () => {}, describe = (n) => n, onFrameRequest = () => {} }) {
  const ctx = canvas.getContext('2d');
  let size = { width: canvas.width, height: canvas.height };
  let zoom = 1;
  let fitting = true;
  let guides = [];
  let marquee = null;
  let hoverId = null;
  let press = null;
  // Framing mode: what Auto is, how the surface is named and described, and
  // how it is left (Esc, Enter); `null` while editing elements.
  let framing = null;

  const theme = () => store.getState().theme;
  const selection = () => store.getState().selection;
  // A burst of wheel turns is one gesture: one undo step.
  const wheelBurst = createBurst({ begin: () => store.beginGesture(), end: () => store.endGesture() });

  /** The video background's framing and what it is drawn against, or `null` without a video. */
  function framingView() {
    const bg = theme().background;
    if (bg.type !== 'video') return null;
    const current = framingOf(bg);
    const auto = framing?.auto() ?? null;
    return { framing: current, view: { source: auto?.size ?? null, rotation: resolvedRotation(current, auto), canvas: size } };
  }

  /** Frames the video background with `next` (one store command), unless nothing changes. */
  function applyFraming(next) {
    const framed = reframed(theme().background, next);
    if (framed) store.dispatch('setTheme', { patch: { background: framed } });
  }

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

  /** Framing mode's overlay: the turned picture's edges and a rule-of-thirds grid. */
  function drawFramingOverlay() {
    const fv = framingView();
    const nodes = [el('div', { class: 'framing-grid' })];
    if (fv) nodes.unshift(el('div', { class: 'framing-picture', style: boxStyle(pictureBox(fv.view.source, fv.view.rotation, fv.framing, size)) }));
    overlay.replaceChildren(...nodes);
  }

  function drawOverlay() {
    if (framing) {
      drawFramingOverlay();
      return;
    }
    const t = theme();
    const sel = new Set(selection());
    const nodes = [];
    const hovered = t.elements.find((e) => e.id === hoverId && !sel.has(e.id) && isShown(t, e));
    if (hovered) nodes.push(el('div', { class: 'hover-box', style: boxStyle(hovered.frame) }));
    const chosen = t.elements.filter((e) => sel.has(e.id) && (!e.cardMember || isShown(t, e)));
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
    const set = new Set(withChildren(theme(), ids));
    return theme().elements.filter((e) => !set.has(e.id) && isShown(theme(), e)).map((e) => e.frame);
  }

  /** A press in framing mode: the drag that follows pans the video. */
  function framePress(evt) {
    const fv = framingView();
    if (!fv) return;
    overlay.focus({ preventScroll: true });
    const picture = pictureBox(fv.view.source, fv.view.rotation, fv.framing, size);
    press = { mode: 'frame', start: fv.framing, picture, client: [evt.clientX, evt.clientY], started: false };
    overlay.classList.add('grabbing');
  }

  /** The picture follows the pointer from where the drag began. */
  function onFrame(evt) {
    const dx = (evt.clientX - press.client[0]) / zoom;
    const dy = (evt.clientY - press.client[1]) / zoom;
    applyFraming({ position: panned(press.start, press.picture, size, dx, dy) });
  }

  function onPointerDown(evt) {
    if (evt.button !== 0) return;
    overlay.setPointerCapture?.(evt.pointerId);
    if (framing) {
      framePress(evt);
      return;
    }
    const start = point(evt.clientX, evt.clientY);
    const handle = evt.target?.dataset?.h;
    const state = store.getState();
    if (handle && state.selection.length === 1) {
      const e = state.theme.elements.find((x) => x.id === state.selection[0]);
      press = { mode: 'resize', id: e.id, handle, frame: { ...e.frame }, start, client: [evt.clientX, evt.clientY], started: false };
      return;
    }
    const id = hitTest(state.theme.elements.filter(e => isShown(state.theme, e)), start.x, start.y);
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
    const movable = state.theme.elements.filter((e) => sel.includes(e.id) && !e.locked && (!e.cardMember || isShown(state.theme, e)));
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
      const id = framing ? null : hitTest(theme().elements.filter(e => isShown(theme(), e)), p.x, p.y);
      if (id !== hoverId) {
        hoverId = id;
        drawOverlay();
      }
      return;
    }
    if (!beginIfMoved(evt)) return;
    if (press.mode === 'frame') onFrame(evt);
    else if (press.mode === 'move') onMove(evt, p);
    else if (press.mode === 'resize') onResize(evt, p);
    else {
      marquee = boxFromPoints(press.start.x, press.start.y, p.x, p.y);
      const hits = marqueeSelect(theme().elements.filter(e => isShown(theme(), e)), marquee);
      store.select([...new Set([...press.base, ...hits])]);
    }
    drawOverlay();
  }

  function onPointerUp() {
    if (press?.started && press.mode !== 'marquee') store.endGesture();
    press = null;
    guides = [];
    marquee = null;
    overlay.classList.remove('grabbing');
    drawOverlay();
  }

  /** The wheel in framing mode zooms the video around the pointer (Ctrl+wheel still zooms the view). */
  function onFramingWheel(evt) {
    if (!framing || evt.ctrlKey) return;
    const fv = framingView();
    if (!fv) return;
    evt.preventDefault();
    wheelBurst.touch();
    applyFraming(zoomedAt(fv.framing, fv.view, wheelZoom(fv.framing.zoom, wheelPixels(evt)), point(evt.clientX, evt.clientY)));
  }

  /** Framing mode's keys: arrows, + and -, 0, Esc and Enter; the rest (undo, save) goes on to the editor. */
  function onFramingKey(evt) {
    if (!framing) return;
    const action = framingKey(evt);
    if (!action) return;
    evt.preventDefault();
    evt.stopPropagation();
    if (action.type === 'leave') {
      framing.onLeave();
      return;
    }
    const fv = framingView();
    if (fv) applyFraming(applyFramingKey(fv.framing, action, fv.view));
  }

  /** A double click on the video background, off any element, asks for framing mode. */
  function onDoubleClick(evt) {
    if (framing || theme().background.type !== 'video') return;
    const p = point(evt.clientX, evt.clientY);
    if (hitTest(theme().elements.filter(e => isShown(theme(), e)), p.x, p.y) === null) onFrameRequest();
  }

  /**
   * Framing mode on (`mode`) or off (`null`). On, the overlay is a focusable
   * surface named `mode.label` and described by the element `mode.describedBy`
   * (the keys); `mode.auto()` is what Auto is now (`video_auto`), and
   * `mode.onLeave()` is asked for by Esc or Enter.
   * @param {{auto: () => {rotation: number, size: {width: number, height: number}|null}|null, label: string, describedBy: string, onLeave: () => void}|null} mode
   */
  function setFraming(mode) {
    wheelBurst.flush();
    if (press?.started && press.mode !== 'marquee') store.endGesture();
    framing = mode;
    press = null;
    hoverId = null;
    guides = [];
    marquee = null;
    overlay.classList.toggle('framing', Boolean(mode));
    overlay.classList.remove('grabbing');
    if (mode) {
      overlay.tabIndex = 0;
      overlay.setAttribute('role', 'application');
      overlay.setAttribute('aria-label', mode.label);
      overlay.setAttribute('aria-describedby', mode.describedBy);
    } else {
      for (const name of ['tabindex', 'role', 'aria-label', 'aria-describedby']) overlay.removeAttribute(name);
    }
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
  overlay.addEventListener('wheel', onFramingWheel, { passive: false });
  overlay.addEventListener('keydown', onFramingKey);
  overlay.addEventListener('dblclick', onDoubleClick);
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
    setFraming,
    framing: () => framing !== null,
    /** Focuses the framing surface (framing mode only). */
    focusFraming: () => framing && overlay.focus({ preventScroll: true }),
  };
}

// Pointer events also work in Tauri, which intercepts native HTML file drops.
import { el } from './dom.js';
import { layerDropIndex } from '../editor/layers.js';

export function wireLayerDrag(list, store) {
  let press = null, ghost = null, marker = null, frame = null, suppressClick = false;
  const scroller = list.closest('.library');
  const unmark = () => {
    marker?.classList.remove('layer-drop-before', 'layer-drop-after');
    marker = null;
  };
  function refresh() {
    unmark();
    if (!press?.dragging) return;
    const row = document.elementFromPoint(press.x, press.y)?.closest('.layer-row');
    if (!row || !list.contains(row)) { press.drop = null; return; }
    const target = Number(row.dataset.elementId), box = row.getBoundingClientRect();
    const before = press.y < box.top + box.height / 2;
    const index = layerDropIndex(store.getState().theme.elements, press.id, target, before);
    press.drop = index === null ? null : { id: press.id, target, before };
    if (press.drop) {
      marker = row;
      row.classList.add(before ? 'layer-drop-before' : 'layer-drop-after');
    }
  }
  function scrollFrame() {
    frame = null;
    if (!press?.dragging) return;
    if (scroller) {
      const bounds = scroller.getBoundingClientRect();
      const over = press.x >= bounds.left && press.x <= bounds.right && press.y >= bounds.top && press.y <= bounds.bottom;
      const distance = over ? press.y < bounds.top + 30 ? -8 : press.y > bounds.bottom - 30 ? 8 : 0 : 0;
      if (distance) { scroller.scrollTop += distance; refresh(); }
    }
    frame = requestAnimationFrame(scrollFrame);
  }
  function cleanup() {
    const pointerId = press?.pointerId;
    press = null;
    if (frame !== null) cancelAnimationFrame(frame);
    frame = null;
    unmark(); ghost?.remove(); ghost = null;
    list.classList.remove('layer-dragging');
    if (pointerId != null && list.hasPointerCapture?.(pointerId)) list.releasePointerCapture(pointerId);
  }
  list.addEventListener('pointerdown', evt => {
    if (evt.button !== 0 || press || evt.target.closest('input, .icon-button, .layer-group-title')) return;
    const row = evt.target.closest('.layer-row');
    if (!row || !list.contains(row)) return;
    const id = Number(row.dataset.elementId);
    const element = store.getState().theme.elements.find(e => e.id === id);
    if (!element) return;
    press = { id, pointerId: evt.pointerId, startX: evt.clientX, startY: evt.clientY, x: evt.clientX, y: evt.clientY, dragging: false, label: element.name, drop: null };
    // Capture only after a drag begins so ordinary name clicks still select.
  });
  list.addEventListener('pointermove', evt => {
    if (!press || evt.pointerId !== press.pointerId) return;
    press.x = evt.clientX; press.y = evt.clientY;
    if (!press.dragging && Math.hypot(press.x - press.startX, press.y - press.startY) >= 4) {
      press.dragging = true;
      list.setPointerCapture?.(evt.pointerId);
      ghost = el('div', { class: 'drop-ghost layer-drag-ghost', text: press.label });
      document.body.append(ghost);
      list.classList.add('layer-dragging');
      frame = requestAnimationFrame(scrollFrame);
    }
    if (!press.dragging) return;
    evt.preventDefault();
    ghost.style.left = `${Math.max(8, Math.min(press.x + 12, window.innerWidth - ghost.offsetWidth - 8))}px`;
    ghost.style.top = `${Math.max(8, Math.min(press.y + 12, window.innerHeight - ghost.offsetHeight - 8))}px`;
    refresh();
  });
  list.addEventListener('pointerup', evt => {
    if (!press || evt.pointerId !== press.pointerId) return;
    const dragging = press.dragging;
    press.x = evt.clientX; press.y = evt.clientY;
    refresh();
    const drop = press.drop;
    cleanup();
    if (!dragging) return;
    suppressClick = true;
    setTimeout(() => { suppressClick = false; }, 0);
    evt.preventDefault();
    if (drop) store.dispatch('reorderLayer', drop);
  });
  list.addEventListener('click', evt => {
    if (suppressClick) { evt.preventDefault(); evt.stopImmediatePropagation(); }
  }, true);
  list.addEventListener('pointercancel', cleanup);
  list.addEventListener('lostpointercapture', cleanup);
  window.addEventListener('blur', cleanup);
  document.addEventListener('keydown', evt => {
    if (evt.key === 'Escape' && press?.dragging) { evt.preventDefault(); evt.stopPropagation(); cleanup(); }
  }, true);
  return { refresh };
}

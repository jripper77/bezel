// Dragging from the library onto the canvas with pointer events (Tauri owns
// HTML5 drag-and-drop for files). A press that does not travel is a click.
import { el } from './dom.js';

const THRESHOLD = 4;

/**
 * Makes `source` draggable onto the canvas.
 * @param {HTMLElement} source
 * @param {object} opts
 * @param {string} opts.label text of the ghost that follows the pointer
 * @param {{containsClient:(x:number,y:number)=>boolean, point:(x:number,y:number)=>{x:number,y:number}}} opts.canvas
 * @param {HTMLElement} opts.stage gets a `drop-target` class while hovering
 * @param {(pos:{x:number,y:number}) => void} opts.onDrop canvas position of the drop
 * @param {() => void} opts.onClick press without drag (or Enter/Space)
 */
export function makeDraggable(source, { label, canvas, stage, onDrop, onClick }) {
  let press = null;
  let ghost = null;

  const cleanup = () => {
    ghost?.remove();
    ghost = null;
    stage.classList.remove('drop-target');
    press = null;
  };

  source.addEventListener('pointerdown', (evt) => {
    if (evt.button !== 0) return;
    press = { x: evt.clientX, y: evt.clientY, dragging: false };
    source.setPointerCapture?.(evt.pointerId);
  });
  source.addEventListener('pointermove', (evt) => {
    if (!press) return;
    if (!press.dragging && Math.hypot(evt.clientX - press.x, evt.clientY - press.y) >= THRESHOLD) {
      press.dragging = true;
      ghost = el('div', { class: 'drop-ghost', text: label });
      document.body.append(ghost);
    }
    if (press.dragging) {
      ghost.style.left = `${evt.clientX}px`;
      ghost.style.top = `${evt.clientY}px`;
      stage.classList.toggle('drop-target', canvas.containsClient(evt.clientX, evt.clientY));
    }
  });
  source.addEventListener('pointerup', (evt) => {
    if (!press) return;
    const { dragging } = press;
    const over = canvas.containsClient(evt.clientX, evt.clientY);
    cleanup();
    if (!dragging) onClick();
    else if (over) onDrop(canvas.point(evt.clientX, evt.clientY));
  });
  source.addEventListener('pointercancel', cleanup);
  source.addEventListener('keydown', (evt) => {
    if (evt.key === 'Enter' || evt.key === ' ') {
      evt.preventDefault();
      onClick();
    }
  });
}

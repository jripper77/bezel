// Keyboard shortcuts: maps a keydown to an editor action. Typing in a field
// never triggers them (except Ctrl+S).

const TYPING = new Set(['INPUT', 'TEXTAREA', 'SELECT']);

/**
 * @param {{key:string, ctrlKey?:boolean, metaKey?:boolean, shiftKey?:boolean}} evt
 * @param {{tagName?:string, isContentEditable?:boolean}|null} focused
 * @returns {null | {type:string, dx?:number, dy?:number}}
 */
export function shortcutFor(evt, focused) {
  const mod = evt.ctrlKey || evt.metaKey;
  const key = evt.key.length === 1 ? evt.key.toLowerCase() : evt.key;
  if (mod && key === 's') return { type: evt.shiftKey ? 'saveAs' : 'save' };
  if (focused && (TYPING.has(focused.tagName) || focused.isContentEditable)) return null;
  if (mod && key === 'z') return { type: evt.shiftKey ? 'redo' : 'undo' };
  if (mod && key === 'y') return { type: 'redo' };
  if (mod && key === 'd') return { type: 'duplicate' };
  if (mod && key === 'a') return { type: 'selectAll' };
  if (key === 'Delete' || key === 'Backspace') return { type: 'remove' };
  if (key === 'Escape') return { type: 'deselect' };
  const step = evt.shiftKey ? 10 : 1;
  const arrows = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
  if (key in arrows) return { type: 'nudge', dx: arrows[key][0], dy: arrows[key][1] };
  return null;
}

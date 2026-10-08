// Keyboard shortcuts: maps a keydown to an editor action. Typing in a field
// never triggers them (except Ctrl+S), and the arrows stay with the widgets
// that move with them (tabs, sliders, radio groups, lists).

const TYPING = new Set(['INPUT', 'TEXTAREA', 'SELECT']);

/** ARIA roles whose widgets use the arrow keys themselves. */
export const ARROW_ROLES = new Set([
  'tab', 'tablist', 'slider', 'radio', 'radiogroup', 'listbox', 'option',
  'menu', 'menubar', 'menuitem', 'menuitemradio', 'menuitemcheckbox',
  'tree', 'treeitem', 'grid', 'gridcell', 'combobox', 'spinbutton', 'scrollbar',
]);

const ARROWS = new Set(['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown']);

/**
 * Whether `focused` (or a container of it) is a widget that moves with the
 * arrow keys, like a tab of a tab list.
 * @param {{getAttribute?: (name: string) => string|null, parentElement?: object|null}|null} focused
 */
export function usesArrows(focused) {
  for (let node = focused; node; node = node.parentElement ?? null) {
    if (ARROW_ROLES.has(node.getAttribute?.('role'))) return true;
  }
  return false;
}

/**
 * @param {{key:string, ctrlKey?:boolean, metaKey?:boolean, shiftKey?:boolean}} evt
 * @param {{tagName?:string, isContentEditable?:boolean, getAttribute?:Function, parentElement?:object|null}|null} focused
 * @returns {null | {type:string, dx?:number, dy?:number}}
 */
export function shortcutFor(evt, focused) {
  const mod = evt.ctrlKey || evt.metaKey;
  const key = evt.key.length === 1 ? evt.key.toLowerCase() : evt.key;
  if (mod && key === 's') return { type: evt.shiftKey ? 'saveAs' : 'save' };
  if (focused && (TYPING.has(focused.tagName) || focused.isContentEditable)) return null;
  if (mod && key === 'z') return { type: evt.shiftKey ? 'redo' : 'undo' };
  if (mod && key === 'y') return { type: 'redo' };
  if (mod && key === 'c') return { type: 'copy' };
  if (mod && key === 'v') return { type: 'paste' };
  if (mod && key === 'g') return { type: evt.shiftKey ? 'ungroup' : 'groupSelection' };
  if (mod && key === 'd') return { type: 'duplicate' };
  if (mod && key === 'a') return { type: 'selectAll' };
  if (key === 'Delete' || key === 'Backspace') return { type: 'remove' };
  if (key === 'Escape') return { type: 'deselect' };
  if (!ARROWS.has(key) || usesArrows(focused)) return null;
  const step = evt.shiftKey ? 10 : 1;
  const arrows = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
  return { type: 'nudge', dx: arrows[key][0], dy: arrows[key][1] };
}

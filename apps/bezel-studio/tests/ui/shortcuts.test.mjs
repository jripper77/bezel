import { test } from 'node:test';
import assert from 'node:assert/strict';
import { shortcutFor, usesArrows } from '../../src/shortcuts.js';

const key = (k, mods = {}) => ({ key: k, ...mods });

test('editing shortcuts', () => {
  assert.deepEqual(shortcutFor(key('z', { ctrlKey: true }), null), { type: 'undo' });
  assert.deepEqual(shortcutFor(key('Z', { ctrlKey: true, shiftKey: true }), null), { type: 'redo' });
  assert.deepEqual(shortcutFor(key('y', { metaKey: true }), null), { type: 'redo' });
  assert.deepEqual(shortcutFor(key('d', { ctrlKey: true }), null), { type: 'duplicate' });
  assert.deepEqual(shortcutFor(key('a', { ctrlKey: true }), null), { type: 'selectAll' });
  assert.deepEqual(shortcutFor(key('Delete'), null), { type: 'remove' });
  assert.deepEqual(shortcutFor(key('Backspace'), null), { type: 'remove' });
  assert.deepEqual(shortcutFor(key('Escape'), null), { type: 'deselect' });
  assert.equal(shortcutFor(key('q'), null), null);
});

test('arrows nudge by 1, or 10 with shift', () => {
  assert.deepEqual(shortcutFor(key('ArrowLeft'), null), { type: 'nudge', dx: -1, dy: 0 });
  assert.deepEqual(shortcutFor(key('ArrowDown', { shiftKey: true }), null), { type: 'nudge', dx: 0, dy: 10 });
});

test('typing in a field keeps its keys, except save', () => {
  const input = { tagName: 'INPUT' };
  assert.equal(shortcutFor(key('Delete'), input), null);
  assert.equal(shortcutFor(key('z', { ctrlKey: true }), { tagName: 'DIV', isContentEditable: true }), null);
  assert.deepEqual(shortcutFor(key('s', { ctrlKey: true }), input), { type: 'save' });
  assert.deepEqual(shortcutFor(key('S', { ctrlKey: true, shiftKey: true }), input), { type: 'saveAs' });
  assert.deepEqual(shortcutFor(key('Delete'), { tagName: 'BUTTON' }), { type: 'remove' });
});

/** A fake element with an ARIA role inside `parent`. */
const node = (tagName, role = null, parent = null) => ({
  tagName,
  getAttribute: (name) => (name === 'role' ? role : null),
  parentElement: parent,
});

test('arrows stay with widgets that move with them', () => {
  const tablist = node('DIV', 'tablist');
  const tab = node('BUTTON', 'tab', tablist);
  for (const focused of [tab, tablist, node('DIV', 'slider'), node('SPAN', null, node('DIV', 'radiogroup')), node('LI', 'option', node('UL', 'listbox'))]) {
    assert.equal(shortcutFor(key('ArrowRight'), focused), null);
    assert.equal(shortcutFor(key('ArrowUp', { shiftKey: true }), focused), null);
  }
  // Other shortcuts still work there, and arrows nudge from plain buttons.
  assert.deepEqual(shortcutFor(key('z', { ctrlKey: true }), tab), { type: 'undo' });
  assert.deepEqual(shortcutFor(key('Delete'), tab), { type: 'remove' });
  assert.deepEqual(shortcutFor(key('ArrowRight'), node('BUTTON', null, node('LI'))), { type: 'nudge', dx: 1, dy: 0 });
  assert.equal(usesArrows(null), false);
  assert.equal(usesArrows(node('BUTTON')), false);
});

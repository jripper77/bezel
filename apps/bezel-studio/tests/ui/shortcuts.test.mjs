import { test } from 'node:test';
import assert from 'node:assert/strict';
import { shortcutFor } from '../../src/shortcuts.js';

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

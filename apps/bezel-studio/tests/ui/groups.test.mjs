import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createStore } from '../../src/editor/store.js';
import { isShown, effectiveOpacity } from '../../src/editor/cards.js';
import { layerGroups } from '../../src/editor/layers.js';
import { DEMO_THEME } from '../../src/demo-theme.js';
const element = (s, id) => s.getState().theme.elements.find(e => e.id === id);
function setup() {
  const s = createStore({ ...DEMO_THEME, elements: [] });
  s.dispatch('add', { widget: 'shape', x: 100, y: 100 }); s.select([]);
  s.dispatch('add', { widget: 'text', x: 300, y: 100 });
  s.dispatch('groupSelection', { ids: [1, 2] }); return s;
}
test('ordinary groups move, scale, expose members and undo as a unit', () => {
  const s = setup(), before = structuredClone(s.getState().theme);
  assert.equal(element(s, 3).isGroup, true); assert.equal(element(s, 3).card, undefined);
  assert.deepEqual(layerGroups(before.elements)[0].children.map(n => n.element.id), [2, 1]);
  s.dispatch('move', { ids: [3, 1], dx: 25, dy: 30 });
  for (const e of before.elements) assert.equal(element(s, e.id).frame.x, e.frame.x + 25);
  const frame = element(s, 3).frame;
  s.dispatch('setFrame', { id: 3, frame: { ...frame, width: frame.width * 2, height: frame.height * 2 } });
  assert.equal(element(s, 1).frame.width, before.elements.find(e => e.id === 1).frame.width * 2);
  s.undo(); s.undo(); assert.deepEqual(s.getState().theme, before);
  s.dispatch('move', { ids: [1], dx: -50, dy: 0 });
  assert.equal(element(s, 3).frame.x, element(s, 1).frame.x);
});
test('copy groups remaps members; copying one member detaches; empty groups disappear', () => {
  const s = setup(); s.select([3]); s.copySelection(); s.paste();
  const copied = s.getState().theme.elements.find(e => e.isGroup && e.id !== 3);
  assert.equal(s.getState().theme.elements.filter(e => e.groupParent === copied.id).length, 2);
  s.undo(); s.select([1]); s.copySelection(); s.paste();
  const lone = element(s, s.getState().selection[0]); assert.equal(lone.groupParent, null);
  s.undo(); s.dispatch('remove', { ids: [1, 2] }); assert.equal(s.getState().theme.elements.length, 0);
  s.undo(); assert.equal(s.getState().theme.elements.length, 3);
});
test('nested group visibility, opacity, lock and ungroup preserve appearance', () => {
  const s = setup(); s.select([]); s.dispatch('add', { widget: 'shape', x: 400, y: 200 });
  s.dispatch('groupSelection', { ids: [3, 4] });
  s.dispatch('update', { id: 5, patch: { opacity: 0.5 } });
  s.dispatch('update', { id: 3, patch: { opacity: 0.5 } });
  assert.equal(effectiveOpacity(s.getState().theme, element(s, 1)), 0.25);
  s.dispatch('update', { id: 5, patch: { locked: true } }); const x = element(s, 1).frame.x;
  s.dispatch('move', { ids: [1], dx: 30, dy: 0 }); assert.equal(element(s, 1).frame.x, x);
  s.dispatch('update', { id: 5, patch: { locked: false, visible: false } });
  assert.equal(isShown(s.getState().theme, element(s, 1)), false);
  s.dispatch('ungroup', { ids: [5] });
  assert.equal(element(s, 3).visible, false); assert.equal(element(s, 3).opacity, 0.25);
  assert.equal(element(s, 3).groupParent, null);
  s.undo(); assert.equal(element(s, 3).groupParent, 5);
});
test('groups on card faces retain scope, inherit opacity once and move to another face together', () => {
  const s = setup(); s.select([]); s.dispatch('add', { widget: 'card', x: 240, y: 400 });
  s.dispatch('attachCard', { ids: [3], parent: 4, face: 0 });
  s.dispatch('update', { id: 4, patch: { opacity: 0.5 } });
  assert.equal(effectiveOpacity(s.getState().theme, element(s, 1)), 0.5);
  s.dispatch('addCardFace', { id: 4, name: 'B' });
  assert.equal(isShown(s.getState().theme, element(s, 1)), false);
  s.dispatch('attachCard', { ids: [3], parent: 4, face: 1 });
  for (const id of [1, 2, 3]) assert.deepEqual(element(s, id).cardMember, { parent: 4, face: 1 });
  assert.equal(isShown(s.getState().theme, element(s, 1)), true);
  s.select([4]); s.dispatch('duplicateCardFace', { id: 4 });
  const groups = s.getState().theme.elements.filter(e => e.isGroup);
  assert.equal(groups.length, 2);
  assert.equal(s.getState().theme.elements.filter(e => e.groupParent === groups[1].id).length, 2);
});

test('group layer drops move the whole block and members stay inside their group', () => {
  const s = setup(); s.select([]); s.dispatch('add', { widget: 'shape', x: 400, y: 200 });
  s.dispatch('reorderLayer', { id: 3, target: 4, before: true });
  assert.deepEqual(s.getState().theme.elements.map(e => e.id), [4, 3, 1, 2]);
  s.dispatch('reorderLayer', { id: 1, target: 4, before: true });
  assert.deepEqual(s.getState().theme.elements.map(e => e.id), [4, 3, 1, 2]);
  s.dispatch('reorderLayer', { id: 1, target: 2, before: true });
  assert.deepEqual(s.getState().theme.elements.map(e => e.id), [4, 3, 2, 1]);
  s.dispatch('reorder', { id: 2, index: 0 });
  assert.deepEqual(s.getState().theme.elements.map(e => e.id), [4, 3, 2, 1]);
});

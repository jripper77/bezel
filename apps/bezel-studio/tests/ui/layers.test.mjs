import { test } from 'node:test';
import assert from 'node:assert/strict';
import { commands, createStore } from '../../src/editor/store.js';
import { DEMO_THEME } from '../../src/demo-theme.js';
import { layerGroups } from '../../src/editor/layers.js';

test('layers group base and all faces without changing paint order', () => {
  const elements = [
    { id: 1 }, { id: 2, card: { faces: ['A', 'B', 'Empty'], activeFace: 1 } },
    { id: 3, cardMember: { parent: 2, face: null } },
    { id: 4, cardMember: { parent: 2, face: 0 } },
    { id: 5 }, { id: 6, cardMember: { parent: 2, face: 0 } },
    { id: 7, cardMember: { parent: 2, face: 1 } },
  ];
  const before = structuredClone(elements), groups = layerGroups(elements);
  assert.deepEqual(groups.map(g => g.element.id), [5, 2, 1]);
  assert.deepEqual(groups[1].groups.map(g => [g.face, g.elements.map(e => e.id)]), [[null, [3]], [0, [6, 4]], [1, [7]], [2, []]]);
  assert.deepEqual(elements, before);
});

test('invalid or orphan membership never hides an object from Layers', () => {
  const elements = [
    { id: 1, card: { faces: ['A'] } },
    { id: 2, cardMember: { parent: 99, face: 0 } },
    { id: 3, cardMember: { parent: 1, face: 3 } },
    { id: 4, cardMember: { parent: 1 } },
  ];
  assert.deepEqual(layerGroups(elements).map(g => g.element.id), [4, 3, 2, 1]);
});

test('relative drops use visible list order in both directions and avoid no-op Undo', () => {
  const theme = { ...DEMO_THEME, elements: [{ id: 1 }, { id: 2 }, { id: 3 }] };
  const store = createStore(theme);
  const saved = store.getState().theme;
  store.dispatch('reorderLayer', { id: 1, target: 3, before: true });
  assert.deepEqual(store.getState().theme.elements.map(e => e.id), [2, 3, 1]);
  store.undo(); assert.equal(store.getState().theme, saved);
  store.dispatch('reorderLayer', { id: 3, target: 1, before: false });
  assert.deepEqual(store.getState().theme.elements.map(e => e.id), [3, 1, 2]);
  store.undo();
  store.dispatch('reorderLayer', { id: 3, target: 2, before: true });
  assert.equal(store.getState().theme, saved); assert.equal(store.canUndo(), false);
});

test('card block drops preserve all faces and reject cross-group membership changes', () => {
  const theme = { ...DEMO_THEME, elements: [
    { id: 1, card: { faces: ['A', 'B'], activeFace: 0 } },
    { id: 2, cardMember: { parent: 1, face: null } },
    { id: 3, cardMember: { parent: 1, face: 0 } },
    { id: 4, cardMember: { parent: 1, face: 1 } },
    { id: 5 }, { id: 6, cardMember: { parent: 1, face: 0 } },
  ] };
  const move = args => commands.reorderLayer(theme, args).theme;
  assert.deepEqual(move({ id: 1, target: 5, before: true }).elements.map(e => e.id), [5, 1, 2, 3, 4, 6]);
  assert.deepEqual(move({ id: 5, target: 1, before: true }).elements.map(e => e.id), [1, 2, 3, 4, 6, 5]);
  const swapped = move({ id: 3, target: 6, before: true });
  assert.deepEqual(layerGroups(swapped.elements)[1].groups[1].elements.map(e => e.id), [3, 6]);
  assert.deepEqual(swapped.elements.find(e => e.id === 3).cardMember, { parent: 1, face: 0 });
  for (const target of [2, 4, 5, 99, 3]) assert.equal(move({ id: 3, target, before: true }), theme);
});

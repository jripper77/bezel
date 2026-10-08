import { test } from 'node:test';
import assert from 'node:assert/strict';
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

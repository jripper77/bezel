import { test } from 'node:test';
import assert from 'node:assert/strict';
import { commands, createStore, HISTORY_LIMIT, layerLabel, merge, selectedElements } from '../../src/editor/store.js';
import { DEMO_THEME } from '../../src/demo-theme.js';

const frame = (s, id) => s.getState().theme.elements.find((e) => e.id === id).frame;

test('add creates a centered, named, selected element with the next id', () => {
  const s = createStore(DEMO_THEME);
  s.dispatch('add', { widget: 'bar', x: 240, y: 1000, sensor: { key: 'gpu.usage', quantity: 'percent', label: 'GPU usage' } });
  const st = s.getState();
  const e = st.theme.elements.at(-1);
  assert.equal(e.id, 4);
  assert.equal(e.name, 'GPU usage');
  assert.equal(e.kind.binding.key, 'gpu.usage');
  assert.equal(Math.round(e.frame.x + e.frame.width / 2), 240);
  assert.deepEqual(st.selection, [4]);
  s.dispatch('add', { widget: 'bar', x: 0, y: 0, sensor: { label: 'GPU usage' } });
  assert.equal(s.getState().theme.elements.at(-1).name, 'GPU usage 2');
  s.dispatch('add', { widget: 'text', x: 10, y: 10 });
  assert.equal(s.getState().theme.elements.at(-1).name, 'Text');
  assert.throws(() => s.dispatch('nope'), /unknown command/);
  assert.equal(s.isDirty(), true);
  s.markSaved();
  assert.equal(s.isDirty(), false);
});

test('move skips locked elements and undo/redo walk the history', () => {
  const s = createStore(DEMO_THEME);
  s.dispatch('move', { ids: [1, 3], dx: 10.4, dy: -5 });
  assert.deepEqual(frame(s, 1), { x: 50, y: 75, width: 400, height: 120 });
  assert.deepEqual(frame(s, 3), DEMO_THEME.elements[2].frame, 'locked stays');
  assert.equal(s.canUndo(), true);
  s.undo();
  assert.deepEqual(frame(s, 1), DEMO_THEME.elements[0].frame);
  assert.equal(s.canRedo(), true);
  s.redo();
  assert.equal(frame(s, 1).x, 50);
  s.undo();
  s.undo();
  assert.equal(frame(s, 1).x, 40, 'undo past the start is a no-op');
  s.redo();
  s.redo();
  assert.equal(frame(s, 1).x, 50, 'redo past the end is a no-op');
});

test('a gesture is one undo step, and a gesture that returns home is none', () => {
  const s = createStore(DEMO_THEME);
  s.beginGesture();
  for (let i = 0; i < 5; i += 1) s.dispatch('move', { ids: [1], dx: 2, dy: 0 });
  s.endGesture();
  assert.equal(frame(s, 1).x, 50);
  s.undo();
  assert.equal(frame(s, 1).x, 40);
  assert.equal(s.canUndo(), false);
  s.beginGesture();
  s.dispatch('move', { ids: [1], dx: 5, dy: 0 });
  s.dispatch('move', { ids: [1], dx: -5, dy: 0 });
  s.endGesture();
  assert.equal(s.canUndo(), false);
});

test('setFrame rounds, update merges, setKind replaces', () => {
  const s = createStore(DEMO_THEME);
  s.dispatch('setFrame', { id: 1, frame: { x: 1.4, y: 2.6, width: 100.2, height: 0 } });
  assert.deepEqual(frame(s, 1), { x: 1, y: 3, width: 101, height: 4 }, 'edges are rounded, not the size');
  s.dispatch('update', { id: 1, patch: { kind: { style: { size: 50 } } } });
  const e = s.getState().theme.elements[0];
  assert.equal(e.kind.style.size, 50);
  assert.equal(e.kind.style.align, 'center', 'siblings kept');
  s.dispatch('setKind', { id: 2, kind: { type: 'bar', binding: { key: 'cpu.usage', min: 0, max: 100 }, direction: 'leftToRight', fill: '#fff', radius: 0 } });
  assert.equal(s.getState().theme.elements[1].kind.type, 'bar');
  s.dispatch('setTheme', { patch: { name: 'Mine', background: { color: '#000000ff' } } });
  assert.equal(s.getState().theme.name, 'Mine');
  assert.equal(s.getState().theme.background.type, 'color');
});

test('duplicate, remove and selection bookkeeping', () => {
  const s = createStore(DEMO_THEME);
  s.select([1, 3, 99]);
  assert.deepEqual(s.getState().selection, [1, 3]);
  assert.deepEqual(selectedElements(s.getState()).map((e) => e.id), [1, 3]);
  s.dispatch('duplicate', { ids: [1, 3] });
  const st = s.getState();
  assert.deepEqual(st.selection, [4, 5]);
  assert.equal(st.theme.elements[3].name, 'Clock copy');
  assert.equal(st.theme.elements[3].frame.x, 56);
  assert.equal(st.theme.elements[4].locked, false, 'copies are unlocked');
  s.dispatch('remove', { ids: [4, 5] });
  assert.equal(s.getState().theme.elements.length, 3);
  assert.deepEqual(s.getState().selection, []);
  s.select([2]);
  s.undo();
  assert.deepEqual(s.getState().selection, [2]);
});

test('reorder moves in the z-order and clamps', () => {
  const s = createStore(DEMO_THEME);
  s.dispatch('reorder', { id: 1, index: 99 });
  assert.deepEqual(s.getState().theme.elements.map((e) => e.id), [2, 3, 1]);
  s.dispatch('reorder', { id: 1, index: -3 });
  assert.deepEqual(s.getState().theme.elements.map((e) => e.id), [1, 2, 3]);
  const before = s.getState();
  s.dispatch('reorder', { id: 42, index: 0 });
  assert.equal(s.getState(), before, 'unknown id changes nothing');
});

test('align to the selection or the canvas, distribute evenly', () => {
  const t = structuredClone(DEMO_THEME);
  t.elements[2].locked = false;
  t.elements.forEach((e, i) => { e.frame = { x: 10 + i * 50, y: 10 * i, width: 40, height: 40 }; });
  t.elements[2].frame.x = 300;
  const s = createStore(t);
  s.dispatch('align', { ids: [1, 2, 3], edge: 'right' });
  assert.deepEqual(s.getState().theme.elements.map((e) => e.frame.x), [300, 300, 300]);
  s.dispatch('align', { ids: [1], edge: 'centerX' });
  assert.equal(frame(s, 1).x, 220, 'single element aligns to the canvas');
  for (const edge of ['left', 'top', 'bottom', 'centerY']) s.dispatch('align', { ids: [1, 2], edge });
  s.dispatch('align', { ids: [1], edge: 'diagonal' });
  s.dispatch('setFrame', { id: 1, frame: { x: 0, y: 0, width: 40, height: 40 } });
  s.dispatch('setFrame', { id: 2, frame: { x: 10, y: 0, width: 40, height: 40 } });
  s.dispatch('setFrame', { id: 3, frame: { x: 200, y: 0, width: 40, height: 40 } });
  s.dispatch('distribute', { ids: [1, 2, 3], axis: 'x' });
  assert.deepEqual(s.getState().theme.elements.map((e) => e.frame.x), [0, 100, 200]);
  const before = s.getState();
  s.dispatch('distribute', { ids: [1, 2], axis: 'y' });
  assert.equal(s.getState(), before, 'needs three');
  s.dispatch('align', { ids: [], edge: 'left' });
});

test('load resets history, subscribers hear every change, the history is bounded', () => {
  const s = createStore(DEMO_THEME);
  const heard = [];
  const off = s.subscribe((_, reason) => heard.push(reason));
  s.dispatch('move', { ids: [1], dx: 1, dy: 1 });
  s.select([1]);
  s.load(DEMO_THEME);
  assert.equal(s.canUndo(), false);
  assert.equal(s.isDirty(), false);
  off();
  s.dispatch('move', { ids: [1], dx: 1, dy: 1 });
  assert.deepEqual(heard, ['move', 'select', 'load']);
  for (let i = 0; i < HISTORY_LIMIT + 10; i += 1) s.dispatch('move', { ids: [1], dx: 1, dy: 0 });
  let steps = 0;
  while (s.canUndo()) { s.undo(); steps += 1; }
  assert.equal(steps, HISTORY_LIMIT);
});

test('merge and layer labels', () => {
  assert.deepEqual(merge({ a: { b: 1, c: 2 }, d: [1] }, { a: { b: 3 }, d: [2], e: undefined }), { a: { b: 3, c: 2 }, d: [2], e: undefined });
  assert.equal(merge({ a: 1 }, 5), 5);
  assert.deepEqual(merge(null, { a: 1 }), { a: 1 });
  assert.deepEqual(layerLabel(DEMO_THEME.elements[0]), { name: 'Clock', widget: 'clock' });
  assert.equal(commands.move(DEMO_THEME, { ids: [], dx: 0, dy: 0 }).theme.elements.length, 3);
});

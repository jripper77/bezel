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
  s.dispatch('move', { ids: [1], dx: 1, dy: 0 });
  assert.equal(s.isDirty(), true);
  s.undo();
  assert.equal(s.isDirty(), false, 'undo back to the saved theme is clean');
  s.redo();
  assert.equal(s.isDirty(), true);
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
  assert.equal(s.isGesturing(), false);
  s.beginGesture();
  assert.equal(s.isGesturing(), true);
  for (let i = 0; i < 5; i += 1) s.dispatch('move', { ids: [1], dx: 2, dy: 0 });
  s.endGesture();
  assert.equal(s.isGesturing(), false);
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
  s.dispatch('setTheme', { patch: { name: 'Mine', background: { type: 'image', asset: 'assets/a.png', fit: 'cover' } } });
  assert.equal(s.getState().theme.name, 'Mine');
  s.dispatch('setTheme', { patch: { background: { type: 'color', color: '#000000ff' } } });
  assert.deepEqual(s.getState().theme.background, { type: 'color', color: '#000000ff' }, 'replaced whole, no stale asset');
  s.dispatch('update', { id: 2, patch: { kind: { track: null } } });
  assert.equal(s.getState().theme.elements[1].kind.track, null, 'null clears an optional field');
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

test('align to the first selected object or the canvas, distribute evenly', () => {
  const t = structuredClone(DEMO_THEME);
  t.elements[2].locked = false;
  t.elements.forEach((e, i) => { e.frame = { x: 10 + i * 50, y: 10 * i, width: 40, height: 40 }; });
  t.elements[2].frame.x = 300;
  const s = createStore(t);
  s.dispatch('align', { ids: [1, 2, 3], edge: 'right' });
  assert.deepEqual(s.getState().theme.elements.map((e) => e.frame.x), [10, 10, 10]);
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

test('setOrientation between vertical and horizontal re-lays the elements in one undo step', () => {
  const s = createStore(DEMO_THEME);
  s.select([2]);
  s.dispatch('setOrientation', { orientation: 'reverse-landscape' });
  const { theme, selection } = s.getState();
  assert.equal(theme.orientation, 'reverse-landscape');
  assert.deepEqual(theme.canvas, { width: 1920, height: 480 });
  assert.deepEqual(theme.elements.map((e) => e.frame), [
    { x: 0, y: 180, width: 400, height: 120 },
    { x: 300, y: 90, width: 300, height: 300 },
    { x: 580, y: 140, width: 440, height: 200 },
  ], 'the vertical stack becomes a row: sizes kept, locked ones too, all inside');
  for (const { frame: f } of theme.elements) {
    assert.ok(f.x >= 0 && f.y >= 0 && f.x + f.width <= 1920 && f.y + f.height <= 480);
  }
  assert.deepEqual(selection, [2], 'the selection survives');
  assert.equal(s.isDirty(), true);
  s.undo();
  assert.equal(s.getState().theme.orientation, 'reverse-portrait');
  assert.deepEqual(s.getState().theme.canvas, DEMO_THEME.canvas);
  assert.deepEqual(s.getState().theme.elements.map((e) => e.frame), DEMO_THEME.elements.map((e) => e.frame));
  assert.equal(s.canUndo(), false, 'it was one step');
  assert.equal(s.isDirty(), false);
  s.redo();
  assert.deepEqual(s.getState().theme.canvas, { width: 1920, height: 480 });
});

test('setOrientation turning 180° keeps the layout; the same or an unknown one changes nothing', () => {
  const s = createStore(DEMO_THEME);
  const layout = s.getState().theme.elements;
  s.dispatch('setOrientation', { orientation: 'portrait' });
  const turned = s.getState().theme;
  assert.equal(turned.orientation, 'portrait');
  assert.deepEqual(turned.canvas, DEMO_THEME.canvas);
  assert.equal(turned.elements, layout, 'the layout is untouched');
  const before = s.getState();
  s.dispatch('setOrientation', { orientation: 'portrait' });
  s.dispatch('setOrientation', { orientation: 'sideways' });
  assert.equal(s.getState(), before);
  s.undo();
  assert.equal(s.getState().theme.orientation, 'reverse-portrait');
  assert.equal(s.canUndo(), false);
  const square = createStore({ ...DEMO_THEME, canvas: { width: 480, height: 480 }, elements: [] });
  square.dispatch('setOrientation', { orientation: 'landscape' });
  assert.deepEqual(square.getState().theme.canvas, { width: 480, height: 480 });
});

test('new elements and copies are named in the language the UI asks for', () => {
  const names = { widget: (w) => `W-${w}`, copy: (name) => `${name} (cópia)` };
  const s = createStore(DEMO_THEME, { names });
  s.dispatch('add', { widget: 'bar', x: 100, y: 100 });
  assert.equal(s.getState().theme.elements.at(-1).name, 'W-bar');
  s.dispatch('add', { widget: 'value', x: 100, y: 100, sensor: { key: 'cpu.usage', quantity: 'percent', label: 'Uso da CPU' } });
  assert.equal(s.getState().theme.elements.at(-1).name, 'Uso da CPU', 'a sensor names its element');
  s.dispatch('duplicate', { ids: [1] });
  assert.equal(s.getState().theme.elements.at(-1).name, 'Clock (cópia)');
});


test('clipboard snapshots multiple objects, preserves appearance and groups paste history', () => {
  const s = createStore(DEMO_THEME);
  s.select([2, 1]);
  const originals = structuredClone(selectedElements(s.getState()));
  s.copySelection();
  assert.equal(s.isDirty(), false);
  assert.equal(s.canUndo(), false);
  assert.equal(s.canPaste(), true);
  s.dispatch('update', { id: 1, patch: { opacity: 0.2, kind: { style: { color: '#abcdef' } } } });
  const before = s.getState().theme;
  s.paste();
  const copies = selectedElements(s.getState());
  assert.equal(copies.length, 2);
  for (const [i, e] of copies.entries()) {
    assert.deepEqual(e.kind, originals[i].kind);
    assert.equal(e.opacity, originals[i].opacity);
    assert.deepEqual(e.frame, { ...originals[i].frame, x: originals[i].frame.x + 16, y: originals[i].frame.y + 16 });
    assert.notEqual(e.id, originals[i].id);
    assert.notEqual(e.name, originals[i].name);
  }
  const pasted = s.getState().theme;
  s.undo();
  assert.equal(s.getState().theme, before);
  s.redo();
  assert.equal(s.getState().theme, pasted);
  s.paste();
  assert.equal(selectedElements(s.getState())[0].frame.x, originals[0].frame.x + 32);
  assert.equal(new Set(s.getState().theme.elements.map(e => e.name)).size, s.getState().theme.elements.length);
  s.load(DEMO_THEME);
  assert.equal(s.canPaste(), false);
  s.paste();
  assert.equal(s.isDirty(), false);
});

test('clipboard keeps SVG asset and clock options after the source is removed', () => {
  const theme = structuredClone(DEMO_THEME);
  theme.elements[0].kind = { type: 'image', asset: 'assets/tabler/fan.svg', fit: 'contain' };
  theme.elements[1].kind = { type: 'text', content: { type: 'clock', pattern: '%A', language: 'it', casing: 'upper' } };
  const s = createStore(theme);
  s.paste();
  assert.equal(s.canUndo(), false);
  s.select([1, 2]);
  s.copySelection();
  s.dispatch('remove', { ids: [1, 2] });
  s.paste();
  assert.deepEqual(selectedElements(s.getState()).map(e => e.kind), theme.elements.slice(0, 2).map(e => e.kind));
});

test('all six alignments use selection order, leave the anchor fixed and undo once',()=>{
 const original=structuredClone(DEMO_THEME);original.elements.forEach((e,i)=>{e.locked=false;e.frame={x:100+i*70,y:200+i*60,width:40+i*20,height:30+i*10};});
 for(const ids of [[1,3,2],[3,1,2]])for(const edge of ['left','right','top','bottom','centerX','centerY']){
  const s=createStore(original);s.select(ids);s.dispatch('align',{ids,edge});const anchor=frame(s,ids[0]);assert.deepEqual(anchor,original.elements.find(e=>e.id===ids[0]).frame);
  for(const id of ids.slice(1)){const f=frame(s,id);
   if(edge==='left')assert.equal(f.x,anchor.x);
   if(edge==='right')assert.equal(f.x+f.width,anchor.x+anchor.width);
   if(edge==='top')assert.equal(f.y,anchor.y);
   if(edge==='bottom')assert.equal(f.y+f.height,anchor.y+anchor.height);
   if(edge==='centerX')assert.equal(f.x+f.width/2,anchor.x+anchor.width/2);
   if(edge==='centerY')assert.equal(f.y+f.height/2,anchor.y+anchor.height/2);
  }
  s.undo();assert.deepEqual(s.getState().theme,original);assert.deepEqual(s.getState().selection,ids);
 }
});
test('locked reference stays usable without aligning the other selection to the canvas',()=>{
 const t=structuredClone(DEMO_THEME);const s=createStore(t);s.dispatch('align',{ids:[3,1],edge:'left'});assert.equal(frame(s,1).x,t.elements[2].frame.x);assert.deepEqual(frame(s,3),t.elements[2].frame);
 const anchor=frame(s,1);s.dispatch('align',{ids:[1,3],edge:'top'});assert.deepEqual(frame(s,1),anchor);assert.deepEqual(frame(s,3),t.elements[2].frame);
});

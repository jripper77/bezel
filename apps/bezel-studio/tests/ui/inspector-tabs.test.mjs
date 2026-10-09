import { test } from 'node:test';
import assert from 'node:assert/strict';
import { CARD_TABS, OBJECT_TABS, PREVIEW_MAX_SCALE, createTabMemory, previewCrop, tabAfterKey, tabsOf } from '../../src/inspector-tabs.js';

test('a card has faces, motion, triggers and look; every other object data and look', () => {
  assert.deepEqual(tabsOf('card'), ['faces', 'motion', 'triggers', 'look']);
  assert.equal(tabsOf('card'), CARD_TABS);
  for (const kind of ['ring', 'graph', 'text', 'group', 'shape']) assert.equal(tabsOf(kind), OBJECT_TABS);
  assert.deepEqual(OBJECT_TABS, ['data', 'look']);
  assert.ok(Object.isFrozen(CARD_TABS) && Object.isFrozen(OBJECT_TABS));
});

test('a kind opens on its first offered tab until one is chosen', () => {
  const memory = createTabMemory();
  assert.equal(memory.current('ring', OBJECT_TABS), 'data');
  assert.equal(memory.current('shape', ['look']), 'look');
  assert.equal(memory.current('card', CARD_TABS), 'faces');
  assert.equal(memory.current('ring', []), null);
});

test('the chosen tab stays open for its kind only, through any number of renders', () => {
  const memory = createTabMemory();
  memory.choose('ring', 'look');
  for (let i = 0; i < 3; i++) assert.equal(memory.current('ring', OBJECT_TABS), 'look');
  assert.equal(memory.current('graph', OBJECT_TABS), 'data');
  memory.choose('card', 'triggers');
  assert.equal(memory.current('card', CARD_TABS), 'triggers');
  assert.equal(memory.current('ring', OBJECT_TABS), 'look');
});

test('a chosen tab no longer offered falls back to the first, and comes back when offered again', () => {
  const memory = createTabMemory();
  memory.choose('shape', 'data');
  assert.equal(memory.current('shape', ['look']), 'look');
  assert.equal(memory.current('shape', OBJECT_TABS), 'data');
});

test('arrow keys step through the tabs and wrap; Home and End go to the ends', () => {
  assert.equal(tabAfterKey(CARD_TABS, 'faces', 'ArrowRight'), 'motion');
  assert.equal(tabAfterKey(CARD_TABS, 'look', 'ArrowRight'), 'faces');
  assert.equal(tabAfterKey(CARD_TABS, 'faces', 'ArrowLeft'), 'look');
  assert.equal(tabAfterKey(CARD_TABS, 'triggers', 'ArrowLeft'), 'motion');
  assert.equal(tabAfterKey(CARD_TABS, 'triggers', 'Home'), 'faces');
  assert.equal(tabAfterKey(CARD_TABS, 'motion', 'End'), 'look');
  assert.equal(tabAfterKey(OBJECT_TABS, 'unknown', 'ArrowRight'), 'look');
});

test('other keys and an empty tab list move nowhere', () => {
  for (const key of ['ArrowUp', 'ArrowDown', 'Enter', 'Tab', 'a']) assert.equal(tabAfterKey(OBJECT_TABS, 'data', key), null);
  assert.equal(tabAfterKey([], 'data', 'ArrowRight'), null);
});

test('the preview copies the object and fits it in the box, keeping its shape', () => {
  const canvas = { width: 480, height: 1920 };
  assert.deepEqual(previewCrop({ x: 40, y: 100, width: 400, height: 200 }, canvas, { width: 260, height: 120 }),
    { sx: 40, sy: 100, sw: 400, sh: 200, width: 240, height: 120 });
  assert.deepEqual(previewCrop({ x: 0, y: 0, width: 480, height: 100 }, canvas, { width: 240, height: 120 }),
    { sx: 0, sy: 0, sw: 480, sh: 100, width: 240, height: 50 });
});

test('a small object is enlarged at most four times', () => {
  const crop = previewCrop({ x: 10, y: 10, width: 20, height: 10 }, { width: 480, height: 480 }, { width: 260, height: 120 });
  assert.equal(PREVIEW_MAX_SCALE, 4);
  assert.deepEqual(crop, { sx: 10, sy: 10, sw: 20, sh: 10, width: 80, height: 40 });
});

test('the part off the canvas is cut away, fractions widen to whole pixels', () => {
  const canvas = { width: 100, height: 100 };
  assert.deepEqual(previewCrop({ x: -20, y: 90, width: 60, height: 40 }, canvas, { width: 400, height: 400 }),
    { sx: 0, sy: 90, sw: 40, sh: 10, width: 160, height: 40 });
  assert.deepEqual(previewCrop({ x: 10.5, y: 20.2, width: 10, height: 9.6 }, canvas, { width: 22, height: 400 }),
    { sx: 10, sy: 20, sw: 11, sh: 10, width: 22, height: 20 });
});

test('nothing to preview off the canvas, with no size or with an empty box', () => {
  const canvas = { width: 100, height: 100 };
  assert.equal(previewCrop({ x: 120, y: 0, width: 50, height: 50 }, canvas, { width: 200, height: 100 }), null);
  assert.equal(previewCrop({ x: 0, y: -80, width: 50, height: 50 }, canvas, { width: 200, height: 100 }), null);
  assert.equal(previewCrop({ x: 10, y: 10, width: 0, height: 50 }, canvas, { width: 200, height: 100 }), null);
  assert.equal(previewCrop({ x: 10, y: 10, width: 50, height: 50 }, canvas, { width: 0, height: 100 }), null);
});

test('a sliver is drawn at least one pixel tall', () => {
  const crop = previewCrop({ x: 0, y: 0, width: 1000, height: 1 }, { width: 1000, height: 100 }, { width: 100, height: 100 });
  assert.equal(crop.height, 1);
  assert.equal(crop.width, 100);
});

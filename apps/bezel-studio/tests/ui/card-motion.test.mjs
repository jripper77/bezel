import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createCardMotion, cardPoses } from '../../src/editor/card-motion.js';
import { createStore } from '../../src/editor/store.js';
import { DEMO_THEME } from '../../src/demo-theme.js';
function scene() { const s = createStore({ ...DEMO_THEME, elements: [] }); s.dispatch('cardDemo', { x: 240, y: 480 }); return s; }
test('motion starts on a face change, finishes, and has no serialized progress', () => {
  const s = scene(), tracker = createCardMotion();
  assert.equal(tracker.update(s.getState().theme, 0).size, 0);
  s.dispatch('cardFace', { id: 1, face: 1 });
  assert.equal(tracker.update(s.getState().theme, 10).get(1).progress, 0);
  assert.equal(tracker.update(s.getState().theme, 335).get(1).progress, 0.5);
  assert.equal(tracker.update(s.getState().theme, 660).size, 0);
  assert.deepEqual(Object.keys(s.getState().theme.elements[0].card).sort(), ['activeFace', 'faces', 'transition']);
});
test('settings and face changed in one render still animate; reduced motion snaps', () => {
  const s = scene(), tracker = createCardMotion(); tracker.update(s.getState().theme, 0);
  s.dispatch('update', { id: 1, patch: { card: { transition: { effect: 'slide' } } } });
  s.dispatch('cardFace', { id: 1, face: 1 });
  assert.equal(tracker.update(s.getState().theme, 100).get(1).settings.effect, 'slide');
  assert.equal(tracker.update(s.getState().theme, 200, false).size, 0);
  assert.equal(tracker.update(s.getState().theme, 300, true).size, 0);
});
test('interruption chooses the visible flip face and never queues changes', () => {
  const s = scene(), tracker = createCardMotion(); tracker.update(s.getState().theme, 0);
  s.dispatch('cardFace', { id: 1, face: 1 }); tracker.update(s.getState().theme, 100);
  s.dispatch('cardFace', { id: 1, face: 0 }); assert.equal(tracker.update(s.getState().theme, 200).size, 0);
  s.dispatch('cardFace', { id: 1, face: 1 }); tracker.update(s.getState().theme, 300);
  assert.equal(tracker.update(s.getState().theme, 500).get(1).to, 1);
  assert.equal(tracker.update(s.getState().theme, 1000).size, 0);
});
test('flip maintains positive scale and a fixed center in all directions', () => {
  const b = { x: 40, y: 90, width: 200, height: 180 }, cx = 140, cy = 180;
  for (const direction of ['left', 'right', 'up', 'down']) for (const progress of [0, 0.25, 0.5, 0.75, 1]) {
    const pose = cardPoses({ from: 0, to: 1, progress, settings: { effect: 'flip', direction } }, b)[0];
    const [a, skewY, skewX, d, x, y] = pose.matrix;
    assert.ok(a > 0 && d > 0);
    assert.ok(Math.abs(a * cx + skewX * cy + x - cx) < 0.2);
    assert.ok(Math.abs(skewY * cx + d * cy + y - cy) < 0.2);
    assert.equal(pose.face, progress < 0.5 ? 0 : 1);
  }
});
test('demo config and face animation survive clipboard, deletion and Undo', () => {
  const s = scene(); s.copySelection(); s.paste();
  const copied = s.getState().theme.elements.find(e => e.id === 6);
  assert.equal(copied.card.transition.effect, 'flip');
  s.dispatch('removeCardFace', { id: 6 });
  assert.deepEqual(s.getState().theme.elements.find(e => e.id === 6).card.transition, copied.card.transition);
  s.undo(); assert.equal(s.getState().theme.elements.find(e => e.id === 6).card.faces.length, 2);
});

test('flip adds perspective instead of shearing the face', () => {
  const b = { x: 40, y: 90, width: 200, height: 180 };
  const p = cardPoses({ from: 0, to: 1, progress: 0.25, settings: { effect: 'flip', direction: 'left' } }, b)[0];
  assert.equal(p.matrix[1], 0); assert.equal(p.matrix[2], 0);
  assert.ok(p.projection.depth < 0);
  assert.ok(p.shade > 0.9);
  assert.equal(p.projection.cx, 140); assert.equal(p.projection.cy, 180);
});

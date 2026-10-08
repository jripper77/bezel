import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createCardTimers } from '../../src/editor/card-timer.js';
import { createCardMotion } from '../../src/editor/card-motion.js';
const scene = () => ({ elements: [{ id: 1, visible: true, card: { faces: ['A', 'B'], activeFace: 0, rotationSeconds: 5, transition: { effect: 'flip', direction: 'left', durationMs: 750 } } }] });
test('timer switches transient faces, schedules transitions, and preserves the document', () => {
  const theme = scene(), before = structuredClone(theme), timer = createCardTimers(), motion = createCardMotion();
  const first = timer.update(theme, 0); assert.equal(first.nextMs, 5000); motion.update(first.theme, 0);
  const switched = timer.update(theme, 5000); assert.equal(switched.theme.elements[0].card.activeFace, 1);
  assert.equal(motion.update(switched.theme, 5000).get(1).progress, 0);
  assert.equal(timer.update(theme, 10000).theme.elements[0].card.activeFace, 0);
  assert.deepEqual(theme, before);
});
test('editing pauses, resumes with a full interval, and skips queued transitions after a stall', () => {
  const theme = scene(), timer = createCardTimers(); timer.update(theme, 0); timer.update(theme, 5000);
  assert.equal(timer.update(theme, 6000, false).nextMs, null);
  assert.equal(timer.update(theme, 100000, false).theme, theme);
  assert.equal(timer.update(theme, 100001).nextMs, 5000);
  assert.equal(timer.update(theme, 500000).theme.elements[0].card.activeFace, 1);
  assert.equal(timer.update(theme, 500001).nextMs, 4999);
  assert.equal(timer.update(theme, 1).theme, theme);
});

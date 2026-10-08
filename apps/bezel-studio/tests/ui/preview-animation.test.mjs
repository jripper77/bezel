import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PREVIEW_FPS, createPreviewAnimation, nextAnimationDelay } from '../../src/preview-animation.js';

test('a still preview asks for no animation frame', () => {
  assert.equal(nextAnimationDelay({ nextMs: null, elapsed: 10 }), null);
  assert.equal(nextAnimationDelay({ nextMs: undefined, elapsed: 10 }), null);
});

test('the next frame comes when the GIF changes, counted from the render', () => {
  // A 100 ms GIF frame, a render that took 20 ms: 80 ms more.
  assert.equal(nextAnimationDelay({ nextMs: 100, elapsed: 20 }), 80);
  // Already past (the IPC was slow): at most the time the render took.
  assert.equal(nextAnimationDelay({ nextMs: 10, elapsed: 30 }), 1000 / PREVIEW_FPS - 30);
});

test('at most 15 preview frames a second, whatever the GIF', () => {
  assert.equal(PREVIEW_FPS, 15);
  const delay = nextAnimationDelay({ nextMs: 20, elapsed: 5 });
  assert.ok(Math.abs(delay - (1000 / 15 - 5)) < 1e-9, `${delay}`);
  assert.equal(nextAnimationDelay({ nextMs: 0, elapsed: 0, perSecond: 10 }), 100);
});

test('a slow render does not add another full render duration of waiting', () => {
  // A full-screen GIF that takes 150 ms to render and send over the IPC.
  assert.equal(nextAnimationDelay({ nextMs: 100, elapsed: 150 }), 0);
});

/** An animation over a fake timer list. */
function harness({ enabled = true } = {}) {
  const h = { requests: 0, timers: [], cancelled: [], enabled };
  h.animation = createPreviewAnimation({
    request: () => { h.requests += 1; },
    enabled: () => h.enabled,
    wait: (ms, fn) => {
      const timer = { ms, fn };
      h.timers.push(timer);
      return timer;
    },
    cancel: (timer) => h.cancelled.push(timer),
  });
  h.fire = () => h.timers.shift().fn();
  return h;
}

test('a frame that changes asks for one render when due', () => {
  const h = harness();
  h.animation.shown({ nextMs: 100, elapsed: 20 });
  assert.equal(h.timers.length, 1);
  assert.equal(h.timers[0].ms, 80);
  assert.ok(h.animation.pending());
  h.fire();
  assert.equal(h.requests, 1);
  assert.ok(!h.animation.pending());
});

test('a newer frame replaces the waiting one; a still one ends it', () => {
  const h = harness();
  h.animation.shown({ nextMs: 100, elapsed: 0 });
  const first = h.timers[0];
  h.animation.shown({ nextMs: 100, elapsed: 10 });
  assert.deepEqual(h.cancelled, [first], 'one timer at a time');
  h.animation.shown({ nextMs: null, elapsed: 10 });
  assert.equal(h.cancelled.length, 2);
  assert.ok(!h.animation.pending());
  h.animation.stop();
  assert.equal(h.cancelled.length, 2, 'nothing left to cancel');
});

test('a hidden page or reduced motion gets no animation frame', () => {
  const h = harness({ enabled: false });
  h.animation.shown({ nextMs: 50, elapsed: 0 });
  assert.equal(h.timers.length, 0);
  h.enabled = true;
  h.animation.shown({ nextMs: 50, elapsed: 0 });
  assert.equal(h.timers.length, 1);
});

test('30 fps waits only for the remainder of the interval', () => {
  assert.equal(nextAnimationDelay({ nextMs: 33, elapsed: 25, perSecond: 30 }), 1000 / 30 - 25);
  assert.equal(nextAnimationDelay({ nextMs: 33, elapsed: 40, perSecond: 30 }), 0);
});

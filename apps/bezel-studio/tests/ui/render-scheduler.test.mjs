import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRenderScheduler } from '../../src/render-scheduler.js';

/** A scheduler over a fake clock whose renders finish when told to. */
function harness({ gesturing = false } = {}) {
  const h = { clock: 0, gesturing, starts: [], timers: [], finish: [] };
  h.scheduler = createRenderScheduler({
    render: () => {
      h.starts.push(h.clock);
      return new Promise((resolve) => { h.finish.push(resolve); });
    },
    gesturing: () => h.gesturing,
    now: () => h.clock,
    wait: (ms, fn) => h.timers.push({ at: h.clock + ms, fn }),
  });
  // Lets the pending promise callbacks run.
  h.settle = () => new Promise((resolve) => { setImmediate(resolve); });
  h.complete = async () => {
    h.finish.shift()();
    await h.settle();
  };
  h.advance = async (ms) => {
    h.clock += ms;
    const due = h.timers.filter((t) => t.at <= h.clock);
    h.timers = h.timers.filter((t) => t.at > h.clock);
    for (const t of due) t.fn();
    await h.settle();
  };
  return h;
}

test('one render at a time; requests meanwhile merge into one', async () => {
  const h = harness();
  h.scheduler.request();
  await h.settle();
  h.scheduler.request();
  h.scheduler.request();
  h.scheduler.request();
  await h.settle();
  assert.equal(h.starts.length, 1, 'still running');
  await h.complete();
  assert.equal(h.starts.length, 2, 'the three requests became one render');
  await h.complete();
  assert.equal(h.starts.length, 2, 'nothing left to draw');
  assert.equal(h.timers.length, 0, 'no waiting outside a gesture');
});

test('while dragging, at most 30 renders start per second', async () => {
  const h = harness({ gesturing: true });
  // A pointer move every 5 ms for one second, renders taking 2 ms.
  for (let t = 0; t < 1000; t += 5) {
    h.scheduler.request();
    await h.settle();
    if (h.finish.length) await h.complete();
    await h.advance(5);
  }
  const gaps = h.starts.slice(1).map((s, i) => s - h.starts[i]);
  assert.ok(h.starts.length <= 31, `${h.starts.length} renders`);
  assert.ok(h.starts.length >= 25, `${h.starts.length} renders`);
  assert.ok(gaps.every((g) => g >= 1000 / 30 - 1e-9), gaps.join(' '));
});

test('the request that ends the gesture always renders', async () => {
  const h = harness({ gesturing: true });
  h.scheduler.request();
  await h.settle();
  await h.complete();
  h.scheduler.request();
  await h.settle();
  assert.equal(h.starts.length, 1, 'too soon during the drag');
  assert.equal(h.timers.length, 1, 'waits for its turn');
  // The drag ends before the turn comes: the final state still renders.
  h.gesturing = false;
  h.scheduler.request();
  await h.advance(10);
  assert.equal(h.starts.length, 1, 'the turn is kept');
  await h.advance(30);
  assert.equal(h.starts.length, 2, 'the final render');
  await h.complete();
  // After the gesture, requests render at once.
  h.scheduler.request();
  await h.settle();
  assert.equal(h.starts.length, 3);
});

test('a failed render does not stop the next ones', async () => {
  let calls = 0;
  const scheduler = createRenderScheduler({
    render: () => {
      calls += 1;
      return Promise.reject(new Error('boom'));
    },
    gesturing: () => false,
  });
  scheduler.request();
  await new Promise((resolve) => { setImmediate(resolve); });
  scheduler.request();
  await new Promise((resolve) => { setImmediate(resolve); });
  assert.equal(calls, 2);
});

test('the real clock and timer work by default', async () => {
  let calls = 0;
  let dragging = true;
  const scheduler = createRenderScheduler({ render: () => { calls += 1; }, gesturing: () => dragging, perSecond: 100 });
  scheduler.request();
  await new Promise((resolve) => { setImmediate(resolve); });
  scheduler.request();
  await new Promise((resolve) => { setTimeout(resolve, 40); });
  dragging = false;
  assert.equal(calls, 2);
});

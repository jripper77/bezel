// When the preview renders (D-2026-09-30-studio-app-2): one render at a
// time, and requests made while one runs merge into the next, which draws
// the latest state. While a gesture (a drag) is active, renders start at
// most `perSecond` times a second; the request that ends the gesture always
// renders, so the last frame is exact.

/**
 * @param {object} deps
 * @param {() => Promise<void>|void} deps.render draws the latest state (handles its own errors)
 * @param {() => boolean} deps.gesturing whether a drag is going on
 * @param {number} [deps.perSecond] most renders started per second during a gesture
 * @param {() => number} [deps.now] milliseconds
 * @param {(ms: number, fn: () => void) => void} [deps.wait]
 */
export function createRenderScheduler({ render, gesturing, perSecond = 30, now = () => performance.now(), wait = (ms, fn) => { setTimeout(fn, ms); } }) {
  const interval = 1000 / perSecond;
  let running = false;
  let waiting = false;
  let wanted = false;
  let lastStart = -Infinity;

  function pump() {
    if (running || waiting || !wanted) return;
    const early = gesturing() ? interval - (now() - lastStart) : 0;
    if (early > 0) {
      waiting = true;
      wait(early, () => {
        waiting = false;
        pump();
      });
      return;
    }
    wanted = false;
    running = true;
    lastStart = now();
    const done = () => {
      running = false;
      pump();
    };
    Promise.resolve()
      .then(render)
      .then(done, done);
  }

  return {
    /** Asks for a render of the latest state. */
    request() {
      wanted = true;
      pump();
    },
  };
}

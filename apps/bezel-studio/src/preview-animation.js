// The backend reports when the next animated preview frame is due. Timing
// starts before rendering/IPC: wait only for the remainder of that interval,
// rather than adding another render-duration pause. The render scheduler
// keeps one request in flight; hidden pages and reduced motion stop timers.

/** Default preview rate; the app supplies 30 for card transitions. */
export const PREVIEW_FPS = 15;

/**
 * How long to wait, once a preview frame is shown, before rendering the
 * next frame of its animated GIFs.
 * @param {object} shown
 * @param {number|null|undefined} shown.nextMs when the frame changes by itself, counted from its render (null: never)
 * @param {number} shown.elapsed milliseconds from the start of that render until now
 * @param {number} [shown.perSecond]
 * @returns {number|null} milliseconds, or null when nothing animates
 */
export function nextAnimationDelay({ nextMs, elapsed, perSecond = PREVIEW_FPS }) {
  if (nextMs === null || nextMs === undefined) return null;
  const left = nextMs - elapsed;
  const fastest = 1000 / perSecond - elapsed;
  return Math.max(0, left, fastest);
}

/**
 * Asks for preview renders at the pace of the GIFs they show.
 * @param {object} deps
 * @param {() => void} deps.request asks for a render of the latest state (the render scheduler's)
 * @param {() => boolean} [deps.enabled] false while the page is hidden or motion is reduced
 * @param {number} [deps.perSecond]
 * @param {(ms: number, fn: () => void) => unknown} [deps.wait]
 * @param {(timer: unknown) => void} [deps.cancel]
 */
export function createPreviewAnimation({
  request,
  enabled = () => true,
  perSecond = PREVIEW_FPS,
  wait = (ms, fn) => setTimeout(fn, ms),
  cancel = (timer) => clearTimeout(timer),
}) {
  let timer = null;

  function stop() {
    if (timer === null) return;
    cancel(timer);
    timer = null;
  }

  return {
    /**
     * A preview frame is on screen: `nextMs` from its render (null: it does
     * not change by itself), whose start was `elapsed` ms ago.
     */
    shown({ nextMs, elapsed }) {
      stop();
      if (!enabled()) return;
      const delay = nextAnimationDelay({ nextMs, elapsed, perSecond });
      if (delay === null) return;
      timer = wait(delay, () => {
        timer = null;
        request();
      });
    },
    /** Forgets the next animation frame (a failed render, a hidden page). */
    stop,
    /** Whether an animation frame is waiting. */
    pending: () => timer !== null,
  };
}

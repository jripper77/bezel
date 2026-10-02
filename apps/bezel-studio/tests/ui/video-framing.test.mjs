// The framing of a video background (D-2026-10-01-video-background-framing-2,
// -5, -6): its JSON (defaults left out, numbers kept in range), the geometry
// the canvas's framing mode moves with (the picture follows the pointer, the
// wheel zooms around it, arrows move it), one undo step per gesture, the
// strings in both languages, and the demo backend that plays the Dragon
// Ball-like video: Auto, a decoder of at most 15 pictures a second that
// stops when nobody asks, the poster with reduced motion or without ffmpeg,
// and the screen's names for a framed video.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  DEFAULT_FRAMING, DEFAULT_PAD, NUDGE, NUDGE_LARGE, ROTATIONS, WHEEL_IDLE_MS, ZOOM_STEP,
  applyFramingKey, clampPosition, clampZoom, compactFraming, createBurst, framingKey, framingOf, framingPercents, isPlainFraming,
  nudged, opaqueColor, panned, pictureBox, reframed, resolvedRotation, wheelPixels, wheelZoom, withFraming, withoutFraming, zoomedAt,
} from '../../src/editor/video-framing.js';
import { backgroundOf } from '../../src/editor/background.js';
import { createStore } from '../../src/editor/store.js';
import { LOCALES, placeholders } from '../../src/i18n/index.js';
import {
  DEMO_DECODER_IDLE_MS, DEMO_GUIDE_PAGES, DEMO_POSTER_MS, DEMO_VIDEO_FPS,
  createDemoBackend, createDemoDecoder, demoFramingSuffix, demoIsThemeVideo, demoPanelFor, demoThemeVideo, demoVideoAuto, demoVideoName,
} from '../../src/demo-backend.js';
import { DEMO_DRAGON_THEME, DEMO_THEME_VIDEOS, DEMO_VIDEO_THEME } from '../../src/demo-data.js';

const CANVAS = { width: 1920, height: 480 };
const DRAGON = { width: 480, height: 1920 };
const video = (framing) => ({ type: 'video', asset: 'assets/dragon.mp4', poster: 'assets/poster-195.png', ...(framing ? { framing } : {}) });
const close = (a, b, eps = 1e-6) => assert.ok(Math.abs(a - b) < eps, `${a} ≉ ${b}`);

test('a background without framing frames with the defaults: Auto, Fill, 100 %, centered, black', () => {
  assert.deepEqual(framingOf(video()), { rotation: null, fit: 'cover', zoom: 1, position: { x: 0.5, y: 0.5 }, padColor: DEFAULT_PAD });
  assert.deepEqual(framingOf(undefined), framingOf(video()));
  assert.deepEqual(DEFAULT_FRAMING.position, { x: 0.5, y: 0.5 });
  assert.deepEqual(ROTATIONS, [0, 90, 180, 270]);
});

test('numbers out of range are clamped and an unknown rotation or fit is the default', () => {
  const f = framingOf(video({ rotation: 45, fit: 'stretch', zoom: 9, position: { x: -1, y: 2 }, padColor: 'red' }));
  assert.deepEqual(f, { rotation: null, fit: 'cover', zoom: 4, position: { x: 0, y: 1 }, padColor: DEFAULT_PAD });
  assert.equal(clampZoom(0.2), 1);
  assert.equal(clampZoom(1.234), 1.23, 'whole percents');
  assert.equal(clampZoom(Number.NaN), 1);
  assert.equal(clampPosition(0.12345), 0.123, 'thousandths');
  assert.equal(clampPosition('x'), 0.5);
  assert.equal(opaqueColor('#FF000080'), '#ff0000ff', 'the pad is opaque');
  assert.equal(opaqueColor('#00ff00'), '#00ff00ff');
  assert.equal(opaqueColor(null), DEFAULT_PAD);
});

test('the theme keeps only what differs from the default, and no framing at all when nothing does', () => {
  assert.equal(compactFraming(framingOf(video())), null);
  assert.deepEqual(withFraming(video(), { zoom: 1.25 }), video({ zoom: 1.25 }));
  assert.deepEqual(withFraming(video(), { rotation: 270, fit: 'contain', zoom: 1.25, position: { y: 0.4 } }), video({ rotation: 270, fit: 'contain', zoom: 1.25, position: { x: 0.5, y: 0.4 } }));
  assert.deepEqual(withFraming(video({ zoom: 2 }), { zoom: 1 }), video(), 'back to the default: the key goes');
  assert.deepEqual(withFraming(video({ rotation: 90 }), { rotation: null }), video(), 'Auto is the key absent');
  assert.deepEqual(withFraming(video({ rotation: 0 }), {}), video({ rotation: 0 }), '0° is a choice, not Auto');
  assert.deepEqual(withFraming(video(), { padColor: '#123456' }), video({ padColor: '#123456ff' }));
  assert.deepEqual(withoutFraming(video({ zoom: 3, fit: 'contain' })), video());
  assert.deepEqual(reframed(video(), { zoom: 2 }), video({ zoom: 2 }));
  assert.equal(reframed(video({ zoom: 2 }), { zoom: 2 }), null, 'nothing changes: no undo step');
  assert.equal(reframed(video({ zoom: 1, position: { x: 0.5, y: 0.5 } }), {}), null, 'a spelled-out default is the default');
  assert.ok(isPlainFraming(framingOf(video({ rotation: 90, padColor: '#ffffffff' }))), 'only turned');
  assert.ok(!isPlainFraming(framingOf(video({ fit: 'contain' }))));
});

test('adding or replacing a video writes no framing', () => {
  const added = backgroundOf({ ref: 'assets/ferias.mp4', kind: 'video', poster: 'assets/ferias-poster.png' });
  assert.equal('framing' in added, false);
  const store = createStore({ ...DEMO_DRAGON_THEME, background: video({ zoom: 2 }) });
  store.dispatch('setTheme', { patch: { background: added } });
  assert.equal('framing' in store.getState().theme.background, false, 'the new video starts from Auto');
});

test('the rotation shown is the one chosen, else Auto\'s, else none', () => {
  assert.equal(resolvedRotation(framingOf(video()), { rotation: 270 }), 270);
  assert.equal(resolvedRotation(framingOf(video({ rotation: 0 })), { rotation: 270 }), 0);
  assert.equal(resolvedRotation(framingOf(video()), null), 0, 'Auto not known yet');
  assert.equal(resolvedRotation(framingOf(video()), { rotation: 45 }), 0);
});

test('the Dragon Ball video turned by Auto fills the landscape canvas exactly', () => {
  assert.deepEqual(pictureBox(DRAGON, 270, framingOf(video()), CANVAS), { x: 0, y: 0, width: 1920, height: 480 });
  // Not turned, Fill: the picture is 4 times the canvas, its middle shown (the user's report).
  assert.deepEqual(pictureBox(DRAGON, 0, framingOf(video()), CANVAS), { x: 0, y: -3600, width: 1920, height: 7680 });
  // Not turned, Fit: a strip in the middle, pad around it.
  assert.deepEqual(pictureBox(DRAGON, 0, framingOf(video({ fit: 'contain' })), CANVAS), { x: 900, y: 0, width: 120, height: 480 });
  // Fit placed at the left edge, at 200 %.
  assert.deepEqual(pictureBox(DRAGON, 180, framingOf(video({ fit: 'contain', zoom: 2, position: { x: 0, y: 0.5 } })), CANVAS), { x: 0, y: -240, width: 240, height: 960 });
  // Position picks the part shown when the picture overflows: the bottom.
  assert.deepEqual(pictureBox(DRAGON, 0, framingOf(video({ position: { x: 0.5, y: 1 } })), CANVAS).y, -7200);
  // An unknown size counts as the canvas's shape.
  assert.deepEqual(pictureBox(null, 90, framingOf(video({ zoom: 2 })), CANVAS), { x: -960, y: -240, width: 3840, height: 960 });
});

const view = (rotation = 0, source = DRAGON) => ({ source, rotation, canvas: CANVAS });

test('a drag moves the picture with the pointer, as far as it can go', () => {
  const start = framingOf(video({ zoom: 2 }));
  const box = pictureBox(DRAGON, 270, start, CANVAS);
  const position = panned(start, box, CANVAS, 100, -40);
  const after = pictureBox(DRAGON, 270, { ...start, position }, CANVAS);
  close(after.x, box.x + 100, 2);
  close(after.y, box.y - 40, 2);
  // An axis with no room keeps its position: Fill at 100 % fits the canvas exactly.
  const fitted = framingOf(video());
  assert.deepEqual(panned(fitted, pictureBox(DRAGON, 270, fitted, CANVAS), CANVAS, 300, 300), { x: 0.5, y: 0.5 });
  // Past the edge it stops there.
  assert.deepEqual(panned(start, box, CANVAS, 99_999, 99_999), { x: 0, y: 0 });
  // A smaller picture (Fit) moves the same way: right is right.
  const fit = framingOf(video({ fit: 'contain' }));
  const strip = pictureBox(DRAGON, 0, fit, CANVAS);
  const moved = pictureBox(DRAGON, 0, { ...fit, position: panned(fit, strip, CANVAS, 180, 0) }, CANVAS);
  close(moved.x, strip.x + 180, 2);
});

test('the wheel zooms around the pointer and keeps what is under it', () => {
  const start = framingOf(video());
  const point = { x: 480, y: 120 };
  const next = zoomedAt(start, view(270), 2, point);
  assert.equal(next.zoom, 2);
  const before = pictureBox(DRAGON, 270, start, CANVAS);
  const after = pictureBox(DRAGON, 270, next, CANVAS);
  close((point.x - before.x) / before.width, (point.x - after.x) / after.width, 1e-3);
  close((point.y - before.y) / before.height, (point.y - after.y) / after.height, 1e-3);
  assert.equal(zoomedAt(start, view(270), 9, point).zoom, 4);
  assert.equal(zoomedAt(next, view(270), 0.5, point).zoom, 1);
  assert.deepEqual(zoomedAt(next, view(270), 1, point).position, next.position, 'no room at 100 %: the position stays');
  assert.ok(wheelZoom(1, -100) > 1 && wheelZoom(2, 100) < 2, 'up zooms in');
  assert.equal(wheelZoom(4, -1000), 4);
  assert.equal(wheelPixels({ deltaY: 3, deltaMode: 1 }), 48, 'lines');
  assert.equal(wheelPixels({ deltaY: 1, deltaMode: 2 }), 400, 'pages');
  assert.equal(wheelPixels({ deltaY: -120 }), -120);
});

test('arrow keys move the picture their way by 1 %, 10 % with Shift', () => {
  const zoomed = framingOf(video({ zoom: 2 }));
  const right = nudged(zoomed, view(270), 1, 0);
  assert.ok(pictureBox(DRAGON, 270, right, CANVAS).x > pictureBox(DRAGON, 270, zoomed, CANVAS).x, 'the picture goes right');
  close(right.position.x, 0.5 - NUDGE);
  close(nudged(zoomed, view(270), 0, 1, true).position.y, 0.5 - NUDGE_LARGE);
  const fit = framingOf(video({ fit: 'contain' }));
  close(nudged(fit, view(0), 1, 0).position.x, 0.5 + NUDGE, 1e-9);
  assert.deepEqual(nudged(fit, view(0), 0, -1).position, fit.position, 'Fit fills the height: no room');
});

test('framing mode keys: arrows, + and -, 0, Esc and Enter; the rest goes on to the editor', () => {
  assert.deepEqual(framingKey({ key: 'ArrowLeft' }), { type: 'nudge', dx: -1, dy: 0, large: false });
  assert.deepEqual(framingKey({ key: 'ArrowDown', shiftKey: true }), { type: 'nudge', dx: 0, dy: 1, large: true });
  assert.deepEqual(framingKey({ key: 'ArrowUp' }), { type: 'nudge', dx: 0, dy: -1, large: false });
  assert.deepEqual(framingKey({ key: 'ArrowRight' }), { type: 'nudge', dx: 1, dy: 0, large: false });
  assert.deepEqual(framingKey({ key: '+' }), { type: 'zoom', step: 1 });
  assert.deepEqual(framingKey({ key: '=' }), { type: 'zoom', step: 1 });
  assert.deepEqual(framingKey({ key: '-' }), { type: 'zoom', step: -1 });
  assert.deepEqual(framingKey({ key: '0' }), { type: 'reset' });
  assert.deepEqual(framingKey({ key: 'Escape' }), { type: 'leave' });
  assert.deepEqual(framingKey({ key: 'Enter' }), { type: 'leave' });
  assert.equal(framingKey({ key: 'z', ctrlKey: true }), null, 'undo stays the editor\'s');
  assert.equal(framingKey({ key: '0', metaKey: true }), null);
  assert.equal(framingKey({ key: 'a' }), null);
  const start = framingOf(video({ zoom: 1.5, position: { x: 0.2, y: 0.7 } }));
  assert.equal(applyFramingKey(start, { type: 'zoom', step: 1 }, view(270)).zoom, 1.55);
  assert.equal(applyFramingKey(start, { type: 'zoom', step: -1 }, view(270)).zoom, 1.45);
  assert.equal(ZOOM_STEP, 0.05);
  assert.deepEqual(applyFramingKey(start, { type: 'reset' }, view(270)), { ...start, zoom: 1, position: { x: 0.5, y: 0.5 } });
  assert.equal(applyFramingKey(start, { type: 'leave' }, view(270)), start);
  assert.notDeepEqual(applyFramingKey(start, { type: 'nudge', dx: 1, dy: 0, large: false }, view(270)), start);
  assert.deepEqual(framingPercents(start), { zoom: 150, x: 20, y: 70 });
});

test('a burst of wheel turns is one gesture that ends after a pause', () => {
  const calls = [];
  const timers = [];
  const burst = createBurst({
    begin: () => calls.push('begin'),
    end: () => calls.push('end'),
    wait: (fn, ms) => { timers.push({ fn, ms, live: true }); return timers.length - 1; },
    cancel: (id) => { timers[id].live = false; },
  });
  burst.touch();
  burst.touch();
  burst.touch();
  assert.deepEqual(calls, ['begin']);
  assert.ok(burst.active());
  assert.deepEqual(timers.map((t) => t.ms), [WHEEL_IDLE_MS, WHEEL_IDLE_MS, WHEEL_IDLE_MS]);
  timers.filter((t) => t.live).forEach((t) => t.fn());
  assert.deepEqual(calls, ['begin', 'end']);
  assert.ok(!burst.active());
  burst.flush();
  assert.deepEqual(calls, ['begin', 'end'], 'nothing to end');
  burst.touch();
  burst.flush();
  assert.deepEqual(calls, ['begin', 'end', 'begin', 'end']);
  // The default timers work too.
  const real = createBurst({ begin: () => {}, end: () => {} });
  real.touch();
  real.flush();
  assert.ok(!real.active());
});

test('a drag, a wheel burst or a slider gesture is one undo step; each control change is one', () => {
  const store = createStore(DEMO_DRAGON_THEME);
  const frame = (patch) => store.dispatch('setTheme', { patch: { background: withFraming(store.getState().theme.background, patch) } });
  store.beginGesture();
  for (const zoom of [1.1, 1.3, 1.6]) frame({ zoom });
  store.endGesture();
  frame({ fit: 'contain' });
  frame({ rotation: 0 });
  assert.deepEqual(store.getState().theme.background.framing, { rotation: 0, fit: 'contain', zoom: 1.6 });
  store.undo();
  store.undo();
  assert.deepEqual(store.getState().theme.background.framing, { zoom: 1.6 });
  store.undo();
  assert.equal('framing' in store.getState().theme.background, false);
  assert.ok(!store.canUndo() && !store.isDirty());
  store.redo();
  assert.deepEqual(store.getState().theme.background.framing, { zoom: 1.6 });
  // A gesture that ends where it began leaves no step behind.
  store.beginGesture();
  frame({ zoom: 2 });
  frame({ zoom: 1.6 });
  store.endGesture();
  store.undo();
  assert.equal('framing' in store.getState().theme.background, false);
});

test('every framing text exists in pt-BR and en with the same placeholders', () => {
  const keys = Object.keys(LOCALES.en).filter((k) => k.startsWith('framing.'));
  assert.ok(keys.length >= 25);
  for (const key of keys) {
    assert.ok(key in LOCALES['pt-BR'], key);
    assert.deepEqual(placeholders(LOCALES['pt-BR'][key]), placeholders(LOCALES.en[key]), key);
  }
  assert.equal(LOCALES['pt-BR']['framing.onCanvas'], 'Enquadrar no canvas');
  assert.equal(LOCALES.en['framing.onCanvas'], 'Frame on canvas');
});

// ------------------------------------------------------------------ demo --
const KEY = '/dev/ttyACM1';
const PANEL = { width: 480, height: 1920 };

test('Auto takes a panel-native video in a turned theme as already turned', () => {
  const dragon = structuredClone(DEMO_DRAGON_THEME);
  assert.deepEqual(demoVideoAuto(dragon, DRAGON, PANEL), { rotation: 270, size: DRAGON });
  assert.deepEqual(demoVideoAuto({ ...dragon, orientation: 'reverse-landscape' }, DRAGON, PANEL).rotation, 90);
  assert.deepEqual(demoVideoAuto({ ...dragon, orientation: 'portrait', canvas: PANEL }, DRAGON, PANEL).rotation, 0, 'half a turn: as it is');
  assert.deepEqual(demoVideoAuto(dragon, { width: 1920, height: 1080 }, PANEL), { rotation: 0, size: { width: 1920, height: 1080 } });
  assert.deepEqual(demoVideoAuto(dragon, DRAGON, null).rotation, 0, 'no panel known');
  assert.deepEqual(demoVideoAuto(dragon, null, PANEL), { rotation: 0, size: null });
  assert.deepEqual(demoVideoAuto({ ...dragon, background: { type: 'color', color: '#000000ff' } }, DRAGON, PANEL), { rotation: 0, size: null });
  assert.deepEqual(demoPanelFor(dragon), PANEL, 'the 8.8"\'s panel, from the canvas');
  assert.deepEqual(demoPanelFor({ canvas: { width: 1000, height: 300 }, orientation: 'landscape' }), null);
  assert.deepEqual(demoPanelFor(dragon, { width: 320, height: 480, name: 'x' }), { width: 320, height: 480 }, 'the live screen\'s');
});

test('the demo answers video_auto like the backend, for the theme as edited', async () => {
  const demo = createDemoBackend('dragon');
  const { theme } = await demo.session();
  assert.deepEqual(await demo.videoAuto(theme), { rotation: 270, size: { width: 480, height: 1920 } });
  await demo.setLive(true, KEY);
  assert.deepEqual((await demo.videoAuto(theme)).rotation, 270, 'the live 8.8"');
  assert.deepEqual(await demo.videoAuto({ ...theme, background: { type: 'video', asset: 'assets/unknown.mp4' } }), { rotation: 0, size: null });
  assert.deepEqual(await demo.videoAuto({ ...theme, orientation: 'portrait', canvas: PANEL }), { rotation: 0, size: { width: 480, height: 1920 } });
  const assets = await demo.assets();
  assert.deepEqual(assets.find((a) => a.ref === 'assets/dragon.mp4'), {
    ref: 'assets/dragon.mp4', kind: 'video', animated: false, dataUrl: null, poster: 'assets/poster-195.png', bytes: 2_588_343, durationMs: 10_200,
  });
  assert.match(assets.find((a) => a.ref === 'assets/poster-195.png').dataUrl, /^data:image\/svg\+xml,/);
  // A video added later is known by its own size (the demo's ferias.mp4: 1920x1080).
  const added = await demo.addMedia();
  assert.deepEqual((await demo.videoAuto({ ...theme, background: { type: 'video', asset: added.ref } })).size, { width: 1920, height: 1080 });
});

/** Fake timers for the decoder: `run()` fires what is due. */
function fakeTimers() {
  const pending = new Map();
  let next = 0;
  return {
    wait: (fn, ms) => { next += 1; pending.set(next, { fn, ms }); return next; },
    cancel: (id) => pending.delete(id),
    pending: () => [...pending.values()],
    run() {
      const due = [...pending.values()];
      pending.clear();
      due.forEach((t) => t.fn());
    },
  };
}

test('the preview decoder gives at most 15 pictures a second and stops when none is asked for', () => {
  const timers = fakeTimers();
  const states = [];
  const decoder = createDemoDecoder({ wait: timers.wait, cancel: timers.cancel, onState: (s) => states.push(s) });
  assert.equal(decoder.playing(), null);
  const first = decoder.picture('assets/dragon.mp4', 10_200, 5_000);
  assert.deepEqual(first, { ms: 0, nextMs: Math.ceil(1000 / DEMO_VIDEO_FPS) });
  assert.deepEqual(decoder.picture('assets/dragon.mp4', 10_200, 5_020), { ms: 20, nextMs: 47 });
  assert.equal(decoder.picture('assets/dragon.mp4', 10_200, 15_300).ms, 100, 'it loops');
  assert.deepEqual(states, ['running']);
  assert.deepEqual(timers.pending().map((t) => t.ms), [DEMO_DECODER_IDLE_MS], 'one idle timer, renewed by each picture');
  timers.run();
  assert.deepEqual(states, ['running', 'stopped']);
  assert.equal(decoder.playing(), null);
  // It resumes from the clock where the video would be.
  assert.equal(decoder.picture('assets/dragon.mp4', 10_200, 7_000).ms, 2_000);
  // Another video restarts it from its own start.
  assert.equal(decoder.picture('assets/other.mp4', 4_000, 8_000).ms, 0);
  assert.equal(decoder.playing(), 'assets/other.mp4');
  decoder.stop();
  decoder.stop();
  assert.deepEqual(states, ['running', 'stopped', 'running', 'running', 'stopped']);
  // Its default timers work too.
  const real = createDemoDecoder();
  real.picture('a.mp4', 1000, 0);
  real.stop();
  assert.equal(real.playing(), null);
});

/** An OffscreenCanvas stand-in for node: records the turns and pictures drawn. */
function fakeCanvases() {
  const log = [];
  class FakeCanvas {
    constructor(width, height) {
      this.width = width;
      this.height = height;
    }

    getContext() {
      const { width, height } = this;
      return new Proxy({}, {
        get: (_, name) => {
          if (name === 'getImageData') return () => ({ data: new Uint8ClampedArray(width * height * 4) });
          if (name === 'createLinearGradient') return () => ({ addColorStop() {} });
          if (name === 'rotate' || name === 'drawImage') return (...args) => log.push([name, ...args.map((a) => (typeof a === 'number' ? Math.round(a * 1000) / 1000 : `${a.width}x${a.height}`))]);
          return () => {};
        },
        set: () => true,
      });
    }
  }
  return { FakeCanvas, log };
}

test('the demo preview plays the video turned by Auto, and its poster without motion or ffmpeg', async (t) => {
  const { FakeCanvas, log } = fakeCanvases();
  const saved = globalThis.OffscreenCanvas;
  globalThis.OffscreenCanvas = FakeCanvas;
  t.after(() => { globalThis.OffscreenCanvas = saved; });
  const timers = fakeTimers();
  const states = [];
  let seconds = 100;
  const demo = createDemoBackend('dragon', { now: () => seconds, wait: timers.wait, cancel: timers.cancel }, { onDecoder: (s) => states.push(s) });
  const { theme } = await demo.session();

  const playing = await demo.render(theme);
  assert.deepEqual([playing.width, playing.height], [1920, 480]);
  assert.ok(playing.nextMs >= 1 && playing.nextMs <= Math.ceil(1000 / DEMO_VIDEO_FPS), `${playing.nextMs}`);
  assert.equal(demo.decoding(), 'assets/dragon.mp4');
  // The stored picture (turned for the panel) is drawn turned back: 270°, filling the canvas.
  assert.ok(log.some(([name, angle]) => name === 'rotate' && angle === Math.round(((270 * Math.PI) / 180) * 1000) / 1000));
  assert.deepEqual(log.filter(([name]) => name === 'drawImage').at(-1), ['drawImage', '480x1920', -240, -960, 480, 1920]);
  seconds += 0.5;
  await demo.render({ ...theme, background: withFraming(theme.background, { zoom: 2 }) });
  assert.deepEqual(states, ['running'], 'framing edits keep the decoder');

  // Reduced motion (or a hidden window): the poster, no next picture; the decoder idles out.
  const still = await demo.render(theme, { motion: false });
  assert.equal(still.nextMs, null);
  timers.run();
  assert.deepEqual(states, ['running', 'stopped']);
  assert.equal(demo.decoding(), null);
  await demo.render(theme, { motion: false });
  assert.deepEqual(states, ['running', 'stopped'], 'no decoder starts');

  // A theme with a GIF and the video: the first due wins.
  const gif = { id: 9, name: 'g', frame: { x: 0, y: 0, width: 10, height: 10 }, visible: true, kind: { type: 'image', asset: 'assets/x.gif', fit: 'contain' } };
  const both = await demo.render({ ...theme, elements: [...theme.elements, gif] }, { motion: false });
  assert.ok(both.nextMs !== null && both.nextMs <= 100, 'the GIF still says when it changes');

  // Without ffmpeg the poster shows and nothing decodes; a video without a poster shows black.
  const bare = createDemoBackend('dragonNoFfmpeg', { now: () => seconds, wait: timers.wait, cancel: timers.cancel }, { onDecoder: (s) => states.push(s) });
  assert.equal((await bare.render(theme)).nextMs, null);
  assert.equal(bare.decoding(), null);
  const drawn = log.length;
  assert.equal((await bare.render({ ...theme, background: { type: 'video', asset: 'assets/dragon.mp4' } })).nextMs, null);
  assert.equal(log.slice(drawn).filter(([name]) => name === 'drawImage').length, 0, 'nothing to draw');
  // An unknown video plays as the canvas's shape; a color background draws no picture.
  await demo.render({ ...theme, background: { type: 'video', asset: 'assets/unknown.mp4', framing: { rotation: 90 } } });
  assert.deepEqual(log.filter(([name]) => name === 'drawImage').at(-1), ['drawImage', '480x1920', -240, -960, 480, 1920]);
  assert.equal((await demo.render(structuredClone(DEMO_VIDEO_THEME), { motion: false })).nextMs, null);
  assert.equal((await demo.render({ ...theme, background: { type: 'color', color: '#000000ff' } })).nextMs, null);
  assert.equal(DEMO_POSTER_MS, 2000);
});

test('the screen\'s name for a theme video follows Auto and the framing', () => {
  const dragon = structuredClone(DEMO_DRAGON_THEME);
  const auto = { rotation: 270, size: DRAGON };
  const info = DEMO_THEME_VIDEOS['assets/dragon.mp4'];
  assert.deepEqual(demoThemeVideo(dragon, auto, info), { turns: 0, asIs: true, name: 'dragon.mp4' }, 'the vendor\'s name, sent as it is');
  assert.equal(demoVideoName(dragon), 'dragon_90.mp4', 'Auto unknown: turned with the theme');
  assert.equal(demoVideoName({ ...dragon, background: withFraming(dragon.background, { rotation: 0 }) }, auto), 'dragon_90.mp4');
  const zoomed = { ...dragon, background: withFraming(dragon.background, { zoom: 1.25 }) };
  const { name, asIs } = demoThemeVideo(zoomed, auto, info);
  assert.match(name, /^dragon_f[0-9a-f]{8}\.mp4$/);
  assert.equal(asIs, false, 'another framing is converted');
  assert.equal(demoFramingSuffix(framingOf(zoomed.background)), name.slice('dragon'.length, -'.mp4'.length));
  assert.notEqual(demoFramingSuffix(framingOf(video({ fit: 'contain' }))), demoFramingSuffix(framingOf(video({ fit: 'contain', padColor: '#ffffff' }))), 'the pad shows with Fit');
  assert.equal(demoFramingSuffix(framingOf(video({ padColor: '#ffffff' }))), '', 'not with Fill');
  assert.equal(demoVideoName(DEMO_VIDEO_THEME), 'nebula_90.mp4');
  for (const file of ['dragon.mp4', 'DRAGON_90.mp4', 'dragon_270_f0a1b2c3d.mp4', 'dragon_f12345678.mp4']) assert.ok(demoIsThemeVideo('assets/dragon.mp4', file), file);
  for (const file of ['dragon2.mp4', 'dragon_45.mp4', 'dragon_fxyz.mp4', 'nebula.mp4']) assert.ok(!demoIsThemeVideo('assets/dragon.mp4', file), file);
  assert.ok(demoIsThemeVideo('assets/Nebula Azul.mp4', 'nebula_azul_90.mp4'));
});

test('live, the screen\'s own dragon.mp4 plays the Dragon Ball theme; a re-framed one is missing', async () => {
  const instant = { now: () => 1000, delay: () => Promise.resolve() };
  const demo = createDemoBackend('dragon', instant);
  // The theme's video is protected under every name it may have.
  const files = (await demo.managerOverview(KEY)).files;
  assert.equal(files.find((f) => f.name === 'dragon.mp4').protected, 'themeVideo');
  await demo.setLive(true, KEY);
  assert.deepEqual((await demo.sample()).video, { state: 'onDevice', path: 'internal/video/dragon.mp4' });
  const { theme } = await demo.session();
  await demo.pushTheme({ ...theme, background: withFraming(theme.background, { zoom: 1.25 }) });
  const missing = (await demo.sample()).video;
  assert.equal(missing.state, 'missing');
  assert.match(missing.path, /^sd\/video\/dragon_f[0-9a-f]{8}\.mp4$/);
  const ready = await demo.prepareThemeVideo(KEY);
  assert.deepEqual([ready.convert.quarterTurns, ready.dimensions], [0, { width: 480, height: 1920 }], 'converted, with no turn');
  // The same name with other bytes is another file: the vendor's copy only counts with the asset's bytes.
  await demo.pushTheme(theme);
  await demo.setLive(false, KEY);
  await demo.deleteStored(KEY, 'internal/video/dragon.mp4', true);
  await demo.setLive(true, KEY);
  assert.deepEqual((await demo.sample()).video, { state: 'missing', path: 'sd/video/dragon.mp4' });
  const asIs = await demo.prepareThemeVideo(KEY);
  assert.deepEqual([asIs.convert, asIs.bytes, asIs.target.path], [null, 2_588_343, 'sd/video/dragon.mp4'], 'sent as it is');
});

test('the guide opens only its own pages', async () => {
  const opened = [];
  const demo = createDemoBackend('dragonNoFfmpeg', {}, { onGuide: (page, language) => opened.push([page, language]) });
  await demo.openGuide('ffmpeg', 'pt-BR');
  await demo.openGuide('ffmpeg', 'en');
  assert.deepEqual(opened, [['ffmpeg', 'pt-BR'], ['ffmpeg', 'en']]);
  await assert.rejects(demo.openGuide('../../etc/passwd', 'en'), (e) => e.code === 'invalidInput');
  await assert.rejects(demo.openGuide('ffmpeg', 'de'), (e) => e.code === 'invalidInput');
  assert.deepEqual(DEMO_GUIDE_PAGES, ['ffmpeg', 'gifs-and-stickers']);
  await createDemoBackend('turing88').openGuide('ffmpeg', 'en');
});

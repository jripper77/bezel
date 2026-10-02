import { test } from 'node:test';
import assert from 'node:assert/strict';
import { DEMO_AWAY_SAMPLES, DEMO_IMPORT_WARNINGS, DEMO_SENSORS, createDemoBackend, demoFits, demoFormat, demoNextChange, demoOrientation, demoThumbnail, demoValue } from '../../src/demo-backend.js';
import { DEMO_DRAGON_THEME, DEMO_GIF_THEME, DEMO_KLIPY_KEY, DEMO_LIBRARY } from '../../src/demo-data.js';
import { DEMO_GIF_OVERLAP, DEMO_GIF_PAGES, DEMO_GIF_PAGE_SIZE, DEMO_REFUSED_KEY, demoContentId, demoGifPage, demoGifSlug } from '../../src/demo-gifs.js';

const fixed = { now: () => 1000 };

test('demo values format like the core', () => {
  assert.equal(demoFormat(42.4, 'percent'), '42%');
  assert.equal(demoFormat(51.6, 'celsius'), '52°C');
  assert.equal(demoFormat(4720, 'megahertz'), '4.72 GHz');
  assert.equal(demoFormat(800, 'megahertz'), '800 MHz');
  assert.equal(demoFormat(512, 'bytes'), '512 B');
  assert.equal(demoFormat(2.5 * 1024 * 1024, 'bytesPerSecond'), '2.5 MiB/s');
  assert.equal(demoFormat(200 * 1024, 'bytes'), '200 KiB');
  assert.equal(demoFormat(93784, 'seconds'), '1d 02:03');
  assert.equal(demoFormat(3700, 'seconds'), '01:01');
  assert.equal(demoFormat(7, 'watts'), '7 W');
  assert.equal(demoFormat(7, 'rpm'), '7');
});

test('demo values never go negative', () => {
  for (let t = 0; t < 100; t += 0.5) assert.ok(demoValue(1, 10, t, 3) >= 0);
});

test('the catalog and samples cover every demo sensor', async () => {
  const demo = createDemoBackend('turing88', fixed);
  const catalog = await demo.catalog();
  assert.equal(catalog.length, DEMO_SENSORS.length);
  const { readings } = await demo.sample();
  for (const entry of catalog) assert.equal(typeof readings[entry.key].display, 'string', entry.key);
  assert.ok(readings['gpu.1.fan'].unavailable);
});

test('saving, listing and opening themes', async () => {
  const demo = createDemoBackend('turing88', fixed);
  const { theme } = await demo.session();
  const { location } = await demo.saveTheme({ ...theme, name: 'Mine' }, false);
  assert.equal(location, 'demo://Mine');
  const library = DEMO_LIBRARY.length;
  const listed = await demo.listThemes();
  assert.deepEqual(listed.map((x) => x.name), ['Demo', ...DEMO_LIBRARY.map((e) => e.theme.name), 'Mine']);
  assert.deepEqual(listed.map((x) => x.bundled), [true, ...DEMO_LIBRARY.map((e) => e.bundled), false]);
  assert.deepEqual([listed[0].orientation, listed.at(-1).orientation], ['reverse-portrait', 'reverse-portrait']);
  assert.equal((await demo.openTheme('demo://Mine')).name, 'Mine');
  await assert.rejects(demo.openTheme('demo://nope'), (e) => e.code === 'notInLibrary' && e.args.location === 'demo://nope');
  await demo.saveTheme({ ...theme, name: 'Mine', orientation: 'landscape', canvas: { width: 1920, height: 480 } }, false);
  assert.equal((await demo.listThemes()).length, library + 2);
  const mine = (await demo.listThemes()).at(-1);
  assert.equal(mine.orientation, 'landscape', 'saving again updates the entry');
  assert.equal(mine.revision, '2', 'and its revision');
  assert.equal((await demo.openTheme('demo://Mine')).orientation, 'landscape');
  assert.equal((await demo.newTheme()).elements.length, 0);
  assert.equal((await demo.newTheme('k', 'Novo')).name, 'Novo');
});

test('library themes name their screens and have thumbnails, but one', async () => {
  const demo = createDemoBackend('turing88', fixed);
  const listed = await demo.listThemes();
  const byName = Object.fromEntries(listed.map((e) => [e.name, e]));
  assert.deepEqual(byName.Demo.models, ['turing-8.8', 'turing-usb-8.8']);
  assert.equal(byName.Demo.diagonalHundredths, 880);
  assert.equal(byName['Midnight 2.1" round'].diagonalHundredths, null, '480×480 comes in several sizes');
  for (const entry of listed) {
    const url = await demo.themeThumbnail(entry.location);
    if (entry.name === 'TURZX 3.5"') assert.equal(url, null, 'cannot be drawn');
    else assert.match(url, /^data:image\/svg\+xml,%3Csvg/, entry.name);
  }
  await assert.rejects(demo.themeThumbnail('demo://nope'), (e) => e.code === 'notInLibrary');
  // Saved again, a theme that could not be drawn has its thumbnail.
  const broken = await demo.openTheme('demo://TURZX 3.5"');
  await demo.saveTheme(broken, false);
  assert.match(await demo.themeThumbnail('demo://TURZX 3.5"'), /^data:image\/svg\+xml,/);
});

test('demo thumbnails draw the background and each kind of element', () => {
  const theme = {
    canvas: { width: 100, height: 50 },
    background: { type: 'color', color: '#112233ff' },
    elements: [
      { frame: { x: 0, y: 0, width: 10, height: 10 }, kind: { type: 'shape', fill: '#ff0000ff', radius: 2 } },
      { frame: { x: 10, y: 0, width: 20, height: 20 }, kind: { type: 'ring', fill: 'url(x)', thickness: 4 } },
      { frame: { x: 30, y: 0, width: 20, height: 10 }, kind: { type: 'text', style: { paint: '#00ff00' } } },
      { frame: { x: 50, y: 0, width: 20, height: 10 }, kind: { type: 'bar' } },
      { frame: { x: 70, y: 0, width: 20, height: 10 }, visible: false, kind: { type: 'shape', fill: '#0000ffff' } },
    ],
  };
  const svg = decodeURIComponent(demoThumbnail(theme).slice('data:image/svg+xml,'.length));
  assert.match(svg, /fill="#112233ff"/);
  assert.match(svg, /fill="#ff0000ff"/);
  assert.match(svg, /stroke="#38bdf8"/, 'an unknown paint falls back');
  assert.match(svg, /fill="#00ff00"/);
  assert.doesNotMatch(svg, /#0000ffff/, 'hidden elements are left out');
  assert.doesNotMatch(demoThumbnail(theme), /[()]/, 'safe in a CSS url()');
  const video = decodeURIComponent(demoThumbnail({ canvas: { width: 4, height: 4 }, background: { type: 'video' } }));
  assert.match(video, /fill="#10111a"/);
});

test('a theme fits the demo models whose panel it is, turned its way', () => {
  assert.deepEqual(demoFits({ canvas: { width: 1920, height: 480 }, orientation: 'landscape' }), { models: ['turing-8.8', 'turing-usb-8.8'], diagonalHundredths: 880 });
  assert.deepEqual(demoFits({ canvas: { width: 1920, height: 480 }, orientation: 'portrait' }), { models: [], diagonalHundredths: null });
  assert.deepEqual(demoFits({ canvas: { width: 480, height: 320 }, orientation: 'reverse-landscape' }).models, ['turing-3.5']);
});

test('the gallery filter is remembered and checked', async () => {
  const demo = createDemoBackend('turing88', fixed);
  assert.deepEqual((await demo.preferences()).themeFilter, { scope: null, axis: 'all' });
  await demo.setThemeFilter('all', 'horizontal');
  assert.deepEqual((await demo.preferences()).themeFilter, { scope: 'all', axis: 'horizontal' });
  await assert.rejects(demo.setThemeFilter('mine', 'all'), (e) => e.code === 'invalidInput' && e.args.detail.includes('mine'));
  await assert.rejects(demo.setThemeFilter(null, 'diagonal'), (e) => e.code === 'invalidInput');
  await demo.setThemeFilter(null, 'vertical');
  assert.deepEqual((await demo.preferences()).themeFilter, { scope: null, axis: 'vertical' });
});

test('new themes: the orientation asked for, else the last one used with the screen, else by shape', async () => {
  const demo = createDemoBackend('two', fixed);
  const [big, small] = (await demo.listDevices()).screens.map((s) => s.key);
  const wide = await demo.newTheme(big, 'A');
  assert.deepEqual([wide.orientation, wide.canvas], ['landscape', { width: 1920, height: 480 }], 'the 8.8" is a bar');
  assert.equal((await demo.newTheme(small, 'B')).orientation, 'portrait', 'a square screen stays vertical');
  const tall = await demo.newTheme(big, 'C', 'reverse-portrait');
  assert.deepEqual([tall.orientation, tall.canvas], ['reverse-portrait', { width: 480, height: 1920 }]);
  assert.equal((await demo.newTheme(big, 'D')).orientation, 'reverse-portrait', 'remembered for that screen');
  await demo.setLive(true, small);
  await demo.pushTheme({ ...tall, orientation: 'reverse-landscape' });
  assert.equal((await demo.newTheme(small, 'E')).orientation, 'reverse-landscape', 'what was shown live');
  assert.equal(demoOrientation(undefined, undefined), 'landscape');
  assert.equal(demoOrientation({ width: 800, height: 480 }, undefined), 'portrait');
  assert.equal(demoOrientation({ width: 320, height: 960 }, undefined), 'landscape');
});

test('images, live mode and fonts', async () => {
  const demo = createDemoBackend('empty', fixed);
  assert.deepEqual(await demo.listDevices(), { screens: [], desktopMode: [] });
  assert.deepEqual(await demo.assets(), []);
  await demo.addImage();
  assert.deepEqual(await demo.assets(), [{ ref: 'assets/image-1.png', kind: 'image' }]);
  assert.equal(demo.isLive(), false);
  await demo.setLive(true, 'k');
  assert.equal(demo.isLive(), true);
  assert.equal((await demo.sample()).live, 'k');
  assert.equal(await demo.getAutostart(), false);
  await demo.setAutostart(true);
  assert.equal(await demo.getAutostart(), true);
  await demo.pushTheme({ name: 'pushed' });
  assert.equal((await demo.session()).theme.name, 'pushed');
  assert.ok((await demo.fonts()).includes('Inter'));
  const imported = await demo.importTheme();
  assert.equal(imported.theme.name, 'Imported');
  assert.deepEqual(imported.warnings, [...DEMO_IMPORT_WARNINGS]);
  await demo.setBrightness('k', 10);
  await demo.release('k');
});

test('the window hides while live, asks over unsaved edits, else closes', async () => {
  const seen = [];
  const demo = createDemoBackend('turing88', fixed, { onWindow: (state) => seen.push(state) });
  const asked = [];
  const stop = await demo.onCloseRequested(() => asked.push('asked'));
  assert.equal(demo.windowState(), 'open');
  await demo.setUnsaved(true);
  demo.requestClose();
  assert.deepEqual(asked, ['asked']);
  assert.equal(demo.windowState(), 'open', 'the UI decides');
  await demo.setLive(true, '/dev/ttyACM1');
  demo.requestClose();
  assert.equal(demo.windowState(), 'hidden');
  await demo.closeWindow();
  assert.equal(demo.windowState(), 'hidden', 'still live');
  await demo.setLive(false);
  await demo.setUnsaved(false);
  stop();
  demo.requestClose();
  assert.deepEqual(asked, ['asked']);
  assert.equal(demo.windowState(), 'closed');
  await demo.closeWindow();
  assert.deepEqual(seen, ['hidden', 'hidden', 'closed', 'closed']);
  // Without hooks nothing breaks.
  await createDemoBackend('empty', fixed).closeWindow();
});

test('quitting from the tray shows the window and asks over unsaved edits', async () => {
  const seen = [];
  const demo = createDemoBackend('turing88', fixed, { onWindow: (state) => seen.push(state) });
  const asked = [];
  await demo.onQuitRequested(() => asked.push('asked'));
  await demo.setLive(true, '/dev/ttyACM1');
  await demo.setUnsaved(true);
  demo.requestClose();
  assert.equal(demo.windowState(), 'hidden', 'live: the window hides');
  demo.requestQuit();
  assert.deepEqual([demo.windowState(), asked], ['open', ['asked']], 'shown, and the UI asks');
  await demo.quitApp();
  assert.equal(demo.windowState(), 'quit');
  const clean = createDemoBackend('turing88', fixed);
  clean.requestQuit();
  assert.equal(clean.windowState(), 'quit', 'nothing unsaved: it quits at once');
  assert.deepEqual(seen, ['hidden', 'open', 'quit']);
});

test('a panel in desktop mode is listed and switched back only when confirmed', async () => {
  const demo = createDemoBackend('desktop', fixed);
  const before = await demo.listDevices();
  assert.equal(before.screens.length, 1);
  const [panel] = before.desktopMode;
  assert.equal(panel.hardwareValidated, false);
  await assert.rejects(demo.leaveDesktopMode(panel.key, false), (e) => e.code === 'notConfirmed');
  assert.equal((await demo.listDevices()).desktopMode.length, 1, 'nothing sent');
  assert.deepEqual(await demo.leaveDesktopMode(panel.key, true), { model: 'Turing 8.8" V1.x (USB)' });
  const after = await demo.listDevices();
  assert.deepEqual([after.screens.length, after.desktopMode.length], [2, 0], 'back as a screen');
  await assert.rejects(demo.leaveDesktopMode(panel.key, true), (e) => e.code === 'screenNotFound');
});

test('the demo GIF changes every 100 ms, like the backend says', () => {
  assert.equal(demoNextChange(DEMO_GIF_THEME, 1234), 66);
  assert.equal(demoNextChange(DEMO_GIF_THEME, 1300), 100);
  const hidden = { ...DEMO_GIF_THEME, elements: DEMO_GIF_THEME.elements.map((e) => ({ ...e, visible: false })) };
  assert.equal(demoNextChange(hidden, 1234), null);
  assert.equal(demoNextChange({ elements: [{ kind: { type: 'image', asset: 'assets/logo.png' } }] }, 0), null);
  assert.equal(demoNextChange(undefined, 0), null);
});

test('a flaky demo screen is away for a while once live, then back', async () => {
  const demo = createDemoBackend('flaky', fixed);
  assert.equal((await demo.sample()).reconnecting, null, 'not live yet');
  await demo.setLive(true, '/dev/ttyACM1');
  for (let i = 0; i < DEMO_AWAY_SAMPLES; i += 1) {
    const away = await demo.sample();
    assert.deepEqual(away.reconnecting, { attempt: 1, attempts: 3 });
    assert.equal(away.live, '/dev/ttyACM1', 'still live');
  }
  const back = await demo.sample();
  assert.equal(back.reconnecting, null);
  assert.equal(back.live, '/dev/ttyACM1');
  assert.equal(back.liveError, null);
});

test('the mcuLive demo names the live 8.8" by its MCU port, like 0.1.0-dev.287', async () => {
  const demo = createDemoBackend('mcuLive', { now: () => 1000, delay: () => Promise.resolve() });
  const [screen] = (await demo.listDevices()).screens;
  assert.deepEqual([screen.key, screen.wake.address], ['/dev/ttyACM1', '/dev/ttyACM0']);
  // A theme no panel fits: Auto knows the panel only from the live screen.
  const odd = { ...DEMO_DRAGON_THEME, canvas: { width: 1000, height: 300 } };
  assert.equal((await demo.videoAuto(odd)).rotation, 0, 'no screen live');
  assert.deepEqual(await demo.setLive(true, screen.key), { live: '/dev/ttyACM0' });
  assert.equal((await demo.sample()).live, '/dev/ttyACM0', 'the samples report the MCU port');
  // Everything else knows the live screen by either port, like the backend.
  assert.equal((await demo.videoAuto(odd)).rotation, 270, 'the live 8.8"\'s panel');
  await assert.rejects(demo.playStored(screen.key, 'internal/image/logo.png'), (e) => e.code === 'live');
  await assert.rejects(demo.stopPlayback(screen.key), (e) => e.code === 'live');
  await demo.pushTheme({ ...DEMO_DRAGON_THEME, orientation: 'reverse-landscape' });
  assert.equal((await demo.newTheme(screen.key, 'A')).orientation, 'reverse-landscape', 'remembered under its listed key');
  await demo.setBrightness(screen.key, 40);
  // Restarted, it is live again under the key it is listed by.
  assert.deepEqual(await demo.restartScreen(screen.key), { key: '/dev/ttyACM1', live: true });
  assert.equal((await demo.sample()).live, '/dev/ttyACM1');
  await demo.setLive(false, screen.key);
  assert.equal((await demo.sample()).live, null);
  await demo.playStored(screen.key, 'internal/image/logo.png');
  // A key no screen is listed by stays as it is.
  await demo.setLive(true, 'k');
  assert.equal((await demo.sample()).live, 'k');
  // Other scenarios record the key they are given.
  const plain = createDemoBackend('turing88', fixed);
  assert.deepEqual(await plain.setLive(true, screen.key), { live: '/dev/ttyACM1' });
});

// ------------------------------------------------------ storage manager --
// The demo's storage manager (D-2026-09-30-storage-manager-13): the user's
// real card beside an internal memory Bezel filled, plans run one file at a
// time (copy, check, and only then delete the source), batch deletes, the
// association of an original and the local copies.
const instant = { now: () => 1_790_000_000, delay: () => Promise.resolve() };
const KEY = '/dev/ttyACM1';
/** `paths` as a delete confirmation lists them: with the sizes the screen lists now. */
const listed = (demo, paths) => paths.map((path) => ({ path, size: demo.storageState().files.get(path) ?? null }));

/** Lets every pending promise callback run. */
const settle = async () => {
  for (let i = 0; i < 50; i += 1) await new Promise((resolve) => { setImmediate(resolve); });
};

test('the vendor card scenario is the user\'s real card beside an internal memory Bezel filled', async () => {
  const demo = createDemoBackend('vendorCard', instant);
  const o = await demo.managerOverview(KEY);
  assert.deepEqual(o.internal, { total: 69_101_158, used: 19_503_514, free: 49_597_644 }, '18.6 MiB of 65.9 MiB');
  assert.deepEqual([o.card.total, o.card.used], [31_890_132_172, 108_842_189 + 29_577_216], '103.8 MiB of 29.7 GiB, plus the partial');
  assert.equal(o.files.length, 18);
  const by = Object.fromEntries(o.files.map((f) => [f.name, f]));
  assert.deepEqual([by['earth.mp4'].entry.state, by['earth.mp4'].entry.localCopy, by['earth.mp4'].finding, by['earth.mp4'].protected], ['stored', true, null, 'boot']);
  assert.equal(by['DARIUS.mp4'].entry, null, 'its original is on the PC');
  assert.deepEqual(by['NVI.mp427034822.mp4'].finding, { code: 'variant', prechecked: false, kept: 'sd/video/NVI.mp4', cataloged: null });
  assert.deepEqual(by['bezel_test_cancel.mp4'].finding, { code: 'hangPartial', prechecked: true, kept: null, cataloged: null });
  assert.deepEqual(o.files.filter((f) => f.finding?.prechecked).map((f) => f.name), ['bezel_test_cancel.mp4']);
  assert.deepEqual(o.restorable.map((r) => [r.name, r.state, r.otherCard]), [['relogio.mp4', 'missing', false], ['foto.png', 'stored', true]]);
  assert.deepEqual([o.deletes, o.cap, o.folderErrors], [true, 26_214_400, []]);
  assert.deepEqual(o.cache, { copies: 6, bytes: 19_386_367, deletedCopies: 0, deletedBytes: 0, limit: 2 * 2 ** 30 });
  assert.match(await demo.managerThumbnail(KEY, 'internal/video/earth.mp4'), /^data:image\/svg\+xml,/);
  assert.equal(await demo.managerThumbnail(KEY, 'sd/video/AMD.mp4'), null);
});

test('a move sends the copy, checks it, and only then deletes the source', async () => {
  const demo = createDemoBackend('vendorCard', instant);
  const seen = [];
  demo.onJobProgress((p) => {
    const files = demo.storageState().files;
    seen.push([p.phase, p.step?.index, files.has(p.step?.source), files.get(p.step?.target) ?? 0, p.total]);
  });
  const plan = await demo.planMove(KEY, ['internal/video/earth.mp4', 'internal/video/aniya.mp4', 'internal/video/DARIUS.mp4'], 'sd');
  assert.deepEqual(plan.steps.map((s) => s.target), ['sd/video/earth.mp4', 'sd/video/aniya.mp4']);
  assert.deepEqual(plan.skipped.map((s) => [s.source, s.code]), [['internal/video/DARIUS.mp4', 'noLocalCopy']]);
  assert.deepEqual(plan.warnings, [{ code: 'bootMedia', path: 'internal/video/earth.mp4' }]);
  assert.equal('content' in plan.steps[0], false, 'no content ids reach the UI');
  assert.equal(plan.bytes, 2_516_582 + 3_040_870);
  const report = await demo.runPlan(plan.ticket, true);
  assert.deepEqual([report.done.length, report.failed, report.cancelled, report.notStarted], [2, null, null, []]);
  // The source is there through the upload and the check; it goes in the last phase.
  for (const [phase, , source, target, total] of seen) {
    if (phase !== 'delete') assert.ok(source, `${phase}: the source stays`);
    if (phase === 'verify' || phase === 'delete') assert.ok(target === total || total === 1, phase);
  }
  assert.deepEqual(seen.filter(([phase]) => phase === 'delete').map(([, i, source]) => [i, source]), [[0, true], [0, false], [1, true], [1, false]]);
  const state = demo.storageState();
  assert.equal(state.files.has('internal/video/earth.mp4'), false);
  assert.equal(state.files.get('sd/video/earth.mp4'), 2_516_582);
  const moved = state.catalog.find((e) => e.path === 'sd/video/earth.mp4');
  assert.deepEqual([moved.state, moved.card, moved.source], ['stored', 31_890_132_172, '/home/demo/Vídeos/earth.mp4']);
  assert.equal(state.catalog.some((e) => e.path === 'internal/video/earth.mp4'), false);
  await assert.rejects(demo.runPlan(plan.ticket, true), (e) => e.code === 'stale');
});

test('a plan runs only with the dialog\'s confirmation, like the app', async () => {
  const demo = createDemoBackend('vendorCard', instant);
  const before = new Map(demo.storageState().files);
  let progress = 0;
  demo.onJobProgress(() => { progress += 1; });
  const plan = await demo.planMove(KEY, ['internal/video/earth.mp4', 'internal/video/aniya.mp4'], 'sd');
  await assert.rejects(demo.runPlan(plan.ticket, false), (e) => e.code === 'notConfirmed' && e.args.detail === 'moving 2 files');
  assert.deepEqual(demo.storageState().files, before, 'nothing sent, nothing deleted');
  assert.equal(progress, 0);
  await assert.rejects(demo.runPlan(plan.ticket, true), (e) => e.code === 'stale', 'the plan is used up, as in the app');
});

test('a cancelled or failed move keeps its source and stops the batch', async () => {
  const demo = createDemoBackend('vendorCard', instant, { hold: true });
  const plan = await demo.planMove(KEY, ['internal/video/earth.mp4', 'internal/video/aniya.mp4'], 'sd');
  const running = demo.runPlan(plan.ticket, true);
  await settle();
  // Held in the middle of the first upload: both there, then cancelled.
  assert.ok(demo.storageState().files.has('internal/video/earth.mp4'));
  await assert.rejects(demo.managerOverview(KEY), (e) => e.code === 'busy');
  await assert.rejects(demo.planCopy(KEY, ['internal/video/jyanme.mp4'], 'sd'), (e) => e.code === 'busy');
  await assert.rejects(demo.deleteFiles(KEY, listed(demo, ['sd/video/AMD.mp4']), true), (e) => e.code === 'busy');
  assert.equal(await demo.cancelJob(), true);
  const report = await running;
  assert.deepEqual(report.done, []);
  assert.deepEqual([report.status, report.cancelled.step.source, report.cancelled.stage], ['ran', 'internal/video/earth.mp4', 'upload']);
  assert.equal(report.cancelled.partial, Math.round(2_516_582 / 8));
  assert.deepEqual(report.notStarted.map((s) => s.source), ['internal/video/aniya.mp4']);
  const files = demo.storageState().files;
  assert.ok(files.has('internal/video/earth.mp4') && files.has('internal/video/aniya.mp4'), 'the sources stay');
  assert.equal(files.get('sd/video/earth.mp4'), report.cancelled.partial, 'the partial file stays for a confirmed delete');
  // Its entry stays pending: the cleanup checks the partial.
  const o = await demo.managerOverview(KEY);
  assert.deepEqual(o.files.find((f) => f.path === 'sd/video/earth.mp4').finding.code, 'pending');

  // A screen that hangs: the first file fails with the reason, the rest never starts.
  const hung = createDemoBackend('hung', instant);
  const failing = await hung.planMove(KEY, ['internal/image/logo.png', 'internal/video/amd_90.mp4'], 'sd');
  const failed = await hung.runPlan(failing.ticket, true);
  assert.deepEqual([failed.failed.stage, failed.failed.halt, failed.failed.error.code], ['upload', 'failed', 'hung']);
  assert.equal(failed.failed.refusal, null);
  assert.equal(failed.notStarted.length, 1);
  assert.ok(hung.storageState().files.has('internal/image/logo.png'));
});

test('a copy keeps the source, a rename takes the new name, a restore sends from the copies', async () => {
  const demo = createDemoBackend('vendorCard', instant);
  const copy = await demo.planCopy(KEY, ['internal/video/dragon.mp4'], 'sd');
  assert.equal((await demo.runPlan(copy.ticket, true)).done.length, 1);
  assert.ok(demo.storageState().files.has('internal/video/dragon.mp4'));
  assert.ok(demo.storageState().files.has('sd/video/dragon.mp4'));

  const refused = await demo.planRename(KEY, 'internal/video/jyanme.mp4', 'jyanme.mov');
  assert.deepEqual([refused.status, refused.code, refused.args], ['refused', 'extensionChanged', { expected: 'mp4' }]);
  const rename = await demo.planRename(KEY, 'internal/video/jyanme.mp4', 'Jyanme_2.mp4');
  assert.equal((await demo.runPlan(rename.ticket, true)).transfer, 'rename');
  assert.ok(demo.storageState().files.has('internal/video/jyanme_2.mp4'));
  assert.ok(!demo.storageState().files.has('internal/video/jyanme.mp4'));

  const { restorable } = await demo.managerOverview(KEY);
  const restore = await demo.planRestore(KEY, restorable.map((r) => r.id), 'sd');
  assert.deepEqual(restore.steps.map((s) => s.target), ['sd/image/foto.png', 'sd/video/relogio.mp4'], 'oldest first');
  assert.equal((await demo.runPlan(restore.ticket, true)).done.length, 2);
  const after = await demo.managerOverview(KEY);
  assert.deepEqual(after.restorable.map((r) => r.name), ['foto.png'], 'the other card\'s entry stays for it');
  assert.equal(after.files.find((f) => f.name === 'relogio.mp4').entry.state, 'stored');
  // Restoring what is there skips it as present.
  const again = await demo.planRestore(KEY, after.restorable.map((r) => r.id), 'sd');
  assert.deepEqual(again.skipped.map((s) => s.code), ['present']);
  await assert.rejects(demo.planRestore(KEY, [], 'usb'), (e) => e.code === 'unknownMedium' && e.args.medium === 'usb');
});

test('a file deleted through Bezel is restorable from its copy, on request', async () => {
  const demo = createDemoBackend('vendorCard', instant);
  await demo.deleteFiles(KEY, listed(demo, ['internal/video/aniya.mp4']), true);
  const { restorable } = await demo.managerOverview(KEY);
  const aniya = restorable.find((r) => r.path === 'internal/video/aniya.mp4');
  assert.deepEqual([aniya.state, aniya.localCopy, aniya.otherCard, aniya.id], ['deleted', true, false, 'internal/video/aniya.mp4@']);
  assert.equal(restorable.at(-1), aniya, 'after what is missing');
  const plan = await demo.planRestore(KEY, [aniya.id], 'internal');
  assert.deepEqual(plan.steps.map((s) => s.target), ['internal/video/aniya.mp4']);
  const report = await demo.runPlan(plan.ticket, true);
  assert.deepEqual([report.status, report.done.length], ['ran', 1]);
  assert.equal(demo.storageState().files.get('internal/video/aniya.mp4'), 3_040_870);
  assert.equal(demo.storageState().catalog.find((e) => e.path === 'internal/video/aniya.mp4').state, 'stored');
  assert.equal((await demo.managerOverview(KEY)).restorable.some((r) => r.path === 'internal/video/aniya.mp4'), false);
  // Without its copy it is not offered.
  await demo.deleteFiles(KEY, listed(demo, ['internal/video/aniya.mp4']), true);
  await demo.clearCache('deleted', true);
  assert.equal((await demo.managerOverview(KEY)).restorable.some((r) => r.path === 'internal/video/aniya.mp4'), false);
});

test('a restore that no longer fits when it runs is refused by code, nothing sent', async () => {
  // 32 MB of internal memory: one copy of the 18.9 MB video fits, two do not.
  const demo = createDemoBackend('noffmpeg', instant);
  await demo.deleteFiles(KEY, listed(demo, ['internal/video/amd_90.mp4']), true);
  const [amd] = (await demo.managerOverview(KEY)).restorable;
  const first = await demo.planRestore(KEY, [amd.id], 'internal');
  const second = await demo.planRestore(KEY, [amd.id], 'internal');
  assert.equal((await demo.runPlan(first.ticket, true)).done.length, 1);
  const before = new Map(demo.storageState().files);
  const refused = await demo.runPlan(second.ticket, true);
  assert.deepEqual([refused.status, refused.code, refused.args.needed], ['refused', 'noSpace', 18_874_368]);
  assert.ok(refused.args.free < refused.args.needed);
  assert.deepEqual(demo.storageState().files, before);
});

test('batch deletes go one by one, are reported and count against the cache limit', async () => {
  const demo = createDemoBackend('vendorCard', instant);
  await assert.rejects(demo.deleteFiles(KEY, listed(demo, ['sd/video/bezel_test_cancel.mp4']), false), (e) => e.code === 'notConfirmed');
  const seen = [];
  demo.onJobProgress((p) => seen.push(p));
  const report = await demo.deleteFiles(KEY, listed(demo, ['sd/video/bezel_test_cancel.mp4', 'internal/video/aniya.mp4', 'sd/video/none.mp4', 'sd/video/AMD.mp4']), true);
  assert.deepEqual(report.deleted, ['sd/video/bezel_test_cancel.mp4', 'internal/video/aniya.mp4']);
  assert.equal(report.freed, 29_577_216 + 3_040_870);
  assert.deepEqual([report.failed.path, report.failed.halt, report.failed.error, report.notStarted], ['sd/video/none.mp4', 'sourceChanged', null, ['sd/video/AMD.mp4']]);
  assert.deepEqual(seen.map((p) => [p.phase, p.done, p.total, p.step.source]).slice(0, 2), [
    ['delete', 0, 4, 'sd/video/bezel_test_cancel.mp4'], ['delete', 1, 4, 'internal/video/aniya.mp4'],
  ]);
  // Bezel's own file deleted: its copy stays and counts against the limit.
  assert.equal(demo.storageState().catalog.find((e) => e.path === 'internal/video/aniya.mp4').state, 'deleted');
  const info = await demo.cacheInfo();
  assert.deepEqual([info.deletedCopies, info.deletedBytes], [1, 3_040_870]);
  assert.deepEqual(await demo.setCacheLimit(1_000), { ...info, copies: 5, bytes: info.bytes - 3_040_870, deletedCopies: 0, deletedBytes: 0, limit: 1_000 }, 'evicted');
  await assert.rejects(demo.setCacheLimit(-1), (e) => e.code === 'invalidInput');

  const turzx = createDemoBackend('turzx', instant);
  const [screen] = (await turzx.listDevices()).screens;
  await assert.rejects(turzx.deleteFiles(screen.key, listed(turzx, ['internal/image/logo.png']), true), (e) => e.code === 'unsupported');
});

test('a file of another size than confirmed is not deleted, like the app', async () => {
  const demo = createDemoBackend('vendorCard', instant);
  const amd = demo.storageState().files.get('sd/video/AMD.mp4');
  const report = await demo.deleteFiles(KEY, [{ path: 'sd/video/AMD.mp4', size: amd - 1 }, ...listed(demo, ['sd/video/m04.mp4'])], true);
  assert.deepEqual([report.deleted, report.failed.path, report.failed.halt, report.notStarted], [[], 'sd/video/AMD.mp4', 'sourceChanged', ['sd/video/m04.mp4']]);
  assert.equal(demo.storageState().files.get('sd/video/AMD.mp4'), amd, 'still there');
});

test('a cancelled batch delete stops before the next file', async () => {
  const demo = createDemoBackend('vendorCard', instant);
  demo.onJobProgress((p) => { if (p.step.index === 0) demo.cancelJob(); });
  const report = await demo.deleteFiles(KEY, listed(demo, ['sd/video/m04.mp4', 'sd/video/AMD.mp4']), true);
  assert.deepEqual([report.deleted, report.cancelled, report.notStarted], [['sd/video/m04.mp4'], true, ['sd/video/AMD.mp4']]);
});

test('associating an original copies it into the store; clearing the cache keeps entries and thumbnails', async () => {
  const demo = createDemoBackend('vendorCard', instant);
  const files = await demo.pickOriginals(false);
  const folder = await demo.pickOriginals(true);
  assert.equal(folder.length, 1);
  const { candidates } = await demo.associateCandidates(KEY, 'internal/video/DARIUS.mp4', folder);
  assert.deepEqual(candidates.map((c) => [c.name, c.sameName]), [['DARIUS.mp4', true], ['abertura.mp4', false]]);
  assert.deepEqual((await demo.associateCandidates(KEY, 'internal/video/DARIUS.mp4', files)).candidates.length, 2);
  await assert.rejects(demo.associateCandidates(KEY, 'internal/video/none.mp4', files), (e) => e.code === 'invalidInput');
  await assert.rejects(demo.associateOriginal(KEY, 'internal/video/DARIUS.mp4', candidates[0].source, false), (e) => e.code === 'notConfirmed');
  await assert.rejects(demo.associateOriginal(KEY, 'internal/video/DARIUS.mp4', '/home/demo/Vídeos/darius_final.mp4', true), (e) => e.code === 'invalidInput');
  const done = await demo.associateOriginal(KEY, 'internal/video/DARIUS.mp4', candidates[0].source, true);
  assert.deepEqual([done.entry.localCopy, done.entry.source, done.finding], [true, '/home/demo/Vídeos/DARIUS.mp4', null]);
  assert.match(await demo.managerThumbnail(KEY, 'internal/video/DARIUS.mp4'), /^data:/);
  // Movable now.
  assert.equal((await demo.planMove(KEY, ['internal/video/DARIUS.mp4'], 'sd')).steps[0].target, 'sd/video/darius.mp4');

  await assert.rejects(demo.clearCache('deleted', false), (e) => e.code === 'notConfirmed');
  await assert.rejects(demo.clearCache('some', true), (e) => e.code === 'invalidInput');
  assert.deepEqual(await demo.clearCache('deleted', true), { removed: 0, bytes: 0 });
  const all = await demo.clearCache('all', true);
  assert.equal(all.removed, 7);
  const o = await demo.managerOverview(KEY);
  const earth = o.files.find((f) => f.name === 'earth.mp4');
  assert.deepEqual([earth.entry.state, earth.entry.localCopy], ['stored', false]);
  assert.match(await demo.managerThumbnail(KEY, 'internal/video/earth.mp4'), /^data:/, 'the thumbnail stays');
  assert.deepEqual((await demo.planMove(KEY, ['internal/video/earth.mp4'], 'sd')).skipped.map((s) => s.code), ['noLocalCopy']);
});

test('uploads are recorded pending before the first byte and stored once checked', async () => {
  const demo = createDemoBackend('turing88', instant, { hold: true });
  const ready = await demo.prepareUpload(KEY, 'demo://relogio.mp4', 'sd');
  const running = demo.runUpload(ready.ticket, false);
  await settle();
  const pending = demo.storageState().catalog.find((e) => e.path === 'sd/video/relogio.mp4');
  assert.deepEqual([pending.state, pending.size, pending.source], ['pending', 6_291_456, 'demo://relogio.mp4']);
  demo.letGo();
  assert.equal((await running).status, 'done');
  assert.equal(demo.storageState().catalog.find((e) => e.path === 'sd/video/relogio.mp4').state, 'stored');
  // A file deleted through Bezel keeps its entry, deleted, with its copy.
  await demo.deleteStored(KEY, 'sd/video/relogio.mp4', true);
  assert.equal(demo.storageState().catalog.find((e) => e.path === 'sd/video/relogio.mp4').state, 'deleted');
});

test('the manager refuses like the overview, and a theme\'s video and the boot media are protected', async () => {
  const denied = createDemoBackend('denied', instant);
  await assert.rejects(denied.managerOverview(KEY), (e) => e.code === 'accessDenied');
  const demo = createDemoBackend('video', instant);
  await assert.rejects(demo.managerOverview('COM9'), (e) => e.code === 'unsupported');
  await assert.rejects(demo.associateCandidates('COM9', 'x', []), (e) => e.code === 'unsupported');
  await assert.rejects(demo.associateOriginal('COM9', 'x', 'y', true), (e) => e.code === 'unsupported');
  await assert.rejects(demo.planRestore('COM9', [], 'sd'), (e) => e.code === 'unsupported');
  // The theme plays nebula.mp4: its turned names on the screen are protected.
  await demo.setLive(true, KEY);
  const ready = await demo.prepareThemeVideo(KEY);
  await demo.runUpload(ready.ticket, false);
  await demo.setLive(false, KEY);
  const o = await demo.managerOverview(KEY);
  const nebula = o.files.find((f) => f.name === 'nebula_90.mp4');
  assert.deepEqual([nebula.protected, nebula.finding, nebula.entry.source], ['themeVideo', null, 'assets/nebula.mp4']);
  const rename = await demo.planRename(KEY, 'sd/video/nebula_90.mp4', 'nebula_b.mp4');
  assert.deepEqual(rename.warnings, [{ code: 'themeVideo', path: 'sd/video/nebula_90.mp4' }]);
  await demo.setBootMedia(KEY, 'sd/video/chuva.mp4', true);
  assert.equal((await demo.managerOverview(KEY)).files.find((f) => f.name === 'chuva.mp4').protected, 'boot');
  // A TUR_USB screen lists sizes only from the catalog.
  const turzx = createDemoBackend('turzx', instant);
  const [screen] = (await turzx.listDevices()).screens;
  const usb = await turzx.managerOverview(screen.key);
  assert.equal(usb.deletes, false);
  assert.deepEqual(usb.files.map((f) => [f.name, f.size]), [['logo.png', 184_320], ['amd_90.mp4', 18_874_368], ['chuva.mp4', null]]);
  assert.deepEqual((await turzx.planMove(screen.key, ['internal/image/logo.png'], 'sd')).skipped.map((s) => s.code), ['deleteUnsupported']);
});

// ------------------------------------------------- GIFs and stickers --
// The demo's KLIPY and collection answer like the backend's commands
// (D-2026-10-01-gif-sticker-search-6).

/** A demo with the GIF hooks recorded. */
function gifDemo(scenario) {
  const seen = { queries: [], links: [] };
  const demo = createDemoBackend(scenario, fixed, { onGifQuery: (q) => seen.queries.push(q), onLink: (l) => seen.links.push(l) });
  return { demo, seen };
}
const code = (expected, args) => (e) => e.code === expected && (args === undefined || JSON.stringify(e.args) === JSON.stringify(args));

test('the demo KLIPY key: nothing asked without one, checked when saved, shown by its last 4', async () => {
  const { demo, seen } = gifDemo('gifsNoKey');
  assert.deepEqual(await demo.klipyKey(), { configured: false, last4: null });
  await assert.rejects(demo.searchGifs({ kind: 'gif', text: '', page: 1, explicit: false }), code('klipyNoKey', {}));
  assert.deepEqual(seen.queries, [], 'no key: nothing reaches KLIPY');
  for (const bad of ['', 'has space', 'a'.repeat(129), 'chave-ç', 42]) await assert.rejects(demo.saveKlipyKey(bad), code('invalidInput'), String(bad));
  assert.deepEqual(await demo.saveKlipyKey('abc_DEF-123x'), { configured: true, last4: '123x' });
  assert.deepEqual(await demo.klipyKey(), { configured: true, last4: '123x' });
  assert.deepEqual(seen.queries, [], 'saving sends nothing');
  assert.deepEqual(await demo.saveKlipyKey('short-1'), { configured: true, last4: null }, 'like the backend: a short key shows no ending');
  assert.deepEqual(await demo.saveKlipyKey('nine-char'), { configured: true, last4: 'char' });
  assert.deepEqual(await demo.removeKlipyKey(), { configured: false, last4: null });
  assert.deepEqual(await createDemoBackend('turing88').klipyKey(), { configured: false, last4: null }, 'a fresh install has none');
  assert.deepEqual(await createDemoBackend('gifs').klipyKey(), { configured: true, last4: DEMO_KLIPY_KEY.slice(-4) });
});

test('the demo KLIPY pages: 24 results, later pages repeat a few, the query shown', async () => {
  assert.equal(demoGifSlug(' Dancing Cat! '), 'dancing-cat');
  assert.equal(demoGifSlug(''), '');
  const { demo, seen } = gifDemo('gifs');
  const first = await demo.searchGifs({ kind: 'gif', text: ' Cat ', page: 1, explicit: false });
  assert.deepEqual([first.kind, first.text, first.page, first.hasNext, first.items.length], ['gif', 'Cat', 1, true, DEMO_GIF_PAGE_SIZE]);
  assert.deepEqual(first.items[0], { id: 'gif-cat-1', title: 'Happy dance', width: 480, height: 270 });
  assert.deepEqual(seen.queries, [{ kind: 'gif', text: 'Cat', page: 1, explicit: false }]);
  const second = await demo.searchGifs({ kind: 'gif', text: 'Cat', page: 2, explicit: false });
  assert.deepEqual(second.items.slice(0, DEMO_GIF_OVERLAP), first.items.slice(-DEMO_GIF_OVERLAP), 'page 2 repeats the end of page 1');
  const last = await demo.searchGifs({ kind: 'gif', text: 'Cat', page: DEMO_GIF_PAGES, explicit: true });
  assert.equal(last.hasNext, false);
  assert.equal(last.items.at(-1).title, 'High five 3', 'titles repeat with a number');
  const stickers = await demo.searchGifs({ kind: 'sticker', text: '', page: 1, explicit: true });
  assert.deepEqual(stickers.items[0], { id: 'sticker-trending-1', title: 'Star', width: 512, height: 512 });
  assert.deepEqual(seen.queries.at(-1), { kind: 'sticker', text: '', page: 1, explicit: true });
  await assert.rejects(demo.searchGifs({ kind: 'video', text: 'x', page: 1, explicit: false }), code('invalidInput'));
  await assert.rejects(demo.searchGifs({ kind: 'gif', text: 'x', page: 0, explicit: false }), code('invalidInput'));
  await assert.rejects(demo.searchGifs(), code('invalidInput'));
  assert.deepEqual(demoGifPage({ kind: 'gif', text: 'Cat', page: 1 }), first, 'recorded: the same for the same query');
});

test('the demo previews and adds only results of the last search, once per content', async () => {
  const { demo } = gifDemo('gifs');
  const page = await demo.searchGifs({ kind: 'sticker', text: 'star', page: 1, explicit: false });
  const [a, b] = page.items;
  const moving = await demo.gifPreview(a.id, false);
  const still = await demo.gifPreview(a.id, true);
  assert.match(moving, /^data:image\/svg\+xml,/);
  assert.ok(moving.includes('animate') && !still.includes('animate'), 'motion reduced: a still');
  assert.ok(!decodeURIComponent(moving).includes('<rect'), 'a sticker has no background');
  assert.ok(!moving.includes('(') && !moving.includes(')'), 'usable in CSS url()');
  const added = await demo.collectGif(a.id);
  assert.deepEqual(Object.keys(added).sort(), ['addedAt', 'bytes', 'height', 'id', 'kind', 'name', 'preview', 'source', 'width']);
  assert.deepEqual([added.id, added.name, added.kind, added.width, added.addedAt], [demoContentId(a.id), 'Star', 'sticker', 512, 1000]);
  assert.match(added.id, /^[0-9a-f]{64}$/);
  assert.deepEqual(added.source, { provider: 'klipy', id: a.id, url: `https://klipy.com/stickers/${a.id}` });
  assert.ok(added.bytes > 0 && added.bytes < 26_214_400);
  assert.deepEqual(await demo.collectGif(a.id), added, 'the same content: the item already there');
  await demo.collectGif(b.id);
  assert.deepEqual((await demo.gifCollection(false)).map((c) => c.name), ['Heart', 'Star'], 'newest first');
  assert.ok(!(await demo.gifCollection(true))[0].preview.includes('animate'), 'stills when motion is reduced');
  await demo.searchGifs({ kind: 'gif', text: 'other', page: 1, explicit: false });
  await assert.rejects(demo.gifPreview(a.id, false), code('gifNotInResults', { item: a.id }), 'a new search replaces the results');
  await assert.rejects(demo.collectGif(a.id), code('gifNotInResults', { item: a.id }));
  await demo.searchGifs({ kind: 'gif', text: 'other', page: 2, explicit: false });
  assert.match(await demo.gifPreview('gif-other-1', false), /^data:/, 'a later page keeps the earlier ones');
});

test('the demo answers 429 and a refused key like KLIPY, after asking', async () => {
  const { demo, seen } = gifDemo('gifsRateLimited');
  await assert.rejects(demo.searchGifs({ kind: 'gif', text: 'cat', page: 1, explicit: false }), code('klipyRateLimited', {}));
  assert.deepEqual(seen.queries, [{ kind: 'gif', text: 'cat', page: 1, explicit: false }], 'the request was made');
  const refused = gifDemo('gifs');
  await refused.demo.saveKlipyKey(DEMO_REFUSED_KEY);
  await assert.rejects(refused.demo.searchGifs({ kind: 'gif', text: '', page: 1, explicit: false }), code('klipyKeyRejected', {}));
});

test('the demo collection: rename, the themes using an item, use and delete', async () => {
  const { demo, seen } = gifDemo('gifs');
  const [first] = (await demo.searchGifs({ kind: 'gif', text: 'wave', page: 1, explicit: false })).items;
  const item = await demo.collectGif(first.id);
  await assert.rejects(demo.renameCollected(item.id, '  '), code('invalidInput'));
  await assert.rejects(demo.renameCollected('nope', 'x'), code('notInCollection', { item: 'nope' }));
  assert.equal((await demo.renameCollected(item.id, ' Olá ')).name, 'Olá');
  assert.deepEqual(await demo.collectedUsers(item.id), { themes: [], openTheme: false });
  const image = await demo.useCollected(item.id, 'image');
  assert.deepEqual(image, { ref: 'assets/ol.gif', kind: 'image', poster: null, bytes: item.bytes, durationMs: null, posterError: null });
  assert.deepEqual(await demo.collectedUsers(item.id), { themes: [], openTheme: true }, 'the open theme holds it once copied, saved or not');
  const background = await demo.useCollected(item.id, 'background');
  assert.deepEqual([background.ref, background.kind, background.bytes], ['assets/ol-2.gif', 'video', item.bytes]);
  assert.ok((await demo.assets()).some((a) => a.ref === image.ref));
  await assert.rejects(demo.useCollected(item.id, 'wallpaper'), code('invalidInput'));
  const { theme } = await demo.session();
  const using = { ...theme, name: 'Mine', background: { type: 'video', asset: background.ref } };
  await demo.saveTheme(using, false);
  assert.deepEqual(await demo.collectedUsers(item.id), { themes: ['Mine'], openTheme: true });
  await assert.rejects(demo.collectedUsers('nope'), code('notInCollection'));
  await assert.rejects(demo.deleteCollected(item.id, false), code('notConfirmed'));
  assert.equal(await demo.deleteCollected(item.id, true), null);
  assert.deepEqual(await demo.gifCollection(false), []);
  await assert.rejects(demo.deleteCollected(item.id, true), code('notInCollection', { item: item.id }));
  await assert.rejects(demo.openLink('https://example.com'), code('invalidInput'));
  await demo.openLink('klipyPartnerPanel');
  assert.deepEqual(seen.links, ['klipyPartnerPanel']);
  await demo.openGuide('gifs-and-stickers', 'pt-BR');
  await createDemoBackend('gifs').openLink('klipyPartnerPanel');
});

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { DEMO_AWAY_SAMPLES, DEMO_IMPORT_WARNINGS, DEMO_SENSORS, createDemoBackend, demoFormat, demoNextChange, demoOrientation, demoValue } from '../../src/demo-backend.js';
import { DEMO_GIF_THEME } from '../../src/demo-data.js';

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
  const names = (await demo.listThemes()).map((x) => x.name);
  assert.deepEqual(names, ['Demo', 'Mine']);
  assert.deepEqual((await demo.listThemes()).map((x) => x.bundled), [true, false]);
  assert.deepEqual((await demo.listThemes()).map((x) => x.orientation), ['reverse-portrait', 'reverse-portrait']);
  assert.equal((await demo.openTheme('demo://Mine')).name, 'Mine');
  await assert.rejects(demo.openTheme('demo://nope'), (e) => e.code === 'notInLibrary' && e.args.location === 'demo://nope');
  await demo.saveTheme({ ...theme, name: 'Mine', orientation: 'landscape', canvas: { width: 1920, height: 480 } }, false);
  assert.equal((await demo.listThemes()).length, 2);
  assert.equal((await demo.listThemes())[1].orientation, 'landscape', 'saving again updates the entry');
  assert.equal((await demo.openTheme('demo://Mine')).orientation, 'landscape');
  assert.equal((await demo.newTheme()).elements.length, 0);
  assert.equal((await demo.newTheme('k', 'Novo')).name, 'Novo');
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

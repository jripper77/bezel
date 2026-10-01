import { test } from 'node:test';
import assert from 'node:assert/strict';
import { DEMO_CLOSE_EVENT, DEMO_LET_GO_EVENT, DEMO_QUIT_EVENT, STILL, createBridge, parseFrame } from '../../src/bridge.js';

const page = (search = '', hostname = 'localhost') => ({ location: { hostname, search } });

function frameBytes(width, height, next = STILL) {
  const bytes = new Uint8Array(12 + width * height * 4);
  const view = new DataView(bytes.buffer);
  view.setUint32(0, width, true);
  view.setUint32(4, height, true);
  view.setUint32(8, next, true);
  bytes[12] = 0xab;
  return bytes;
}

test('parseFrame reads the header and keeps the pixels', () => {
  const frame = parseFrame(frameBytes(2, 3).buffer);
  assert.equal(frame.width, 2);
  assert.equal(frame.height, 3);
  assert.equal(frame.rgba.length, 24);
  assert.equal(frame.rgba[0], 0xab);
  assert.equal(frame.nextMs, null, 'nothing animates');
});

test('parseFrame says when an animated GIF changes next', () => {
  assert.equal(parseFrame(frameBytes(1, 1, 66)).nextMs, 66);
  assert.equal(parseFrame(frameBytes(1, 1, 0)).nextMs, 0);
});

test('parseFrame accepts a view at an offset', () => {
  const outer = new Uint8Array(4 + 12 + 4);
  outer.set(frameBytes(1, 1), 4);
  const frame = parseFrame(outer.subarray(4));
  assert.deepEqual([frame.width, frame.height, frame.rgba.length], [1, 1, 4]);
});

test('parseFrame rejects short or inconsistent frames', () => {
  assert.throws(() => parseFrame(new Uint8Array(8)), /too short/);
  assert.throws(() => parseFrame(frameBytes(2, 2).subarray(0, 16)), /expected 16/);
});

test('tauri mode maps every call to its command', async () => {
  const calls = [];
  const invoke = async (cmd, args) => {
    calls.push([cmd, args]);
    return cmd === 'render_preview' ? frameBytes(1, 1).buffer : [];
  };
  const bridge = createBridge({ ...page(), __TAURI__: { core: { invoke } } });
  assert.equal(bridge.mode, 'tauri');
  const theme = { name: 'x' };
  await bridge.listDevices();
  await bridge.catalog();
  await bridge.sample();
  await bridge.session();
  const frame = await bridge.render(theme);
  assert.equal(frame.width, 1);
  assert.ok(frame.millis >= 0);
  await bridge.pushTheme(theme);
  await bridge.setLive(true, 'k');
  await bridge.setBrightness('k', 40);
  await bridge.release('k');
  await bridge.saveTheme(theme, true);
  await bridge.listThemes();
  await bridge.openTheme('loc');
  await bridge.newTheme('k', 'Novo', 'landscape');
  await bridge.importTheme();
  await bridge.addImage();
  await bridge.assets();
  await bridge.fonts();
  await bridge.getAutostart();
  await bridge.setAutostart(true);
  await bridge.storageOverview('k');
  await bridge.mediaTools();
  await bridge.locateFfmpeg();
  await bridge.pickMedia();
  await bridge.prepareUpload('k', '/home/me/clip.mp4', 'sd');
  await bridge.prepareThemeVideo('k');
  await bridge.runUpload(7, true);
  await bridge.cancelJob();
  await bridge.deleteStored('k', 'internal/video/a.mp4', true);
  await bridge.playStored('k', 'internal/video/a.mp4');
  await bridge.stopPlayback('k');
  await bridge.setBootMedia('k', null, true);
  await bridge.setBootMedia('k', 'internal/video/a.mp4', true, 40);
  await bridge.setUnsaved(true);
  await bridge.closeWindow();
  await bridge.preferences();
  await bridge.setLanguage('en');
  await bridge.setSensorOptions('1.1.1.1', null);
  await bridge.pickFolder();
  await bridge.leaveDesktopMode('hid:/dev/hidraw7', true);
  await bridge.quitApp();
  await bridge.showSensors(['net.ping', 'cpu.usage']);
  await bridge.restartScreen('k');
  await bridge.addMedia();
  await bridge.addMedia('/home/me/Ondas.gif');
  await bridge.themeThumbnail('/home/me/t.bezeltheme');
  await bridge.setThemeFilter(null, 'vertical');
  assert.deepEqual(calls.map((c) => c[0]), [
    'list_devices', 'sensor_catalog', 'sample_sensors', 'editor_session', 'render_preview', 'push_theme', 'set_live',
    'set_brightness', 'release_screen', 'save_theme', 'list_themes', 'open_theme', 'new_theme', 'import_theme',
    'add_image', 'list_assets', 'list_fonts', 'get_autostart', 'set_autostart',
    'storage_overview', 'media_tools', 'locate_ffmpeg', 'pick_media', 'prepare_upload', 'prepare_theme_video',
    'run_upload', 'cancel_job', 'delete_stored', 'play_stored', 'stop_playback', 'set_boot_media', 'set_boot_media',
    'set_unsaved', 'close_window', 'preferences', 'set_language', 'set_sensor_options', 'pick_folder',
    'leave_desktop_mode', 'quit_app', 'show_sensors', 'restart_screen', 'add_media', 'add_media',
    'theme_thumbnail', 'set_theme_filter',
  ]);
  assert.deepEqual(calls[44][1], { location: '/home/me/t.bezeltheme' });
  assert.deepEqual(calls[45][1], { scope: null, axis: 'vertical' });
  assert.deepEqual(calls[42][1], { path: null }, 'the native dialog asks');
  assert.deepEqual(calls[43][1], { path: '/home/me/Ondas.gif' }, 'a dropped file');
  assert.deepEqual(calls[41][1], { screen: 'k' });
  assert.deepEqual(calls[40][1], { keys: ['net.ping', 'cpu.usage'] });
  assert.deepEqual(calls[35][1], { language: 'en' });
  assert.deepEqual(calls[36][1], { pingHost: '1.1.1.1', mangohudDir: null });
  assert.deepEqual(calls[38][1], { key: 'hid:/dev/hidraw7', confirmed: true });
  assert.deepEqual(calls[32][1], { unsaved: true });
  assert.deepEqual(calls[23][1], { screen: 'k', source: '/home/me/clip.mp4', medium: 'sd' });
  assert.deepEqual(calls[25][1], { ticket: 7, overwrite: true });
  assert.deepEqual(calls[27][1], { screen: 'k', path: 'internal/video/a.mp4', confirmed: true });
  assert.deepEqual(calls[30][1], { screen: 'k', path: null, confirmed: true, brightness: null });
  assert.deepEqual(calls[31][1], { screen: 'k', path: 'internal/video/a.mp4', confirmed: true, brightness: 40 });
  assert.equal(bridge.fileSource({ name: 'x.png' }), null, 'dropped files come with paths from Tauri');
  assert.deepEqual(calls[6][1], { on: true, screen: 'k' });
  assert.deepEqual(calls[7][1], { screen: 'k', percent: 40 });
  assert.deepEqual(calls[9][1], { theme, saveAs: true });
  assert.deepEqual(calls[12][1], { screen: 'k', name: 'Novo', orientation: 'landscape' });
});

test('tauri mode listens to upload progress and system file drops', async () => {
  const listened = [];
  const listen = async (name, cb) => {
    listened.push(name);
    cb({ payload: name === 'storage-progress' ? { phase: 'upload', done: 1, total: 2 } : { paths: ['/a.png'], position: { x: 1, y: 2 } } });
    return () => {};
  };
  const invoke = async () => null;
  const seen = [];
  const bridge = createBridge({ ...page(), __TAURI__: { core: { invoke }, event: { listen } } });
  await bridge.onJobProgress((p) => seen.push(p));
  await bridge.onFileDrop((d) => seen.push(d));
  let asked = 0;
  await bridge.onCloseRequested(() => { asked += 1; });
  assert.equal(asked, 1);
  await bridge.onQuitRequested(() => { asked += 1; });
  assert.equal(asked, 2);
  assert.deepEqual(listened, ['storage-progress', 'tauri://drag-over', 'tauri://drag-drop', 'tauri://drag-leave', 'close-requested', 'quit-requested']);
  assert.deepEqual(seen[0], { phase: 'upload', done: 1, total: 2 });
  assert.deepEqual(seen[2], { type: 'drop', paths: ['/a.png'], position: { x: 1, y: 2 } });

  // The webview API, when the page has it, reports the drop itself.
  const drops = [];
  const webview = { onDragDropEvent: async (cb) => { cb({ payload: { type: 'leave' } }); return () => {}; } };
  const withWebview = createBridge({ ...page(), __TAURI__: { core: { invoke }, webview: { getCurrentWebview: () => webview } } });
  await withWebview.onFileDrop((d) => drops.push(d));
  assert.deepEqual(drops, [{ type: 'leave' }]);
  const bare = createBridge({ ...page(), __TAURI__: { core: { invoke } } });
  assert.equal(typeof (await bare.onJobProgress(() => {})), 'function');
  assert.equal(typeof (await bare.onFileDrop(() => {})), 'function');
  assert.equal(typeof (await bare.onCloseRequested(() => {})), 'function');
  assert.equal(typeof (await bare.onQuitRequested(() => {})), 'function');
});

test('demo mode shows what the window does and takes the close button as an event', async () => {
  const listeners = {};
  const attributes = {};
  const win = {
    ...page(),
    addEventListener: (name, cb) => { listeners[name] = cb; },
    document: { documentElement: { setAttribute: (k, v) => { attributes[k] = v; } } },
  };
  const bridge = createBridge(win);
  let asked = 0;
  await bridge.onCloseRequested(() => { asked += 1; });
  await bridge.setUnsaved(true);
  listeners[DEMO_CLOSE_EVENT]();
  assert.equal(asked, 1, 'unsaved edits: the UI asks');
  await bridge.closeWindow();
  assert.equal(attributes['data-demo-window'], 'closed');
  let quitting = 0;
  await bridge.onQuitRequested(() => { quitting += 1; });
  listeners[DEMO_QUIT_EVENT]();
  assert.equal(quitting, 1, 'the tray Quit asks too');
  await bridge.quitApp();
  assert.equal(attributes['data-demo-window'], 'quit');
});

test('demo mode shows on the page which sensors the list shows', async () => {
  const attributes = {};
  const bridge = createBridge({ ...page(), document: { documentElement: { setAttribute: (k, v) => { attributes[k] = v; } } } });
  await bridge.showSensors(['net.ping', 'cpu.usage']);
  assert.equal(attributes['data-demo-sensors'], 'net.ping cpu.usage');
  await bridge.showSensors([]);
  assert.equal(attributes['data-demo-sensors'], '');
});

test('demo mode with hold lets each upload phase go on a window event', async () => {
  const listeners = {};
  const bridge = createBridge({ ...page('?demo=turing88&hold'), addEventListener: (name, cb) => { listeners[name] = cb; } });
  const phases = [];
  bridge.onJobProgress((p) => phases.push(p.phase));
  const ready = await bridge.prepareUpload('/dev/ttyACM1', await bridge.pickMedia(), 'internal');
  listeners[DEMO_LET_GO_EVENT]();
  listeners[DEMO_LET_GO_EVENT]();
  const done = await bridge.runUpload(ready.ticket, false);
  assert.equal(done.status, 'done');
  assert.deepEqual([...new Set(phases)], ['convert', 'upload', 'verify']);
});

test('demo mode serves scenarios as copies', async () => {
  const bridge = createBridge(page('?demo=two'));
  assert.equal(bridge.mode, 'demo');
  const a = (await bridge.listDevices()).screens;
  a[0].key = 'mutated';
  const b = (await bridge.listDevices()).screens;
  assert.equal(b.length, 2);
  assert.equal(b[0].key, '/dev/ttyACM1');
});

test('demo defaults to the Turing 8.8" and reports scenario errors', async () => {
  assert.equal((await createBridge(page()).listDevices()).screens.length, 1);
  assert.equal((await createBridge(page('?demo=nope')).listDevices()).screens.length, 1);
  await assert.rejects(createBridge(page('?demo=error')).listDevices(), /permission denied/);
});

test('outside localhost without Tauri there is no backend', async () => {
  const bridge = createBridge(page('', 'example.com'));
  assert.equal(bridge.mode, 'unavailable');
  await assert.rejects(bridge.listDevices(), /no backend/);
});

test('the demo serves every call the app does', () => {
  const tauri = createBridge({ ...page(), __TAURI__: { core: { invoke: async () => null } } });
  const demo = createBridge(page());
  // Files dropped from the system reach only the app (Tauri owns that drag).
  const appOnly = ['onFileDrop'];
  const missing = Object.keys(tauri).filter((k) => typeof tauri[k] === 'function' && typeof demo[k] !== 'function' && !appOnly.includes(k));
  assert.deepEqual(missing, []);
});

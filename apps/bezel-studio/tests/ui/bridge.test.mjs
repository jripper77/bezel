import { test } from 'node:test';
import assert from 'node:assert/strict';
import { DEMO_CLOSE_EVENT, createBridge, parseFrame } from '../../src/bridge.js';

const page = (search = '', hostname = 'localhost') => ({ location: { hostname, search } });

function frameBytes(width, height) {
  const bytes = new Uint8Array(8 + width * height * 4);
  const view = new DataView(bytes.buffer);
  view.setUint32(0, width, true);
  view.setUint32(4, height, true);
  bytes[8] = 0xab;
  return bytes;
}

test('parseFrame reads the header and keeps the pixels', () => {
  const frame = parseFrame(frameBytes(2, 3).buffer);
  assert.equal(frame.width, 2);
  assert.equal(frame.height, 3);
  assert.equal(frame.rgba.length, 24);
  assert.equal(frame.rgba[0], 0xab);
});

test('parseFrame accepts a view at an offset', () => {
  const outer = new Uint8Array(4 + 8 + 4);
  outer.set(frameBytes(1, 1), 4);
  const frame = parseFrame(outer.subarray(4));
  assert.deepEqual([frame.width, frame.height, frame.rgba.length], [1, 1, 4]);
});

test('parseFrame rejects short or inconsistent frames', () => {
  assert.throws(() => parseFrame(new Uint8Array(4)), /too short/);
  assert.throws(() => parseFrame(frameBytes(2, 2).subarray(0, 12)), /expected 16/);
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
  await bridge.listScreens();
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
  assert.deepEqual(calls.map((c) => c[0]), [
    'list_screens', 'sensor_catalog', 'sample_sensors', 'editor_session', 'render_preview', 'push_theme', 'set_live',
    'set_brightness', 'release_screen', 'save_theme', 'list_themes', 'open_theme', 'new_theme', 'import_theme',
    'add_image', 'list_assets', 'list_fonts', 'get_autostart', 'set_autostart',
    'storage_overview', 'media_tools', 'locate_ffmpeg', 'pick_media', 'prepare_upload', 'prepare_theme_video',
    'run_upload', 'cancel_job', 'delete_stored', 'play_stored', 'stop_playback', 'set_boot_media', 'set_boot_media',
    'set_unsaved', 'close_window',
  ]);
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
  assert.deepEqual(listened, ['storage-progress', 'tauri://drag-over', 'tauri://drag-drop', 'tauri://drag-leave', 'close-requested']);
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
});

test('demo mode serves scenarios as copies', async () => {
  const bridge = createBridge(page('?demo=two'));
  assert.equal(bridge.mode, 'demo');
  const a = await bridge.listScreens();
  a[0].key = 'mutated';
  const b = await bridge.listScreens();
  assert.equal(b.length, 2);
  assert.equal(b[0].key, '/dev/ttyACM1');
});

test('demo defaults to the Turing 8.8" and reports scenario errors', async () => {
  assert.equal((await createBridge(page()).listScreens()).length, 1);
  assert.equal((await createBridge(page('?demo=nope')).listScreens()).length, 1);
  await assert.rejects(createBridge(page('?demo=error')).listScreens(), /permission denied/);
});

test('outside localhost without Tauri there is no backend', async () => {
  const bridge = createBridge(page('', 'example.com'));
  assert.equal(bridge.mode, 'unavailable');
  await assert.rejects(bridge.listScreens(), /no backend/);
});

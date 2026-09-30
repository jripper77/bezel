import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createBridge, parseFrame } from '../../src/bridge.js';

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
  await bridge.newTheme('k');
  await bridge.importTheme();
  await bridge.addImage();
  await bridge.assets();
  await bridge.fonts();
  await bridge.getAutostart();
  await bridge.setAutostart(true);
  assert.deepEqual(calls.map((c) => c[0]), [
    'list_screens', 'sensor_catalog', 'sample_sensors', 'editor_session', 'render_preview', 'push_theme', 'set_live',
    'set_brightness', 'release_screen', 'save_theme', 'list_themes', 'open_theme', 'new_theme', 'import_theme',
    'add_image', 'list_assets', 'list_fonts', 'get_autostart', 'set_autostart',
  ]);
  assert.deepEqual(calls[6][1], { on: true, screen: 'k' });
  assert.deepEqual(calls[7][1], { screen: 'k', percent: 40 });
  assert.deepEqual(calls[9][1], { theme, saveAs: true });
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

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createBridge } from '../../src/bridge.js';

const page = (search = '', hostname = 'localhost') => ({ location: { hostname, search } });

test('tauri mode invokes the command', async () => {
  const calls = [];
  const bridge = createBridge({ ...page(), __TAURI__: { core: { invoke: async (cmd) => (calls.push(cmd), []) } } });
  assert.equal(bridge.mode, 'tauri');
  assert.deepEqual(await bridge.listScreens(), []);
  assert.deepEqual(calls, ['list_screens']);
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

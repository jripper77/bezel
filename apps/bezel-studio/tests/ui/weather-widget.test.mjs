import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createWidget, widgetOf, WIDGETS } from '../../src/editor/widgets.js';
import { createStore } from '../../src/editor/store.js';
import { DEMO_THEME } from '../../src/demo-theme.js';

test('weather is a portable editor object and its configuration survives copy, paste and Undo', () => {
  assert.ok(WIDGETS.includes('weather'));
  const made = createWidget('weather', { width: 480, height: 1920 });
  assert.equal(widgetOf({ kind: made.kind }), 'weather');
  assert.equal(made.kind.content.showIcon, true);
  const store = createStore(DEMO_THEME);
  store.dispatch('add', { widget: 'weather', x: 240, y: 400 });
  const id = store.getState().selection[0];
  store.dispatch('update', { id, patch: { kind: { content: { city: 'Milano', latitude: 45.4642, longitude: 9.19, language: 'it', fahrenheit: true } } } });
  const before = store.getState().theme;
  store.copySelection(); store.paste();
  assert.deepEqual(store.getState().theme.elements.at(-1).kind, before.elements.at(-1).kind);
  store.undo(); assert.equal(store.getState().theme, before);
});


test('weather examples use Italian condition names and correct Fahrenheit conversion', async () => {
  const { weatherFamily, weatherText } = await import('../../src/weather-format.js');
  assert.equal(weatherText({ city: 'Roma', language: 'it', fahrenheit: true }, 20, 3), 'Roma\n68\u00b0F\nNuvoloso');
  assert.equal(weatherText({ city: 'Roma' }, 0, 0, 'it-IT'), 'Roma\n0\u00b0C\nSereno');
  for (const [code, family] of [[0, 0], [2, 1], [3, 2], [48, 3], [61, 4], [85, 5], [95, 6], [999, 7]]) assert.equal(weatherFamily(code), family);
});

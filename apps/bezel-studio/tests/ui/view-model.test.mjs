import { test } from 'node:test';
import assert from 'node:assert/strict';
import { countLabel, detailRows, fitPreview, screenCard, soleModel } from '../../src/view-model.js';
import { SCENARIOS } from '../../src/demo-data.js';
import { translator } from '../../src/i18n/index.js';

const t = translator('en');
const [turing88, asleep] = SCENARIOS.two.screens;

test('a known model makes a titled card with its resolution', () => {
  const card = screenCard(turing88, t);
  assert.equal(card.title, 'Turing Smart Screen 8.8"');
  assert.equal(card.meta, '480×1920 · 8.8"');
  assert.equal(card.stateLabel, 'awake');
});

test('several candidates are listed until the handshake decides', () => {
  assert.equal(soleModel(asleep), null);
  const card = screenCard(asleep, t);
  assert.equal(card.title, 'Turing Smart Screen 2.1" / Turing Smart Screen 2.8"');
  assert.equal(card.meta, 'Model confirmed on connect');
  assert.equal(card.stateLabel, 'asleep');
});

test('count label pluralises', () => {
  assert.equal(countLabel(0, t), 'No screen connected');
  assert.equal(countLabel(1, t), '1 screen connected');
  assert.equal(countLabel(3, t), '3 screens connected');
});

test('preview keeps the panel proportions inside the box', () => {
  assert.deepEqual(fitPreview({ width: 480, height: 1920 }, { width: 400, height: 600 }), { width: 150, height: 600 });
  assert.deepEqual(fitPreview({ width: 480, height: 480 }, { width: 400, height: 600 }), { width: 400, height: 400 });
  assert.deepEqual(fitPreview({ width: 0, height: 10 }, { width: 400, height: 600 }), { width: 0, height: 0 });
  assert.deepEqual(fitPreview({ width: 10, height: 10 }, { width: 0, height: 600 }), { width: 0, height: 0 });
});

test('detail rows describe endpoints and capabilities', () => {
  const rows = Object.fromEntries(detailRows(turing88, t).map((r) => [r.label, r.value]));
  assert.equal(rows.Resolution, '480×1920');
  assert.equal(rows['Display port'], '/dev/ttyACM1 (0525:a4a7)');
  assert.equal(rows['Wake port'], '/dev/ttyACM0 (1a86:ca88, CT88INCH)');
  assert.equal(rows.Capabilities, 'brightness, partial update, storage, on-screen video');
  assert.equal(rows['Hardware validated'], 'no');

  const asleepRows = Object.fromEntries(detailRows(asleep, t).map((r) => [r.label, r.value]));
  assert.equal(asleepRows.Model, 'Model confirmed on connect');
  assert.equal(asleepRows['Display port'], '—');
  assert.equal(asleepRows.Capabilities, '—');
});

test('a validated model with no capabilities', () => {
  const bare = structuredClone(turing88);
  bare.models[0].hardwareValidated = true;
  bare.models[0].capabilities = {};
  const rows = Object.fromEntries(detailRows(bare, t).map((r) => [r.label, r.value]));
  assert.equal(rows.Capabilities, '—');
  assert.equal(rows['Hardware validated'], 'yes');
});

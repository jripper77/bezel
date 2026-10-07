import test from 'node:test';
import assert from 'node:assert/strict';
import { libreStatus } from '../../src/libre-status.js';

const catalog = [
  { key: 'cpu.temperature', category: 'cpu', label: 'CPU Package', source: 'LibreHardwareMonitor', quantity: 'celsius' },
  { key: 'lhm.psu.corsair.0.voltage.3', category: 'board', label: 'Corsair Input', source: 'LibreHardwareMonitor', quantity: 'volts' },
  { key: 'lhm.psu.corsair.0.power.14', category: 'board', label: 'Corsair Total Output', source: 'LibreHardwareMonitor', quantity: 'watts' },
];

test('a working CPU cannot hide a failed Corsair reporting a synthetic zero', () => {
  const health = libreStatus(catalog, {
    'cpu.temperature': { value: 53 },
    'lhm.psu.corsair.0.voltage.3': { unavailable: 'No reply' },
    'lhm.psu.corsair.0.power.14': { value: 0 },
  });
  assert.equal(health.state, 'partial');
  assert.deepEqual(health.failed, ['Corsair Input']);
});

test('missing/stale readings fail; legitimate zero measurements are healthy', () => {
  assert.equal(libreStatus(catalog, null).state, 'error');
  assert.equal(libreStatus(catalog, {
    'cpu.temperature': { value: 53, unavailable: 'Stale' },
    'lhm.psu.corsair.0.voltage.3': { value: Number.NaN },
  }).state, 'error');
  assert.equal(libreStatus(catalog, {
    'cpu.temperature': { value: 0 },
    'lhm.psu.corsair.0.voltage.3': { value: 230 },
  }).state, 'ok');
  assert.equal(libreStatus([], {}), null);
});

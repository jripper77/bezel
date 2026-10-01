import { test } from 'node:test';
import assert from 'node:assert/strict';
import { axisOf, groupSensors, shownSensorKeys, thumbScreen } from '../../src/ui/library.js';
import { joinColor, splitColor } from '../../src/ui/fields.js';

const catalog = [
  { key: 'net.down', category: 'network', label: 'Download' },
  { key: 'cpu.usage', category: 'cpu', label: 'CPU usage' },
  { key: 'x.y', category: 'zeta', label: 'Odd' },
  { key: 'gpu.usage', category: 'gpu', label: 'GPU usage' },
  { key: 'x.z', category: 'alpha', label: 'Other' },
];

test('sensors group by category in display order, unknown ones last', () => {
  assert.deepEqual(groupSensors(catalog).map(([c]) => c), ['cpu', 'gpu', 'network', 'alpha', 'zeta']);
});

test('the filter matches label or key, ignoring case', () => {
  assert.deepEqual(groupSensors(catalog, ' USAGE ').map(([c, items]) => [c, items.length]), [['cpu', 1], ['gpu', 1]]);
  assert.deepEqual(groupSensors(catalog, 'net.').map(([c]) => c), ['network']);
  assert.deepEqual(groupSensors(catalog, 'nothing'), []);
});

test('the list shows the filtered sensors while open, and none while closed', () => {
  const withPing = [...catalog, { key: 'net.ping', category: 'network', label: 'Ping' }];
  assert.deepEqual(shownSensorKeys(withPing, '', true), ['cpu.usage', 'gpu.usage', 'net.down', 'net.ping', 'x.z', 'x.y']);
  assert.deepEqual(shownSensorKeys(withPing, 'usage', true), ['cpu.usage', 'gpu.usage'], 'the ping filtered out');
  assert.deepEqual(shownSensorKeys(withPing, 'ping', true), ['net.ping']);
  assert.deepEqual(shownSensorKeys(withPing, '', false), [], 'another tab, or the window hidden');
});

test('colors split into rgb and alpha percent and join back', () => {
  assert.deepEqual(splitColor('#FF8800CC'), { rgb: '#ff8800', alpha: 80 });
  assert.deepEqual(splitColor('#102030'), { rgb: '#102030', alpha: 100 });
  assert.deepEqual(splitColor(undefined), { rgb: '#ffffff', alpha: 100 });
  assert.equal(joinColor('#FF8800', 80), '#ff8800cc');
  assert.equal(joinColor('#000000', 150), '#000000ff');
  assert.equal(joinColor('#000000', -5), '#00000000');
});

test('theme cards know vertical from horizontal and draw the screen shape', () => {
  assert.equal(axisOf({ orientation: 'reverse-landscape', canvas: { width: 1920, height: 480 } }), 'horizontal');
  assert.equal(axisOf({ orientation: 'portrait', canvas: { width: 480, height: 1920 } }), 'vertical');
  assert.equal(axisOf({ canvas: { width: 800, height: 480 } }), 'horizontal', 'by shape without an orientation');
  assert.equal(axisOf({ canvas: { width: 480, height: 480 } }), 'vertical');
  assert.deepEqual(thumbScreen({ width: 480, height: 1920 }), { width: 15, height: 80 });
  assert.deepEqual(thumbScreen({ width: 1920, height: 480 }), { width: 80, height: 26.7 });
  assert.deepEqual(thumbScreen({ width: 480, height: 480 }), { width: 60, height: 80 });
});

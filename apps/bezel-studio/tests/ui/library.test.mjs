import { test } from 'node:test';
import assert from 'node:assert/strict';
import { groupSensors } from '../../src/ui/library.js';
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

test('colors split into rgb and alpha percent and join back', () => {
  assert.deepEqual(splitColor('#FF8800CC'), { rgb: '#ff8800', alpha: 80 });
  assert.deepEqual(splitColor('#102030'), { rgb: '#102030', alpha: 100 });
  assert.deepEqual(splitColor(undefined), { rgb: '#ffffff', alpha: 100 });
  assert.equal(joinColor('#FF8800', 80), '#ff8800cc');
  assert.equal(joinColor('#000000', 150), '#000000ff');
  assert.equal(joinColor('#000000', -5), '#00000000');
});

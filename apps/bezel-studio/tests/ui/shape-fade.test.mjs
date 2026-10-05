import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fadeLine } from '../../src/shape-fade.js';
test('linear transparency spans non-square boxes at horizontal, vertical and diagonal angles', () => {
  const f = { x: 10, y: 20, width: 80, height: 40 };
  const close = (actual, expected) => actual.forEach((x, i) => assert.ok(Math.abs(x - expected[i]) < 1e-8));
  close(fadeLine(f, 0), [10, 40, 90, 40]);
  close(fadeLine(f, 90), [50, 20, 50, 60]);
  close(fadeLine(f, 180), [90, 40, 10, 40]);
  const d = fadeLine(f, 45);
  close([(d[0] + d[2]) / 2, (d[1] + d[3]) / 2], [50, 40]);
});

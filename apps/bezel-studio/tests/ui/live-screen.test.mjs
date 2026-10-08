import { test } from 'node:test';
import assert from 'node:assert/strict';
import { answersTo, deviceLabel, liveScreenIn } from '../../src/live-screen.js';
import { SCENARIOS } from '../../src/demo-data.js';

// The 8.8" (display /dev/ttyACM1, MCU /dev/ttyACM0) beside a 2.1" asleep,
// listed by its MCU, and a TURZX screen with no wake chip.
const [turing88, asleep21] = SCENARIOS.two.screens;
const [turzx] = SCENARIOS.turzx.screens;
const screens = [turing88, asleep21, turzx];

test('the live screen named by its MCU port is the listed one, by its listed key', () => {
  assert.equal(liveScreenIn(screens, '/dev/ttyACM0'), turing88);
  assert.equal(liveScreenIn(screens, '/dev/ttyACM0').key, '/dev/ttyACM1');
  assert.equal(liveScreenIn(screens, '/dev/ttyACM1'), turing88, 'by its display, its key');
  assert.equal(liveScreenIn(screens, 'COM3'), asleep21, 'asleep: listed by its MCU');
  assert.equal(liveScreenIn(screens, '3-1.4'), turzx, 'no wake chip: by its display');
});

test('an unlisted live key selects no screen', () => {
  assert.equal(liveScreenIn(screens, '/dev/ttyACM9'), null);
  assert.equal(liveScreenIn([], '/dev/ttyACM0'), null, 'nothing listed');
});

test('no live key names no screen', () => {
  assert.equal(liveScreenIn(screens, null), null);
  assert.equal(liveScreenIn(screens, undefined), null);
  assert.equal(liveScreenIn(screens, ''), null);
});

test('a screen answers to its key, its display and its wake chip only', () => {
  assert.ok(answersTo(turing88, '/dev/ttyACM1'));
  assert.ok(answersTo(turing88, '/dev/ttyACM0'));
  assert.ok(!answersTo(turing88, 'COM3'));
  assert.ok(answersTo(asleep21, 'COM3'));
  assert.ok(!answersTo(turzx, '/dev/ttyACM0'), 'no wake chip');
  assert.ok(!answersTo({ key: 'k', display: null, wake: null }, 'x'));
});

test('a listed key wins over another screen\'s port', () => {
  // A screen keyed by an address another one also answers to (never seen on
  // hardware): the one listed by it is the live one.
  const odd = { ...asleep21, key: '/dev/ttyACM0', wake: { ...asleep21.wake, address: '/dev/ttyACM0' } };
  assert.equal(liveScreenIn([turing88, odd], '/dev/ttyACM0'), odd);
});

test('ambiguous models retain a readable family when USB metadata is absent', () => {
  assert.equal(deviceLabel({ key: 'COM3', family: 'turing-rev-a', models: [{ name: 'A' }, { name: 'B' }], display: {} }), 'Turing / UsbPCMonitor (Rev A) · COM3');
  assert.equal(deviceLabel({ key: 'COM9', models: [], display: {}, family: 'unrecognized' }), 'COM9');
});

test('the protocol family wins over Windows generic driver names', () => {
  const screen = { key: 'COM3', family: 'turing-rev-a', models: [{ name: 'A' }, { name: 'B' }], display: { manufacturer: 'Microsoft', product: 'Dispositivo seriale USB (COM3)' } };
  assert.equal(deviceLabel(screen), 'Turing / UsbPCMonitor (Rev A) · COM3');
});

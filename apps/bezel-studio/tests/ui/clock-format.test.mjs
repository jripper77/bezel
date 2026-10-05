import test from 'node:test';
import assert from 'node:assert/strict';
import { CLOCK_PATTERNS, formatClock } from '../../src/clock-format.js';
const date = new Date(2026, 9, 5, 14, 30, 45);
test('date format, explicit language, system language and Unicode case', () => {
  assert.equal(formatClock('%A %e %B', date, 'it'), 'lunedì 5 ottobre');
  assert.equal(formatClock('%A %B', date, null, 'normal', 'it-IT'), 'lunedì ottobre');
  assert.equal(formatClock('%a %b', date, 'it', 'upper'), 'LUN OTT');
  assert.equal(formatClock('%A %e %B', date, 'it', 'title'), 'Lunedì 5 Ottobre');
  assert.equal(formatClock('%A %e %B', date, 'en'), 'Monday 5 October');
  assert.equal(formatClock('%A %B', date, 'pt-BR'), 'segunda-feira outubro');
  assert.equal(formatClock('%H:%M:%S %I %p %d/%m/%Y %y %% %q %', date, 'it'), '14:30:45 02 PM 05/10/2026 26 % %q %');
  assert.equal(formatClock('%I %p', new Date(2026, 0, 1, 0), 'it'), '12 AM');
  assert.equal(formatClock('%A', date, null, 'normal', 'fr'), 'Monday');
  assert.ok(CLOCK_PATTERNS.includes('%A %e %B'));
  assert.ok(formatClock('%H:%M'));
});

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { AXES, SCOPES, SHOW_ALL, axisOf, countText, emptyState, filterThemes, fitsScreen, rememberedFilter, scopeIn, screenLabel, thumbnailKey } from '../../src/theme-filter.js';
import { LOCALES } from '../../src/i18n/index.js';

// A library like the bundled one: the 8.8" both ways, a 3.5" and a square.
const wide = { name: 'Wide', location: '/t/wide', canvas: { width: 1920, height: 480 }, orientation: 'landscape', models: ['turing-8.8', 'turing-usb-8.8'], diagonalHundredths: 880, revision: 'a1' };
const tall = { name: 'Tall', location: '/t/tall', canvas: { width: 480, height: 1920 }, orientation: 'reverse-portrait', models: ['turing-8.8', 'turing-usb-8.8'], diagonalHundredths: 880, revision: 'b2' };
const small = { name: 'Small', location: '/t/small', canvas: { width: 320, height: 480 }, orientation: 'portrait', models: ['turing-3.5'], diagonalHundredths: 350 };
const square = { name: 'Square', location: '/t/square', canvas: { width: 480, height: 480 }, orientation: 'portrait', models: ['turing-2.1', 'turing-2.8'], diagonalHundredths: null };
const odd = { name: 'Odd', location: '/t/odd', canvas: { width: 333, height: 777 }, orientation: 'portrait', models: [] };
const library = [wide, tall, small, square, odd];

const turing88 = { key: '/dev/ttyACM1', models: [{ id: 'turing-8.8' }] };
const asleep21 = { key: 'COM3', models: [{ id: 'turing-2.1' }, { id: 'turing-2.8' }] };
const names = (list) => list.map((e) => e.name);

test('the remembered filter keeps known choices only', () => {
  assert.deepEqual(rememberedFilter(null), { scope: null, axis: 'all' });
  assert.deepEqual(rememberedFilter({ scope: 'all', axis: 'vertical' }), { scope: 'all', axis: 'vertical' });
  assert.deepEqual(rememberedFilter({ scope: 'screen', axis: 'horizontal' }), { scope: 'screen', axis: 'horizontal' });
  assert.deepEqual(rememberedFilter({ scope: 'mine', axis: 'diagonal' }), { scope: null, axis: 'all' });
  assert.deepEqual([SCOPES, AXES], [['screen', 'all'], ['all', 'vertical', 'horizontal']]);
});

test('"for this screen" is the default once a screen is known, never without one', () => {
  const unchosen = rememberedFilter(null);
  assert.equal(scopeIn(unchosen, turing88), 'screen');
  assert.equal(scopeIn(unchosen, null), 'all');
  assert.equal(scopeIn({ scope: 'all', axis: 'all' }, turing88), 'all', 'the choice stays');
  assert.equal(scopeIn({ scope: 'screen', axis: 'all' }, null), 'all', 'nothing to fit without a screen');
});

test('a theme fits a screen when one of its models may be the screen', () => {
  assert.ok(fitsScreen(wide, turing88));
  assert.ok(fitsScreen(tall, turing88));
  assert.ok(!fitsScreen(small, turing88));
  assert.ok(fitsScreen(square, asleep21), 'any candidate model');
  assert.ok(!fitsScreen(odd, asleep21));
  assert.ok(!fitsScreen({ ...wide, models: undefined }, turing88));
  assert.ok(!fitsScreen(wide, null));
});

test('the filter narrows by screen and by orientation', () => {
  const unchosen = rememberedFilter(null);
  assert.deepEqual(names(filterThemes(library, unchosen, turing88)), ['Wide', 'Tall']);
  assert.deepEqual(names(filterThemes(library, unchosen, null)), names(library), 'all without a screen');
  assert.deepEqual(names(filterThemes(library, { scope: 'all', axis: 'all' }, turing88)), names(library));
  assert.deepEqual(names(filterThemes(library, { scope: null, axis: 'horizontal' }, turing88)), ['Wide']);
  assert.deepEqual(names(filterThemes(library, { scope: 'all', axis: 'vertical' }, turing88)), ['Tall', 'Small', 'Square', 'Odd']);
  assert.deepEqual(names(filterThemes(library, { scope: 'screen', axis: 'vertical' }, asleep21)), ['Square']);
  assert.deepEqual(names(filterThemes(library, { ...SHOW_ALL }, asleep21)), names(library));
});

test('theme cards know vertical from horizontal', () => {
  assert.equal(axisOf(wide), 'horizontal');
  assert.equal(axisOf(tall), 'vertical');
  assert.equal(axisOf({ canvas: { width: 800, height: 480 } }), 'horizontal', 'by shape without an orientation');
  assert.equal(axisOf({ canvas: { width: 480, height: 480 } }), 'vertical');
});

test('an empty gallery says why and offers what brings themes back', () => {
  const horizontalHere = { scope: null, axis: 'horizontal' };
  assert.deepEqual(emptyState([small], horizontalHere, turing88), { key: 'themes.noneForScreenAxis', showAll: true });
  assert.deepEqual(emptyState([small], rememberedFilter(null), turing88), { key: 'themes.noneForScreen', showAll: true });
  assert.deepEqual(emptyState([wide], { scope: 'all', axis: 'vertical' }, turing88), { key: 'themes.noneForAxis', showAll: true });
  assert.deepEqual(emptyState([small], { scope: 'screen', axis: 'all' }, null), { key: 'themes.empty', showAll: false }, 'nothing filtered: an empty library');
  assert.deepEqual(emptyState([], horizontalHere, turing88), { key: 'themes.empty', showAll: false });
  for (const key of ['themes.noneForScreenAxis', 'themes.noneForScreen', 'themes.noneForAxis', 'themes.empty', 'themes.showAll']) {
    assert.ok(key in LOCALES.en && key in LOCALES['pt-BR'], key);
  }
  assert.equal(LOCALES['pt-BR']['themes.noneForScreenAxis'], 'Nenhum tema para esta tela nesta orientação.');
});

test('the count says how many themes the filter shows', () => {
  assert.equal(countText(0, 5), null);
  assert.deepEqual(countText(2, 5), { key: 'themes.countSome', params: { shown: 2, total: 5 } });
  assert.deepEqual(countText(5, 5), { key: 'themes.countAll', params: { count: 5 } });
  assert.deepEqual(countText(1, 1), { key: 'themes.countOne', params: {} });
  for (const key of ['themes.countSome', 'themes.countAll', 'themes.countOne']) assert.ok(key in LOCALES.en, key);
});

test('cards name the screen a theme was made for, in the UI language', () => {
  assert.equal(screenLabel(wide, 'en'), '8.8″ · 1920×480');
  assert.equal(screenLabel(wide, 'pt-BR'), '8,8″ · 1920×480');
  assert.equal(screenLabel(small, 'pt-BR'), '3,5″ · 320×480');
  assert.equal(screenLabel({ ...small, diagonalHundredths: 96 }, 'en'), '0.96″ · 320×480');
  assert.equal(screenLabel({ ...small, diagonalHundredths: 500 }, 'pt-BR'), '5″ · 320×480');
  assert.equal(screenLabel(square, 'en'), '480×480', 'several sizes: only the pixels');
  assert.equal(screenLabel(odd, 'en'), '333×777');
});

test('a thumbnail is kept per location and revision', () => {
  assert.equal(thumbnailKey(wide), '/t/wide\na1');
  assert.notEqual(thumbnailKey(wide), thumbnailKey({ ...wide, revision: 'a2' }), 'saved again');
  assert.equal(thumbnailKey(small), '/t/small\n');
});

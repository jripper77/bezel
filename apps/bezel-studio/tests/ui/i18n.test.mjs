import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { LOCALES, applyTranslations, pickLocale, translator } from '../../src/i18n/index.js';

test('every locale has exactly the same keys', () => {
  const [base, ...rest] = Object.values(LOCALES).map((table) => Object.keys(table).sort());
  for (const keys of rest) assert.deepEqual(keys, base);
});

test('locale choice follows the browser languages', () => {
  assert.equal(pickLocale(['pt-BR', 'en']), 'pt-BR');
  assert.equal(pickLocale(['pt-PT']), 'pt-BR');
  assert.equal(pickLocale(['de', 'en-US']), 'en');
  assert.equal(pickLocale(['de']), 'en');
  assert.equal(pickLocale([]), 'en');
});

test('translator substitutes params and falls back', () => {
  const t = translator('pt-BR');
  assert.equal(t('screens.count.other', { count: 2 }), '2 telas conectadas');
  assert.equal(t('screens.count.other'), '{count} telas conectadas');
  assert.equal(t('no.such.key'), 'no.such.key');
  assert.equal(translator('xx')('yes'), 'yes');
});

test('every data-i18n key used in index.html exists', () => {
  const html = readFileSync(new URL('../../src/index.html', import.meta.url), 'utf8');
  const keys = [...html.matchAll(/data-i18n(?:-title|-aria-label)?="([^"]+)"/g)].map((m) => m[1]);
  assert.ok(keys.length > 5);
  for (const key of keys) assert.ok(key in LOCALES.en, key);
});

test('applyTranslations fills text, title and aria-label', () => {
  const el = (dataset) => ({ dataset, textContent: '', title: '', attrs: {}, setAttribute(k, v) { this.attrs[k] = v; } });
  const text = el({ i18n: 'yes' });
  const title = el({ i18nTitle: 'no' });
  const aria = el({ i18nAriaLabel: 'screens.title' });
  const root = {
    querySelectorAll: (sel) => ({ '[data-i18n]': [text], '[data-i18n-title]': [title], '[data-i18n-aria-label]': [aria] })[sel],
  };
  applyTranslations(root, translator('en'));
  assert.equal(text.textContent, 'yes');
  assert.equal(title.title, 'no');
  assert.equal(aria.attrs['aria-label'], 'Screens');
});

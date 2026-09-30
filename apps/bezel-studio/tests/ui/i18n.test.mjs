import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { LOCALES, applyTranslations, pickLocale, translator } from '../../src/i18n/index.js';
import { WIDGETS } from '../../src/editor/widgets.js';

const src = new URL('../../src/', import.meta.url);

function sources(dir = src) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((d) => {
    const url = new URL(d.name + (d.isDirectory() ? '/' : ''), dir);
    if (d.isDirectory()) return d.name === 'i18n' ? [] : sources(url);
    return d.name.endsWith('.js') ? [readFileSync(url, 'utf8')] : [];
  });
}

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
  assert.equal(t('inspector.multi', { count: 2 }), '2 elementos selecionados');
  assert.equal(t('inspector.multi'), '{count} elementos selecionados');
  assert.equal(t('no.such.key'), 'no.such.key');
  assert.equal(translator('xx')('top.save'), 'Save');
});

test('every static key used by the code exists', () => {
  const keys = sources().flatMap((code) => [...code.matchAll(/\bt\(\s*'([^'$]+)'/g)].map((m) => m[1]));
  assert.ok(keys.length > 50);
  for (const key of keys) assert.ok(key in LOCALES.en, key);
});

test('every widget has a name', () => {
  for (const w of WIDGETS) assert.ok(`widget.${w}` in LOCALES.en, w);
});

test('every data-i18n key used in index.html exists', () => {
  const html = readFileSync(new URL('index.html', src), 'utf8');
  const keys = [...html.matchAll(/data-i18n(?:-title|-aria-label|-placeholder)?="([^"]+)"/g)].map((m) => m[1]);
  assert.ok(keys.length > 20);
  for (const key of keys) assert.ok(key in LOCALES.en, key);
});

test('applyTranslations fills text, title, aria-label and placeholder', () => {
  const el = (dataset) => ({ dataset, textContent: '', title: '', placeholder: '', attrs: {}, setAttribute(k, v) { this.attrs[k] = v; } });
  const text = el({ i18n: 'top.save' });
  const title = el({ i18nTitle: 'top.undo' });
  const aria = el({ i18nAriaLabel: 'library.title' });
  const hint = el({ i18nPlaceholder: 'library.searchSensors' });
  const root = {
    querySelectorAll: (sel) => ({ '[data-i18n]': [text], '[data-i18n-title]': [title], '[data-i18n-aria-label]': [aria], '[data-i18n-placeholder]': [hint] })[sel],
  };
  applyTranslations(root, translator('en'));
  assert.equal(text.textContent, 'Save');
  assert.equal(title.title, 'Undo (Ctrl+Z)');
  assert.equal(aria.attrs['aria-label'], 'Library');
  assert.equal(hint.placeholder, 'Search sensors');
});

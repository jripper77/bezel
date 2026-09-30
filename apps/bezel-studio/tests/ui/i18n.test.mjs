import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { LOCALES, applyTranslations, pickLocale, translator } from '../../src/i18n/index.js';
import { WIDGETS } from '../../src/editor/widgets.js';
import { LANGUAGES, languageOptions } from '../../src/ui/preferences.js';
import { DEMO_FOLDER, createDemoBackend } from '../../src/demo-backend.js';

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

/** String and template literals of `code`: where each starts and ends, and its text (`\0` for `${…}`). */
function literals(code) {
  const out = [];
  for (let i = 0; i < code.length; i += 1) {
    const c = code[i];
    if (c === '/' && code[i + 1] === '/') {
      i = code.indexOf('\n', i);
      if (i < 0) break;
      continue;
    }
    if (c === '/' && code[i + 1] === '*') {
      i = code.indexOf('*/', i) + 1;
      continue;
    }
    if (c !== "'" && c !== '"' && c !== '`') continue;
    let j = i + 1;
    let text = '';
    while (j < code.length && code[j] !== c) {
      if (code[j] === '\\') {
        text += code[j + 1];
        j += 2;
      } else if (c === '`' && code[j] === '$' && code[j + 1] === '{') {
        let depth = 1;
        j += 2;
        while (depth && j < code.length) {
          depth += code[j] === '{' ? 1 : code[j] === '}' ? -1 : 0;
          j += 1;
        }
        text += '\0';
      } else {
        text += code[j];
        j += 1;
      }
    }
    out.push({ start: i, end: j + 1, text });
    i = j;
  }
  return out;
}

/** The top-level items of the list that opens at `open`, as [start, end) spans. */
function items(code, open, byStart) {
  const spans = [];
  let depth = 0;
  let from = open + 1;
  for (let i = open + 1; i < code.length; i += 1) {
    const literal = byStart.get(i);
    if (literal) {
      i = literal.end - 1;
    } else if ('([{'.includes(code[i])) {
      depth += 1;
    } else if (')]}'.includes(code[i])) {
      if (depth === 0) {
        if (code.slice(from, i).trim()) spans.push([from, i]);
        return spans;
      }
      depth -= 1;
    } else if (code[i] === ',' && depth === 0) {
      spans.push([from, i]);
      from = i + 1;
    }
  }
  return spans;
}

const hasWords = (text) => /\p{L}/u.test(text.replace(/\0/g, ''));
// Where a literal would be shown: text, title, labels, placeholders, a
// field's readout, a toast, and the children of `el(…)`.
const SINK = /(?:\b(?:text|title|label|placeholder|alt|alphaLabel)|'aria-label')\s*:\s*$|\bformat:\s*\([^)]*\)\s*=>\s*$|\.(?:textContent|title|placeholder)\s*=\s*$|setAttribute\(\s*'(?:aria-label|title|placeholder)'\s*,\s*$|\b(?:toast|notify)\(\s*$/;

/** The literal texts with words that `code` would show. */
function visibleLiterals(code) {
  const all = literals(code);
  const byStart = new Map(all.map((l) => [l.start, l]));
  const shown = all.filter((l) => hasWords(l.text) && SINK.test(code.slice(Math.max(0, l.start - 60), l.start)));
  for (const call of code.matchAll(/\bel\(/g)) {
    const children = items(code, call.index + 2, byStart)[2];
    if (!children) continue;
    const open = children[0] + code.slice(children[0], children[1]).search(/\S/);
    if (code[open] !== '[') continue;
    for (const [from, to] of items(code, open, byStart)) {
      const start = from + code.slice(from, to).search(/\S/);
      const literal = byStart.get(start);
      if (literal && literal.end === start + code.slice(start, to).trimEnd().length && hasWords(literal.text)) shown.push(literal);
    }
  }
  return shown.map((l) => l.text.replace(/\0/g, '${…}'));
}

test('the literal scan finds text shown without a translation', () => {
  const code = "el('p', { text: 'Hello' }, ['World', t('x'), `${a} s`, `${a}%`, icon(I.x)]); x.textContent = 'Hi'; toast('Oops');"
    + " y.setAttribute('aria-label', 'Lbl'); f(l, v, { format: (v) => `${v} min` }); el('i', { class: 'text-button', 'data-x': 'ok' }, []);";
  assert.deepEqual(visibleLiterals(code).sort(), ['${…} min', '${…} s', 'Hello', 'Hi', 'Lbl', 'Oops', 'World']);
});

test('no text is written in the UI code: everything goes through t()', () => {
  const files = ['app.js', ...readdirSync(new URL('ui/', src)).filter((f) => f.endsWith('.js')).map((f) => `ui/${f}`)];
  const found = files.flatMap((f) => visibleLiterals(readFileSync(new URL(f, src), 'utf8')).map((text) => `${f}: ${text}`));
  assert.deepEqual(found, []);
});

test('index.html has no text of its own but the brand', () => {
  const html = readFileSync(new URL('index.html', src), 'utf8');
  const texts = [...html.matchAll(/>([^<>]+)</g)].map((m) => m[1].trim()).filter((text) => /\p{L}/u.test(text));
  assert.deepEqual([...new Set(texts)], ['Bezel']);
  const attrs = [...html.matchAll(/\s(?:title|alt|placeholder|aria-label)="([^"]*)"/g)].map((m) => m[1]).filter((v) => /\p{L}/u.test(v));
  assert.deepEqual(attrs, []);
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

test('the language choices name the system language and each language by its own name', () => {
  assert.deepEqual(LANGUAGES, Object.keys(LOCALES));
  assert.deepEqual(languageOptions(translator('pt-BR'), 'en'), [
    ['', 'Igual ao do sistema (English)'],
    ['pt-BR', 'Português (Brasil)'],
    ['en', 'English'],
  ]);
  assert.equal(languageOptions(translator('en'), 'pt-BR')[0][1], 'Same as the system (Português (Brasil))');
});

test('the demo keeps the chosen language and follows the browser otherwise', async () => {
  const demo = createDemoBackend('turing88', {}, { languages: ['pt-BR', 'en'] });
  const { language, systemLanguage } = await demo.preferences();
  assert.deepEqual({ language, systemLanguage }, { language: null, systemLanguage: 'pt-BR' });
  await demo.setLanguage('en');
  assert.equal((await demo.preferences()).language, 'en');
  await assert.rejects(demo.setLanguage('de'), (e) => e.code === 'unknownLanguage' && e.args.language === 'de');
  await demo.setLanguage(null);
  assert.equal((await demo.preferences()).language, null);
  assert.equal((await createDemoBackend('turing88').preferences()).systemLanguage, 'en');
});

test('the demo checks and keeps the sensor options like the backend', async () => {
  const demo = createDemoBackend('turing88');
  const before = await demo.preferences();
  assert.deepEqual([before.pingHost, before.defaultPingHost, before.mangohudDir, before.mangohud], ['8.8.8.8', '8.8.8.8', null, true]);
  await demo.setSensorOptions(' 1.1.1.1 ', await demo.pickFolder());
  const after = await demo.preferences();
  assert.deepEqual([after.pingHost, after.mangohudDir], ['1.1.1.1', DEMO_FOLDER]);
  await assert.rejects(demo.setSensorOptions('-c 5', null), (e) => e.code === 'invalidHost' && e.args.host === '-c 5');
  await assert.rejects(demo.setSensorOptions('1.1.1.1', 'relative'), (e) => e.code === 'invalidFolder');
  await demo.setSensorOptions('', null);
  assert.equal((await demo.preferences()).pingHost, '8.8.8.8');
});

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

/**
 * The UI's code, as paths under src/: every module but the translations
 * (i18n/) and demo mode (demo-*.js: the stand-in backend and its data, whose
 * messages are a backend's English `message`, like the real one's).
 */
function uiFiles(dir = src, prefix = '') {
  return readdirSync(dir, { withFileTypes: true }).flatMap((d) => {
    if (d.isDirectory()) return d.name === 'i18n' ? [] : uiFiles(new URL(`${d.name}/`, dir), `${prefix}${d.name}/`);
    return d.name.endsWith('.js') && !d.name.startsWith('demo-') ? [`${prefix}${d.name}`] : [];
  });
}

test('the prose scan reads every module but the translations and demo mode', () => {
  const files = uiFiles();
  for (const f of ['app.js', 'gif-search.js', 'collection.js', 'messages.js', 'ui/gif-search.js', 'ui/collection.js', 'editor/store.js']) assert.ok(files.includes(f), f);
  assert.deepEqual(files.filter((f) => f.startsWith('i18n/') || /(?:^|\/)demo-/.test(f)), []);
});

// A literal that reads like a sentence is text for a person, whatever
// receives it: `el(…)`, a toast, or a local helper like `keyProblem('…')`.
/** A capitalised word with another after it: "Paste the key", "Added ${name}". */
const CAPITALISED_PHRASE = /\p{Lu}\p{Ll}*\s+[\p{L}\0]/u;
/** A word that ends a sentence: "Done.", "first!", "Really?", "Saving…". */
const SENTENCE_END = /\p{L}[.!?…]$/u;
/** The start of a translation key built in code (`'error.' + code`): it ends in "." but is no sentence. */
const KEY_PREFIX = /^[a-z][\w.]*\.$/;
/** A lowercase word as it stands in a sentence, maybe before a comma, ";" or ":". */
const LOWERCASE_WORD = /^\p{Ll}+[,;:]?$/u;

/** Whether a literal's `text` (`\0` for each `${…}`) reads like prose: one of the shapes above, or three lowercase words in a row. */
function readsLikeProse(text) {
  const trimmed = text.trim();
  if (CAPITALISED_PHRASE.test(trimmed)) return true;
  if (SENTENCE_END.test(trimmed) && !KEY_PREFIX.test(trimmed)) return true;
  const plain = trimmed.split(/\s+/).map((word) => LOWERCASE_WORD.test(word));
  return plain.some((word, i) => word && plain[i + 1] && plain[i + 2]);
}

/** The texts of the string and template literals of `code` that read like prose. */
function proseLiterals(code) {
  return literals(code).filter((l) => hasWords(l.text) && readsLikeProse(l.text)).map((l) => l.text.replace(/\0/g, '${…}'));
}

/**
 * The literals of the UI code that read like prose and are not for a
 * translation, as "<file>: <text>". Each one is on the tree: an exception
 * that no longer is fails the test too, so the list cannot go stale.
 */
const NOT_PROSE = new Set([
  // Font families the font picker lists: their own names in every language.
  'app.js: JetBrains Mono',
  'app.js: Roboto Mono',
  // An Error's message for a malformed frame from the renderer: a detail for
  // the log, which the user reads inside the translated `error.unknown`.
  'bridge.js: frame too short',
]);

test('the prose scan finds sentences whatever function receives them', () => {
  const code = [
    "keyProblem('Paste the key in the field first.'); say(`Added ${name}`); note('paste the key first');",
    "note('Done.'); note(\"Really?\"); note(`Saving…`);",
    "t('gifs.keyEmpty'); t('error.' + code); t(`gifs.kind.${kind}`); x.textContent = '?'; f(`${a} s`, `${a}%`);",
    "el('p', { class: 'icon-button key-help-button', 'aria-describedby': 'gif-key-status gif-key-error' });",
    "matchMedia('(prefers-reduced-motion: reduce)'); url.startsWith('data:image/'); key === 'ArrowDown'; g('image/gif', '0 0 4px');",
    '// Paste the key first. /* Not this either. */',
    "/* Nor this. */ throw new Error(`unknown widget ${widget}`);",
  ].join('\n');
  assert.deepEqual(proseLiterals(code), ['Paste the key in the field first.', 'Added ${…}', 'paste the key first', 'Done.', 'Really?', 'Saving…']);
});

test('no sentence is written in the UI code, whatever function receives it', () => {
  const found = uiFiles().flatMap((f) => proseLiterals(readFileSync(new URL(f, src), 'utf8')).map((text) => `${f}: ${text}`));
  assert.deepEqual(found.filter((entry) => !NOT_PROSE.has(entry)), []);
  assert.deepEqual([...NOT_PROSE].filter((entry) => !found.includes(entry)), [], 'an exception no longer on the tree');
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

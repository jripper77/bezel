import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { LOCALES, applyTranslations, guideLocale, pickLocale, placeholders, translator } from '../../src/i18n/index.js';
import { WIDGETS } from '../../src/editor/widgets.js';
import { LANGUAGES, languageOptions } from '../../src/ui/preferences.js';
import { DEMO_FOLDER, DEMO_GUIDE_PAGES, createDemoBackend } from '../../src/demo-backend.js';
import { GIF_KINDS, HELP_STEPS, TILE_TEXT, keyFailure, resultsMessage } from '../../src/gif-search.js';
import { COLLECTION_FILTERS } from '../../src/collection.js';
import { errorMessage } from '../../src/messages.js';

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

test('the studio speaks Portuguese (Brazil), English and Italian', () => {
  assert.deepEqual(Object.keys(LOCALES).sort(), ['en', 'it', 'pt-BR']);
});

/** For each locale, the keys of `en` it lacks and the keys it has that `en` does not. */
function keyDrift(tables) {
  const base = Object.keys(tables.en);
  return Object.fromEntries(Object.entries(tables).map(([locale, table]) => [locale, {
    missing: base.filter((key) => !Object.hasOwn(table, key)),
    extra: Object.keys(table).filter((key) => !Object.hasOwn(tables.en, key)),
  }]));
}

/** The keys whose text has other `{name}` placeholders than `en`'s, as "<locale>: <key>". */
function placeholderDrift(tables) {
  return Object.entries(tables).flatMap(([locale, table]) => Object.keys(tables.en)
    .filter((key) => Object.hasOwn(table, key) && placeholders(table[key]).join() !== placeholders(tables.en[key]).join())
    .map((key) => `${locale}: ${key}`));
}

test('the parity check names the keys a locale lacks or adds and the placeholders it changes', () => {
  const tables = { en: { a: 'A {x}', b: 'B' }, it: { a: 'A {y}', c: 'C' } };
  assert.deepEqual(keyDrift(tables), { en: { missing: [], extra: [] }, it: { missing: ['b'], extra: ['c'] } });
  assert.deepEqual(placeholderDrift(tables), ['it: a']);
});

test('en, pt-BR and it have the same keys, each with the same placeholders', () => {
  const clean = { missing: [], extra: [] };
  assert.deepEqual(keyDrift(LOCALES), { en: clean, 'pt-BR': clean, it: clean });
  assert.deepEqual(placeholderDrift(LOCALES), []);
});

test('every Italian text is written: none is empty', () => {
  assert.deepEqual(Object.keys(LOCALES.it).filter((key) => !String(LOCALES.it[key]).trim()), []);
});

test('the card keeps its name: Card in English and Italian', () => {
  for (const key of ['widget.card', 'card.container']) {
    assert.equal(LOCALES.en[key], 'Card', key);
    assert.equal(LOCALES.it[key], 'Card', key);
  }
  assert.equal(LOCALES['pt-BR']['widget.card'], 'Cartão');
});

test('locale choice follows the browser languages', () => {
  assert.equal(pickLocale(['pt-BR', 'en']), 'pt-BR');
  assert.equal(pickLocale(['pt-PT']), 'pt-BR');
  assert.equal(pickLocale(['de', 'en-US']), 'en');
  assert.equal(pickLocale(['de']), 'en');
  assert.equal(pickLocale([]), 'en');
  assert.equal(pickLocale(['it-IT']), 'it');
  assert.equal(pickLocale(['it']), 'it');
  assert.equal(pickLocale(['de', 'it-CH', 'en']), 'it');
});

test('the guide opens in Portuguese for pt-BR and in English otherwise', () => {
  assert.equal(guideLocale('pt-BR'), 'pt-BR');
  assert.equal(guideLocale('en'), 'en');
  assert.equal(guideLocale('it'), 'en');
});

test('the Italian translator substitutes params', () => {
  const t = translator('it');
  assert.equal(t('inspector.multi', { count: 2 }), '2 elementi selezionati');
  assert.equal(t('top.save'), 'Salva');
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
  // Default theme content, editable by the user rather than UI chrome.
  'editor/widgets.js: No media session',
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

// ------------------------------------------------------------------ keys --
// The helpers of the GIF search and the collection that put text on screen
// or in a live region (`announce`, `keyProblem`, `refuseName`) take a
// translation key and params, never a text, and translate it themselves, as
// `t` does. So a text given to one, even a single word, is an unknown key,
// and the scan below fails on it; one key in place of another is the e2e's.

/** Whether `key` is a key of both dictionaries. */
const isKey = (key) => typeof key === 'string' && Object.values(LOCALES).every((table) => Object.hasOwn(table, key));

/** The modules of the GIF search and the collection: every first argument of a helper there is a key. */
const KEYED_FILES = ['gif-search.js', 'collection.js', 'ui/gif-search.js', 'ui/collection.js'];
/** The functions that take a translation key (and params) and show its text. */
const KEY_HELPERS = ['t', 'announce', 'keyProblem', 'refuseName'];
/** The helpers each of the KEYED_FILES defines: their first parameter is `key`. */
const DEFINED_HELPERS = new Map([['ui/gif-search.js', ['announce', 'keyProblem']], ['ui/collection.js', ['announce', 'refuseName']]]);
/**
 * The functions whose answer `{key, params}` may feed a helper as
 * `<name>.key`, when every `<name>` of the module is declared from one of
 * them; the test after the scan proves that each names only keys.
 */
const MESSAGE_FUNCTIONS = Object.freeze({ errorMessage, keyFailure, resultsMessage });
/** The keys `prefix` + each of `names`. */
const keysOf = (prefix, names) => names.map((name) => `${prefix}${name}`);
/**
 * The first arguments of a helper that are neither a quoted key, a choice of
 * quoted keys, a message's `.key` nor the helper's own `key`, as
 * "<file>: <helper>(<argument>)", and every key each can be. Each one is on
 * the tree: an entry that no longer is fails the test too.
 */
const KEY_SOURCES = new Map([
  // `HELP_STEPS.map((step) => …)`.
  ['ui/gif-search.js: t(step)', HELP_STEPS],
  ['ui/gif-search.js: t(TILE_TEXT[state])', Object.values(TILE_TEXT)],
  // `GIF_KINDS.map((kind) => …)`.
  ['ui/gif-search.js: t(`gifs.kind.${kind}`)', keysOf('gifs.kind.', GIF_KINDS)],
  // The filter chosen, when it leaves every item out: one of COLLECTION_FILTERS but `all`.
  ['ui/collection.js: t(`collection.noneOf.${filter}`)', keysOf('collection.noneOf.', COLLECTION_FILTERS.filter((f) => f !== 'all'))],
  // `COLLECTION_FILTERS.map((f) => …)`.
  ['ui/collection.js: t(`collection.filter.${f}`)', keysOf('collection.filter.', COLLECTION_FILTERS)],
  // `kindOf` answers `gif` or `sticker`, the kinds of GIF_KINDS.
  ['collection.js: t(`collection.kind.${kindOf(item)}`)', keysOf('collection.kind.', GIF_KINDS)],
]);

/**
 * `code` with its comments and the text of its string and template literals
 * blanked (same offsets; the code inside a template's `${…}` stays), and
 * its literals by where they start, with their text (`\0` for a `${…}`).
 */
function lex(code) {
  const masked = [...code];
  const strings = new Map();
  const blank = (from, to) => {
    for (let k = from; k < to; k += 1) if (masked[k] !== '\n') masked[k] = ' ';
  };
  // Where the comment at `i` ends, once blanked; `i` itself when none starts there.
  function comment(i) {
    const pair = code.slice(i, i + 2);
    if (pair !== '//' && pair !== '/*') return i;
    const close = pair === '//' ? code.indexOf('\n', i) : code.indexOf('*/', i + 2) + 2;
    const end = close < 2 ? code.length : close;
    blank(i, end);
    return end;
  }
  // Lexes code from `from`; inside a `${…}`, up to its closing brace, whose offset it answers.
  function scan(from, inside) {
    let depth = 0;
    let i = from;
    while (i < code.length) {
      const c = code[i];
      const next = comment(i);
      if (next !== i) i = next;
      else if (c === "'" || c === '"' || c === '`') i = string(i);
      else if (inside && c === '}' && depth === 0) return i;
      else {
        if (inside) depth += { '{': 1, '}': -1 }[c] ?? 0;
        i += 1;
      }
    }
    return i;
  }
  function string(start) {
    const quote = code[start];
    let j = start + 1;
    let text = '';
    while (j < code.length && code[j] !== quote) {
      if (code[j] === '\\') {
        text += code[j + 1];
        blank(j, j + 2);
        j += 2;
      } else if (quote === '`' && code[j] === '$' && code[j + 1] === '{') {
        blank(j, j + 1);
        j = scan(j + 2, true) + 1;
        text += '\0';
      } else {
        text += code[j];
        blank(j, j + 1);
        j += 1;
      }
    }
    strings.set(start, { end: j + 1, text });
    return j + 1;
  }
  scan(0, false);
  return { masked: masked.join(''), strings };
}

/** The offset of the bracket that closes the one at `open` in masked code. */
function closing(masked, open) {
  let depth = 0;
  for (let i = open; i < masked.length; i += 1) {
    depth += { '(': 1, '[': 1, '{': 1, ')': -1, ']': -1, '}': -1 }[masked[i]] ?? 0;
    if (depth === 0) return i;
  }
  return masked.length;
}

/** `[from, to)` without the spaces around it. */
function trimmed(masked, from, to) {
  const lead = masked.slice(from, to).search(/\S/);
  if (lead < 0) return [from, from];
  return [from + lead, from + masked.slice(from, to).trimEnd().length];
}

/** The offsets of `[from, to)` outside literals and brackets, with their character. */
function topLevel({ masked, strings }, from, to) {
  const out = [];
  let depth = 0;
  let i = from;
  while (i < to) {
    const c = masked[i];
    depth += { '(': 1, '[': 1, '{': 1, ')': -1, ']': -1, '}': -1 }[c] ?? 0;
    if (depth === 0 && !strings.has(i)) out.push([i, c]);
    i = strings.get(i)?.end ?? i + 1;
  }
  return out;
}

/** The two branches of `cond ? a : b` when `[from, to)` is one at its top level, else `null`. */
function branches(lexed, from, to) {
  const { masked } = lexed;
  const isTernary = (i, c) => c === '?' && !'.?'.includes(masked[i + 1]) && masked[i - 1] !== '?';
  let question = -1;
  let pending = 0;
  for (const [i, c] of topLevel(lexed, from, to)) {
    if (isTernary(i, c)) {
      if (question < 0) question = i;
      pending += 1;
    } else if (c === ':' && question >= 0) {
      pending -= 1;
      if (pending === 0) return [[question + 1, i], [i + 1, to]];
    }
  }
  return null;
}

/** The helpers `file` defines: `{name, param, body: [from, to)}`. */
function helperDefinitions(masked) {
  return [...masked.matchAll(/\bfunction\s+([\w$]+)\s*\(\s*([\w$]*)/g)]
    .filter((m) => KEY_HELPERS.includes(m[1]))
    .map((m) => {
      const params = masked.indexOf('(', m.index);
      const open = masked.indexOf('{', closing(masked, params));
      return { name: m[1], param: m[2], body: [open, closing(masked, open)] };
    });
}

/** Whether a key starts as the template `text` (`\0` for each `${…}`) does before its first `${…}`. */
const startsAKey = (text) => Object.keys(LOCALES.en).some((key) => key.startsWith(text.slice(0, text.indexOf('\0'))));

/**
 * Whether `text` is `<name>.key` of a message: every `<name>` of the module
 * is declared once with the answer of a MESSAGE_FUNCTIONS, and never set again.
 */
function isMessageKey(masked, text) {
  const name = /^([\w$]+)\.key$/.exec(text)?.[1];
  if (!name) return false;
  const made = [...masked.matchAll(new RegExp(String.raw`\b(?:const|let|var)\s+${name}\s*=\s*([\w$]*)`, 'g'))].map((m) => m[1]);
  const set = [...masked.matchAll(new RegExp(String.raw`(?<![\w$.])${name}\s*=(?![=>])`, 'g'))].length;
  return made.length > 0 && made.length === set && made.every((fn) => Object.hasOwn(MESSAGE_FUNCTIONS, fn));
}

/** Whether `text` is the first parameter of a helper whose body has the call at `at`. */
const isOwnKey = (helpers, text, at) => helpers.some((h) => h.param === text && h.body[0] < at && at < h.body[1]);

/**
 * Every key the argument at `[from, to)` of a call of `callee` at `at` can
 * be, `null` when no rule takes it: a quoted key; a choice between keys;
 * one of the KEY_SOURCES; a message's `.key`; a helper's own `key`. Outside
 * the KEYED_FILES (not `strict`), anything but a quoted key or a template
 * that no key starts as is taken unchecked.
 */
function argumentKeys(scan, callee, [from, to], at) {
  const { file, code, strict, lexed } = scan;
  const [start, end] = trimmed(lexed.masked, from, to);
  const text = code.slice(start, end);
  const literal = lexed.strings.get(start)?.end === end ? lexed.strings.get(start) : null;
  if (literal && !literal.text.includes('\0')) return [literal.text];
  const choice = branches(lexed, start, end);
  if (choice) {
    const both = choice.map((branch) => argumentKeys(scan, callee, branch, at));
    return both.includes(null) ? null : both.flat();
  }
  if (!strict) return literal && !startsAKey(literal.text) ? null : [];
  const source = `${file}: ${callee}(${text})`;
  if (KEY_SOURCES.has(source)) {
    scan.sources.add(source);
    return KEY_SOURCES.get(source);
  }
  return isMessageKey(lexed.masked, text) || isOwnKey(scan.helpers, text, at) ? [] : null;
}

/**
 * Checks the first argument of each helper call of `code` against the rules
 * of `argumentKeys`: `{problems, sources, helpers}`, the calls that break
 * them ("<file>: <call>: <why>"), the KEY_SOURCES used and the helpers the
 * module defines. `strict`: `code` is one of the KEYED_FILES.
 */
function keyScan(file, code, strict) {
  const lexed = lex(code);
  const scan = { file, code, strict, lexed, helpers: helperDefinitions(lexed.masked), sources: new Set() };
  const problems = [];
  for (const call of lexed.masked.matchAll(new RegExp(String.raw`(?<![\w$.])(${KEY_HELPERS.join('|')})\s*\(`, 'g'))) {
    if (/\bfunction\s+$/.test(lexed.masked.slice(Math.max(0, call.index - 20), call.index))) continue;
    const open = call.index + call[0].length - 1;
    const first = items(lexed.masked, open, new Map())[0];
    const shown = `${file}: ${code.slice(call.index, first ? first[1] : open + 1).replace(/\s+/g, ' ')})`;
    const found = first ? argumentKeys(scan, call[1], first, call.index) : null;
    if (!first && strict) problems.push(`${shown}: no key`);
    else if (first && found === null) problems.push(`${shown}: not a key, nor a source of keys`);
    for (const key of found ?? []) if (!isKey(key)) problems.push(`${shown}: unknown key "${key}"`);
  }
  return { problems, sources: scan.sources, helpers: scan.helpers };
}

test('the key scan takes keys and refuses texts, whatever the helper', () => {
  const code = [
    "announce('gifs.keyRemoved'); keyProblem('gifs.keyEmpty', { a: 1 }); refuseName(on ? 'collection.nameEmpty' : 'gifs.keyEmpty');",
    "announce('Removed'); keyProblem(\"Paste it\"); t(on ? 'gifs.keyRemoved' : 'Nope'); el('p', { text: `${t('Nope2')}` });",
    'function announce(key, params) { live.textContent = t(key, params); }',
    'const why = errorMessage(t, e); announce(why.key, why.params); const bad = { key: \'Removed\' }; announce(bad.key);',
    "let said = resultsMessage(r); said = { key: 'Removed' }; announce(said.key);",
    "keyProblem(text); t(key); t(`gifs.kind.${kind}`); t(TILE_TEXT[state]); announce(); // announce('Nope3')",
  ].join('\n');
  const { problems } = keyScan('x.js', code, true);
  assert.deepEqual(problems.map((p) => p.replace(/^x\.js: /, '')), [
    "announce('Removed'): unknown key \"Removed\"",
    'keyProblem("Paste it"): unknown key "Paste it"',
    "t(on ? 'gifs.keyRemoved' : 'Nope'): unknown key \"Nope\"",
    "t('Nope2'): unknown key \"Nope2\"",
    'announce(bad.key): not a key, nor a source of keys',
    'announce(said.key): not a key, nor a source of keys',
    'keyProblem(text): not a key, nor a source of keys',
    't(key): not a key, nor a source of keys',
    't(`gifs.kind.${kind}`): not a key, nor a source of keys',
    't(TILE_TEXT[state]): not a key, nor a source of keys',
    'announce(): no key',
  ]);
  // Outside the GIF and collection modules only quoted keys (and a template's start) are held to the dictionaries.
  assert.deepEqual(keyScan('y.js', "t(key); t(`gifs.kind.${k}`); t(`gifz.${k}`); t('Nope');", false).problems, [
    'y.js: t(`gifz.${k}`): not a key, nor a source of keys',
    'y.js: t(\'Nope\'): unknown key "Nope"',
  ]);
});

test('the GIF and collection helpers take a key, and every key they are given is in both dictionaries', () => {
  const problems = [];
  const sources = new Set();
  for (const file of uiFiles()) {
    const scan = keyScan(file, readFileSync(new URL(file, src), 'utf8'), KEYED_FILES.includes(file));
    problems.push(...scan.problems);
    for (const source of scan.sources) sources.add(source);
    if (!KEYED_FILES.includes(file)) continue;
    const defined = scan.helpers.map((h) => `${h.name}(${h.param})`).sort();
    assert.deepEqual(defined, (DEFINED_HELPERS.get(file) ?? []).map((name) => `${name}(key)`).sort(), `${file}: its helpers take a key`);
  }
  assert.deepEqual(problems, []);
  assert.deepEqual([...KEY_SOURCES.keys()].filter((source) => !sources.has(source)), [], 'a source of keys no longer on the tree');
  for (const [source, keys] of KEY_SOURCES) for (const key of keys) assert.ok(isKey(key), `${source}: ${key}`);
});

test('the messages a GIF or collection helper is given name only keys of both dictionaries', () => {
  const t = translator('pt-BR');
  const codes = Object.keys(LOCALES.en).filter((key) => key.startsWith('error.')).map((key) => key.slice('error.'.length));
  const errors = [...codes.map((code) => ({ code, args: {} })), { code: 'somethingNew', message: 'x' }, { code: 7 }, new Error('boom'), 'plain', null, undefined];
  const results = [0, 1, 2].flatMap((count) => ['', 'cat'].flatMap((text) => [false, true].map((more) => resultsMessage({ count, text, more }))));
  const said = {
    errorMessage: errors.map((e) => errorMessage(t, e)),
    keyFailure: [...errors, { code: 'invalidInput' }].map((e) => keyFailure(t, e)),
    resultsMessage: results,
  };
  assert.deepEqual(Object.keys(said), Object.keys(MESSAGE_FUNCTIONS), 'each message function is tried');
  for (const [fn, messages] of Object.entries(said)) {
    for (const message of messages) {
      assert.deepEqual(Object.keys(message).sort(), ['key', 'params'], fn);
      assert.ok(isKey(message.key), `${fn}: ${message.key}`);
    }
  }
  assert.deepEqual([...new Set(results.map((m) => m.key))].sort(), [
    'gifs.announce', 'gifs.announceMore', 'gifs.announceNoMore', 'gifs.announceOne', 'gifs.announceTrending', 'gifs.noResults', 'gifs.noTrending',
  ], 'every answer of resultsMessage');
  assert.equal(keyFailure(t, { code: 'invalidInput' }).key, 'gifs.keyInvalid');
  assert.equal(errorMessage(t, { code: 'somethingNew' }).key, 'error.unknown');
});

test('index.html has no text of its own but the brand', () => {
  const html = readFileSync(new URL('index.html', src), 'utf8');
  const texts = [...html.matchAll(/>([^<>]+)</g)].map((m) => m[1].trim()).filter((text) => /\p{L}/u.test(text));
  assert.deepEqual([...new Set(texts)], ['Bezel Evo']);
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
    ['it', 'Italiano'],
  ]);
  assert.equal(languageOptions(translator('it'), 'it')[0][1], 'Come il sistema (Italiano)');
  assert.equal(languageOptions(translator('en'), 'pt-BR')[0][1], 'Same as the system (Português (Brasil))');
});

test('the demo keeps the chosen language and follows the browser otherwise', async () => {
  const demo = createDemoBackend('turing88', {}, { languages: ['pt-BR', 'en'] });
  const { language, systemLanguage } = await demo.preferences();
  assert.deepEqual({ language, systemLanguage }, { language: null, systemLanguage: 'pt-BR' });
  await demo.setLanguage('en');
  assert.equal((await demo.preferences()).language, 'en');
  await demo.setLanguage('it');
  assert.equal((await demo.preferences()).language, 'it');
  assert.equal((await createDemoBackend('turing88', {}, { languages: ['it-IT'] }).preferences()).systemLanguage, 'it');
  await assert.rejects(demo.setLanguage('de'), (e) => e.code === 'unknownLanguage' && e.args.language === 'de');
  await demo.setLanguage(null);
  assert.equal((await demo.preferences()).language, null);
  assert.equal((await createDemoBackend('turing88').preferences()).systemLanguage, 'en');
});

test('the demo opens the guide in English or Portuguese only, like the backend', async () => {
  const opened = [];
  const demo = createDemoBackend('turing88', {}, { onGuide: (page, language) => opened.push([page, language]) });
  for (const language of ['en', 'pt-BR', guideLocale('it')]) await demo.openGuide(DEMO_GUIDE_PAGES[0], language);
  assert.deepEqual(opened.map(([, language]) => language), ['en', 'pt-BR', 'en']);
  await assert.rejects(demo.openGuide(DEMO_GUIDE_PAGES[0], 'it'), (e) => e.code === 'invalidInput');
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

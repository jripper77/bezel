// What the window's code may ask, and from where (D-2026-10-01-gif-sticker-
// search-18): only `src/bridge.js` talks to the backend (`invoke`,
// `__TAURI__`, `__TAURI_INTERNALS__`, Tauri's `ipc`), and only the GIF search
// and collection UI asks KLIPY or opens a link in the browser (the bridge's
// `searchGifs`, `gifPreview`, `collectGif`, `openLink`); the user's guides
// open only from the buttons that open them today (`openGuide`). This reads
// every module under src/ and finds each use of those names: as a name
// (`bridge.openLink`, `{ openLink }`, `openLink:`) and as a string or
// template text that is the name or its command (`bridge['openLink']`,
// `'open_link'`), escapes decoded. A mention in a comment is not a use. The
// demo's KLIPY and the translations are not the bridge: they are out of the
// KLIPY and guide rules, never out of the backend one.
//
// Nor can a page run code those rules do not read (D-2026-10-01-gif-sticker-
// search-19): Tauri hashes an inline script into the CSP at build, so it
// runs. So every page under src/ (`index.html`) loads only the app's own
// modules (`<script type="module" src>` of a module under src/), and has no
// inline script, no inline event handler (`on*=`) and no `javascript:` URL.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';

const APP = new URL('../../', import.meta.url);

/** Who may use which names, each name also counted by its backend command. */
const RULES = Object.freeze([
  {
    what: 'talks to the backend',
    code: /(?<![\w$])(?:invoke|ipc)(?![\w$])|__TAURI/g,
    literal: (text) => /__TAURI|ipc:\/\/|ipc\.localhost/.test(text) || ['invoke', 'ipc'].includes(text),
    allowed: ['src/bridge.js'],
    covers: () => true,
  },
  {
    what: 'asks KLIPY or opens a link',
    names: { searchGifs: 'search_gifs', gifPreview: 'gif_preview', collectGif: 'collect_gif', openLink: 'open_link' },
    // The bridge defines them; the GIF search dialog and the collection panel use them.
    allowed: ['src/bridge.js', 'src/ui/gif-search.js', 'src/ui/collection.js'],
    covers: notTheBridge,
  },
  {
    what: "opens a user's guide",
    names: { openGuide: 'open_guide' },
    // The bridge defines it; app.js gives the inspector its FFmpeg guide,
    // whose framing guide button calls it; the GIF search opens its own guide.
    allowed: ['src/bridge.js', 'src/app.js', 'src/ui/inspector.js', 'src/ui/gif-search.js'],
    covers: notTheBridge,
  },
].map((rule) => {
  if (!rule.names) return rule;
  const names = Object.keys(rule.names);
  const commands = Object.values(rule.names);
  return { ...rule, code: new RegExp(`(?<![\\w$])(?:${names.join('|')})(?![\\w$])`, 'g'), literal: (text) => names.includes(text) || commands.includes(text) };
}));

const [BACKEND, KLIPY, GUIDES] = RULES;

/**
 * Uses of a guarded name that are not the bridge's, each by its exact text:
 * the Library's "Search KLIPY" action only opens the dialog, asking nothing.
 */
const NOT_THE_BRIDGE = Object.freeze([
  { file: 'src/app.js', text: 'searchGifs: () => void gifSearch.open(),' },
  { file: 'src/ui/library.js', text: 'actions.searchGifs()' },
]);

/** The demo's own backend (its KLIPY is a fake) and the translations are out of the KLIPY and guide rules. */
function notTheBridge(file) {
  return !/^src\/demo-[^/]*\.js$/.test(file) && !file.startsWith('src/i18n/');
}

/** Every file under `dir` whose name matches `pattern`, as `src/<path>`. */
function filesUnder(dir, pattern) {
  return readdirSync(new URL(dir, APP), { withFileTypes: true }).flatMap((entry) => {
    if (entry.isDirectory()) return filesUnder(`${dir}${entry.name}/`, pattern);
    return pattern.test(entry.name) ? [`${dir}${entry.name}`] : [];
  });
}

/** Every module under src/, as `src/<path>`. */
const modules = () => filesUnder('src/', /\.m?js$/);

/** Every page under src/, as `src/<path>`. */
const pages = () => filesUnder('src/', /\.x?html?$/i);

/** Words after which a `/` starts a regular expression, not a division. */
const BEFORE_EXPRESSION = new Set(['return', 'typeof', 'instanceof', 'in', 'of', 'new', 'delete', 'void', 'throw', 'case', 'do', 'else', 'yield', 'await']);

/** `raw` (a literal's text as written) with its escapes decoded. */
function unescaped(raw) {
  const simple = { n: '\n', r: '\r', t: '\t', b: '\b', f: '\f', v: '\v', 0: '\0' };
  return raw.replaceAll(/\\(?:u\{([\da-f]+)\}|u([\da-f]{4})|x([\da-f]{2})|(\r\n|[\s\S]))/gi, (_, cp, u, x, other) => {
    const hex = cp ?? u ?? x;
    if (hex) return String.fromCodePoint(Number.parseInt(hex, 16));
    if (other === '\n' || other === '\r\n') return '';
    return simple[other] ?? other;
  });
}

/**
 * Reads `source` as far as telling code from comments and literals: `code`
 * is the source with every comment and every literal's text blanked (line
 * breaks kept, so offsets and lines stay), `literals` each string, template
 * text and regular expression with its decoded text and offset.
 */
function readSource(source) {
  const out = [...source];
  const literals = [];
  const blank = (from, to) => {
    for (let k = from; k < to; k += 1) if (out[k] !== '\n') out[k] = ' ';
  };
  const keep = (from, to, decode = true) => {
    literals.push({ text: decode ? unescaped(source.slice(from, to)) : source.slice(from, to), at: from });
    blank(from, to);
  };
  // For each `${` still open: how many `{` are open inside it.
  const substitutions = [];
  let i = 0;
  let last = '';

  const quoted = (start, quote) => {
    let j = start + 1;
    while (source[j] !== quote) {
      if (j >= source.length || source[j] === '\n') throw new SyntaxError(`unclosed string at ${start}`);
      j += source[j] === '\\' ? 2 : 1;
    }
    keep(start + 1, j);
    return j + 1;
  };
  // A template's text from `start` to its end or to its next `${`.
  const template = (start) => {
    let j = start;
    while (source[j] !== '`' && !(source[j] === '$' && source[j + 1] === '{')) {
      if (j >= source.length) throw new SyntaxError(`unclosed template at ${start}`);
      j += source[j] === '\\' ? 2 : 1;
    }
    keep(start, j);
    if (source[j] === '`') return j + 1;
    substitutions.push(0);
    return j + 2;
  };
  const regex = (start) => {
    let j = start + 1;
    let inClass = false;
    while (inClass || source[j] !== '/') {
      if (j >= source.length || source[j] === '\n') throw new SyntaxError(`unclosed regular expression at ${start}`);
      if (source[j] === '[') inClass = true;
      else if (source[j] === ']') inClass = false;
      j += source[j] === '\\' ? 2 : 1;
    }
    keep(start + 1, j, false);
    j += 1;
    while (/[a-z]/i.test(source[j] ?? '')) j += 1;
    return j;
  };
  const regexMayStart = () => last === '' || BEFORE_EXPRESSION.has(last) || (!/^[\w$]/.test(last) && !')]}'.includes(last) && last !== 'literal');

  while (i < source.length) {
    const c = source[i];
    const next = source[i + 1];
    if (c === '/' && next === '/') {
      const end = source.indexOf('\n', i);
      const to = end === -1 ? source.length : end;
      blank(i, to);
      i = to;
    } else if (c === '/' && next === '*') {
      const end = source.indexOf('*/', i + 2);
      if (end === -1) throw new SyntaxError(`unclosed comment at ${i}`);
      blank(i, end + 2);
      i = end + 2;
    } else if (c === '"' || c === "'") {
      i = quoted(i, c);
      last = 'literal';
    } else if (c === '`') {
      i = template(i + 1);
      last = 'literal';
    } else if (c === '/' && regexMayStart()) {
      i = regex(i);
      last = 'literal';
    } else if (c === '}' && substitutions.at(-1) === 0) {
      substitutions.pop();
      i = template(i + 1);
      last = 'literal';
    } else if (/[\w$]/.test(c)) {
      let j = i;
      while (/[\w$]/.test(source[j] ?? '')) j += 1;
      last = source.slice(i, j);
      i = j;
    } else {
      if (c === '{' && substitutions.length) substitutions[substitutions.length - 1] += 1;
      if (c === '}' && substitutions.length) substitutions[substitutions.length - 1] -= 1;
      if (!/\s/.test(c)) last = c;
      i += 1;
    }
  }
  if (substitutions.length) throw new SyntaxError('unclosed template substitution');
  return { code: out.join(''), literals };
}

/**
 * Each use of a guarded name in `source`, read as the module `file`:
 * `{ rule, file, line, text }`, `text` being the line it is on. `excused`
 * lists the NOT_THE_BRIDGE entries found (and left out).
 */
function usesIn(file, source) {
  const read = readSource(source);
  let { code } = read;
  const excused = [];
  for (const entry of NOT_THE_BRIDGE.filter((e) => e.file === file)) {
    if (!code.includes(entry.text)) continue;
    excused.push(entry);
    code = code.replaceAll(entry.text, ' '.repeat(entry.text.length));
  }
  const lines = source.split('\n');
  const lineAt = (offset) => source.slice(0, offset).split('\n').length;
  const use = (rule, offset) => ({ rule: rule.what, allowed: rule.allowed, file, line: lineAt(offset), text: lines[lineAt(offset) - 1].trim() });
  const uses = RULES.filter((rule) => rule.covers(file)).flatMap((rule) => [
    ...[...code.matchAll(rule.code)].map((m) => use(rule, m.index)),
    ...read.literals.filter((l) => rule.literal(l.text)).map((l) => use(rule, l.at)),
  ]);
  return { uses, excused };
}

/** The uses outside the modules their rule allows, as `<file>:<line> <rule>: <line's text>`, in file and line order. */
const outside = (uses) => uses
  .filter((u) => !u.allowed.includes(u.file))
  .sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line)
  .map((u) => `${u.file}:${u.line} ${u.rule}: ${u.text}`);

/**
 * Reads the HTML `source` as far as its start tags, as a browser does
 * (comments, doctype and end tags skipped; `/` between attributes is a
 * space): each tag's lower-cased `name`, its `attributes` (lower-cased
 * name, value as written, offset) and its offset; a script's `text` up to
 * its end tag (`null` for any other tag). Only a script's content is read
 * as text: inside an `<svg>`, a `<style>` or `<title>` holds markup, so
 * theirs is read as markup everywhere.
 */
function readTags(source) {
  const tags = [];
  const skipTo = (from, end) => {
    const at = source.indexOf(end, from);
    if (at === -1) throw new SyntaxError(`no "${end}" after ${from}`);
    return at + end.length;
  };
  const attributesFrom = (start, attributes) => {
    let j = start;
    for (;;) {
      while (/[\s/]/.test(source[j] ?? '')) j += 1;
      if (j >= source.length) throw new SyntaxError(`unclosed tag at ${start}`);
      if (source[j] === '>') return j + 1;
      const at = j;
      const name = /^[\s\S][^\s/>=]*/.exec(source.slice(j))[0];
      j += name.length;
      while (/\s/.test(source[j] ?? '')) j += 1;
      let value = '';
      if (source[j] === '=') {
        j += 1;
        while (/\s/.test(source[j] ?? '')) j += 1;
        if (source[j] === '"' || source[j] === "'") {
          const end = skipTo(j + 1, source[j]);
          value = source.slice(j + 1, end - 1);
          j = end;
        } else {
          value = /^[^\s>]*/.exec(source.slice(j))[0];
          j += value.length;
        }
      }
      attributes.push({ name: name.toLowerCase(), value, at });
    }
  };
  let i = 0;
  while (i < source.length) {
    const lt = source.indexOf('<', i);
    if (lt === -1) break;
    const name = /^[a-z][^\s/>]*/i.exec(source.slice(lt + 1))?.[0];
    if (source.startsWith('<!--', lt)) i = skipTo(lt + 4, '-->');
    else if (/[!?/]/.test(source[lt + 1] ?? '')) i = skipTo(lt, '>');
    else if (!name) i = lt + 1;
    else {
      const tag = { name: name.toLowerCase(), attributes: [], at: lt, text: null };
      i = attributesFrom(lt + 1 + name.length, tag.attributes);
      if (tag.name === 'script') {
        const end = /<\/script[\s/>]/i.exec(source.slice(i));
        if (!end) throw new SyntaxError(`unclosed <script> at ${lt}`);
        tag.text = source.slice(i, i + end.index);
        i += end.index;
      }
      tags.push(tag);
    }
  }
  return tags;
}

/** An attribute value as the browser reads it: character references decoded (numeric and the few that spell a URL scheme). */
function decodedValue(value) {
  const named = { colon: ':', tab: '\t', newline: '\n', amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" };
  return value.replaceAll(/&(?:#x([\da-f]+)|#(\d+)|([a-z]+));?/gi, (whole, hex, dec, name) => {
    if (hex ?? dec) return String.fromCodePoint(Number.parseInt(hex ?? dec, hex ? 16 : 10));
    return named[name.toLowerCase()] ?? whole;
  });
}

/** Whether an attribute value is a `javascript:` URL (the browser drops tabs and line breaks, and leading spaces). */
const isScriptUrl = (value) => /^javascript:/i.test(decodedValue(value).replaceAll(/[\t\n\r]/g, '').replace(/^[\u0000-\u0020]+/, ''));

/** A fake origin the page `src/<path>` is served from, to resolve its URLs as the app does. */
const ORIGIN = 'https://app.invalid';

/**
 * The module of the app (`src/<path>`, one of `known`) that the URL `src`
 * in the page `page` names, else `null`: a path of this origin, without
 * scheme, host, query or fragment.
 */
function ownModule(page, src, known) {
  const text = decodedValue(src).trim();
  if (/^[a-z][\w+.-]*:/i.test(text) || /^[/\\]{2}/.test(text)) return null;
  let url;
  try {
    url = new URL(text, `${ORIGIN}/${page.replace(/^src\//, '')}`);
  } catch {
    return null;
  }
  if (url.origin !== ORIGIN || url.search || url.hash) return null;
  let path;
  try {
    path = decodeURIComponent(url.pathname);
  } catch {
    return null;
  }
  return known.includes(`src${path}`) ? `src${path}` : null;
}

/**
 * What the page `page` (its `source`) runs that is not one of the app's
 * modules (`known`), each `{ page, line, what, text }` (`text` being the
 * line it is on), in order: an inline script (a `<script>` with content), a
 * script that is not a `type="module"` with the `src` of a module under
 * src/, an inline event handler (an `on*` attribute) and a `javascript:`
 * URL. A tag's first attribute of a name counts, as for the browser.
 */
function pageProblems(page, source, known) {
  const lines = source.split('\n');
  const problems = [];
  const report = (offset, what) => {
    const line = source.slice(0, offset).split('\n').length;
    problems.push({ offset, page, line, what, text: lines[line - 1].trim() });
  };
  for (const tag of readTags(source)) {
    const first = (name) => tag.attributes.find((a) => a.name === name);
    for (const attribute of tag.attributes) {
      if (attribute.name.startsWith('on')) report(attribute.at, 'an inline event handler');
      else if (isScriptUrl(attribute.value)) report(attribute.at, 'a javascript: URL');
    }
    if (tag.name !== 'script') continue;
    if (/\S/.test(tag.text)) report(tag.at, 'an inline script');
    const src = first('src') ?? first('href') ?? first('xlink:href');
    const module = first('type')?.value.trim().toLowerCase() === 'module';
    if (!src || !module || !ownModule(page, src.value, known)) report(tag.at, "a script that is not one of the app's modules");
  }
  return problems.sort((a, b) => a.offset - b.offset).map(({ offset, ...problem }) => problem);
}

test('only the bridge talks to the backend; only the GIF and collection UI asks KLIPY or opens a link', () => {
  const files = modules();
  for (const file of ['src/app.js', 'src/bridge.js', 'src/ui/gif-search.js', 'src/ui/collection.js', 'src/ui/library.js', 'src/demo-backend.js']) {
    assert.ok(files.includes(file), `${file} is read`);
  }
  const found = files.map((file) => ({ file, ...usesIn(file, readFileSync(new URL(file, APP), 'utf8')) }));
  const uses = found.flatMap((f) => f.uses);

  // The reader sees the uses that are allowed: a reader blind to them would pass anything.
  const seen = (file, rule) => uses.filter((u) => u.file === file && u.rule === rule).map((u) => u.text).join('\n');
  assert.ok(seen('src/bridge.js', BACKEND.what).includes('win.__TAURI__?.core?.invoke'));
  assert.ok(seen('src/bridge.js', BACKEND.what).includes("invoke('open_link'"));
  for (const name of Object.keys(KLIPY.names)) {
    assert.ok(seen('src/bridge.js', KLIPY.what).includes(`${name}: `), `the bridge's ${name}`);
    assert.ok(seen('src/ui/gif-search.js', KLIPY.what).includes(`bridge.${name}(`), `the GIF search's ${name}`);
  }
  assert.ok(seen('src/app.js', GUIDES.what).includes("bridge.openGuide('ffmpeg'"));
  assert.ok(seen('src/ui/gif-search.js', GUIDES.what).includes('bridge.openGuide(GUIDE_PAGE'));

  // Each use that is not the bridge's is still there (else the entry is stale).
  for (const entry of NOT_THE_BRIDGE) {
    assert.ok(found.find((f) => f.file === entry.file)?.excused.includes(entry), `${entry.file}: "${entry.text}" is gone; drop it from NOT_THE_BRIDGE`);
  }

  assert.deepEqual(outside(uses), [], 'used outside the modules allowed to');
});

test('the reader: a comment is not a use; a string by its name, a template and bracket access are', () => {
  const at = (file, source) => outside(usesIn(file, source).uses).map((u) => u.replace(/ .*/, ''));
  // Mentions in comments and inside other strings are not uses.
  assert.deepEqual(at('src/app.js', "// bridge.openLink('x')\n/* window.__TAURI__.core.invoke() */\nconst a = t('media.searchGifs'); // invoke\n"), []);
  assert.deepEqual(at('src/app.js', "const url = 'https://example.org/'; // bridge.collectGif\n"), []);
  // The critic's mutants (D-2026-10-01-gif-sticker-search-18).
  const visible = "document.addEventListener('visibilitychange', () => {\n  if (!document.hidden) void bridge.searchGifs({ kind: 'gif', text: 'cat', page: 1, explicit: false });\n});\n";
  assert.deepEqual(at('src/app.js', visible), ['src/app.js:2']);
  assert.deepEqual(at('src/app.js', "if (!document.hidden) globalThis.__TAURI__.core.invoke('open_link', { link: 'klipyPartnerPanel' });\n"), [
    'src/app.js:1', 'src/app.js:1', 'src/app.js:1',
  ]);
  assert.deepEqual(at('src/app.js', "if (!document.hidden) bridge['openLink']('klipyPartnerPanel');\n"), ['src/app.js:1']);
  // Other spellings.
  assert.deepEqual(at('src/ui/storage.js', "const { gifPreview: show } = bridge;\nconst x = globalThis['__TAURI_INTERNALS__'];\n"), ['src/ui/storage.js:1', 'src/ui/storage.js:2']);
  assert.deepEqual(at('src/ui/storage.js', "bridge['open\\x4cink'](link);\nbridge[`collectGif`](id);\nconst s = `${bridge.gifPreview(id)}`;\n"), [
    'src/ui/storage.js:1', 'src/ui/storage.js:2', 'src/ui/storage.js:3',
  ]);
  assert.deepEqual(at('src/ui/canvas.js', "window.ipc.postMessage('x');\nfetch('ipc://localhost/open_link');\n"), ['src/ui/canvas.js:1', 'src/ui/canvas.js:2']);
  // A regular expression with quotes and slashes, or a division, does not hide what follows.
  assert.deepEqual(at('src/app.js', "const re = /'\"\\/\\/[/']/g; bridge.collectGif(id);\n"), ['src/app.js:1']);
  assert.deepEqual(at('src/app.js', "const half = width / 2; bridge.openGuide('x'); const q = (height) / 4;\n"), []);
  assert.deepEqual(at('src/ui/canvas.js', "const half = width / 2; bridge.openGuide('x'); const q = (height) / 4;\n"), ['src/ui/canvas.js:1']);
  // The Library's action opens the dialog: excused in its own module only.
  assert.deepEqual(at('src/app.js', 'const actions = { searchGifs: () => void gifSearch.open(), };\n'), []);
  assert.deepEqual(at('src/ui/storage.js', 'const actions = { searchGifs: () => void gifSearch.open(), };\n'), ['src/ui/storage.js:1']);
  // The demo's KLIPY and the translations are not the bridge, but never talk to the backend.
  assert.deepEqual(at('src/demo-gifs.js', 'const demo = { searchGifs: async () => [] };\n'), []);
  assert.deepEqual(at('src/demo-gifs.js', 'window.__TAURI__.core.invoke("x");\n'), ['src/demo-gifs.js:1', 'src/demo-gifs.js:1']);
  assert.deepEqual(at('src/i18n/en.js', "export default { 'media.searchGifs': 'Search', 'x': 'openLink' };\n"), []);
  assert.throws(() => readSource("const a = 'open"), SyntaxError);
});

test("the pages load only the app's own modules: no inline script, no inline event handler", () => {
  const known = modules();
  const found = pages();
  assert.ok(found.includes('src/index.html'), 'src/index.html is read');
  const read = (page) => readFileSync(new URL(page, APP), 'utf8');

  // The reader sees the page's own module and reads it to its end: a reader
  // blind to scripts, or stopping early, would pass anything.
  const tags = readTags(read('src/index.html'));
  const src = (tag) => tag.attributes.find((a) => a.name === 'src')?.value ?? '';
  assert.ok(tags.some((tag) => tag.name === 'script' && ownModule('src/index.html', src(tag), known) === 'src/app.js'), 'the page loads src/app.js');
  assert.equal(tags.at(-1).attributes.find((a) => a.name === 'id')?.value, 'toast');

  const problems = found.flatMap((page) => pageProblems(page, read(page), known));
  assert.deepEqual(problems.map((p) => `${p.page}:${p.line} ${p.what}: ${p.text}`), [], "runs code that is not one of the app's modules");
});

test('the page reader: inline scripts, handlers and other scripts, however written', () => {
  const known = ['src/app.js', 'src/ui/dom.js'];
  const at = (source, page = 'src/index.html') => pageProblems(page, source, known).map((p) => `${p.page}:${p.line} ${p.what}`);
  // Line 4 is the head's first, 7 the body's.
  const page = (head, body = '') => `<!doctype html>\n<html lang="en">\n<head>\n${head}\n</head>\n<body>\n${body}\n</body>\n</html>\n`;
  const own = '<script type="module" src="app.js"></script>';
  const notOwn = "a script that is not one of the app's modules";

  // The app's own modules, however their path is written; comments and text are not markup.
  assert.deepEqual(at(page(own)), []);
  assert.deepEqual(at(page('<SCRIPT TYPE=Module SRC=./app.js></SCRIPT>\n<script type="module" src="/ui/dom.js">\n</script>\n<script type=" module " src="&#97;pp.js"></script>')), []);
  assert.deepEqual(at(page(own, "<!-- <script>window.__TAURI__.core.invoke('open_link')</script> <img onload=\"x()\"> -->\n<p title=\"onload=x() <script>\" data-on=\"1\">a > b</p>")), []);
  assert.deepEqual(at(page(own, '<a href="guide.html">javascript:</a>')), []);
  assert.deepEqual(at('<script type="module" src="../app.js"></script>', 'src/ui/page.html'), []);

  // The critic's mutants (D-2026-10-01-gif-sticker-search-19): an inline script, an inline handler.
  assert.deepEqual(at(page(`${own}\n<script>window.__TAURI__.core.invoke('open_link', {link:'klipyPartnerPanel'})</script>`)), [
    'src/index.html:5 an inline script', `src/index.html:5 ${notOwn}`,
  ]);
  assert.deepEqual(at(page(own, '<img src="icon.svg" alt="" onload="window.__TAURI__.core.invoke(\'open_link\')">')), ['src/index.html:7 an inline event handler']);
  // Inline code, however written.
  assert.deepEqual(at(page(`<script type="module" src="app.js">bridge.openLink('x')</script>\n<script type="importmap">{}</script>`)), [
    'src/index.html:4 an inline script', 'src/index.html:5 an inline script', `src/index.html:5 ${notOwn}`,
  ]);
  assert.deepEqual(at(page(own, '<img/onerror=alert(1) src=x>\n<div title="a>b" ONCLICK=\'x()\'></div>\n<svg><set attributeName="x" onbegin="x()"/></svg>')), [
    'src/index.html:7 an inline event handler', 'src/index.html:8 an inline event handler', 'src/index.html:9 an inline event handler',
  ]);
  assert.deepEqual(at(page(own, '<a href=" jav&#x09;ascript&colon;void(0)">x</a>\n<iframe src="JavaScript:x()"></iframe>\n<form action=javascript:x()></form>')), [
    'src/index.html:7 a javascript: URL', 'src/index.html:8 a javascript: URL', 'src/index.html:9 a javascript: URL',
  ]);
  // Inside an <svg>, a <style> or a <title> holds markup: read as markup.
  assert.deepEqual(at(page(own, '<svg><style><img src=x onerror=x()></style>\n<title><a href="javascript:x()">t</a></title></svg>')), [
    'src/index.html:7 an inline event handler', 'src/index.html:8 a javascript: URL',
  ]);
  // A script that is not one of the app's modules: elsewhere, not a module, not a module of the app.
  const others = [
    'https://cdn.example/x.js', '//cdn.example/x.js', '\\\\cdn.example\\x.js', '/\\cdn.example/x.js', 'data:text/javascript,x()',
    'blob:https://app.invalid/1', 'app.js?v=2', 'app.js#x', 'demo.js', 'app.mjs', 'ui/', '',
  ];
  for (const src of others) assert.deepEqual(at(page(`<script type="module" src="${src}"></script>`)), [`src/index.html:4 ${notOwn}`], src);
  assert.deepEqual(at(page('<script src="app.js"></script>\n<script type="text/javascript" src="app.js"></script>\n<script type="module"></script>')), [
    `src/index.html:4 ${notOwn}`, `src/index.html:5 ${notOwn}`, `src/index.html:6 ${notOwn}`,
  ]);
  assert.deepEqual(at(page('<svg><script href="data:text/javascript,x()"></script></svg>')), [`src/index.html:4 ${notOwn}`]);
  assert.deepEqual(at('<script type="module" src="app.js"></script>', 'src/ui/page.html'), [`src/ui/page.html:1 ${notOwn}`]);
  // The first attribute of a name is the one the browser takes.
  assert.deepEqual(at(page('<script type="module" src="app.js" src="https://cdn.example/x.js"></script>')), []);
  assert.deepEqual(at(page('<script type="module" src="https://cdn.example/x.js" src="app.js"></script>')), [`src/index.html:4 ${notOwn}`]);
  assert.deepEqual(at(page('<script type="text/javascript" type="module" src="app.js"></script>')), [`src/index.html:4 ${notOwn}`]);
  // A page it cannot read is an error, not a pass.
  for (const broken of ['<script type="module" src="app.js">', '<div title="x>', '<!-- <script>', '<p']) assert.throws(() => readTags(broken), SyntaxError, broken);
});

// Static guards of the window's look (D-2026-10-09-studio-redesign-4/-6):
// the CSP stays exactly as it is, no font is fetched from Google, every colour
// of styles.css is a token (a hex value lives only in the light `:root` block
// or in the dark `:root` inside `@media (prefers-color-scheme: dark)`), and
// the IBM Plex faces it declares are the bundled files, in the weights that
// ship (400, 500, 600).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const APP = fileURLToPath(new URL('../../', import.meta.url));
const SRC = join(APP, 'src');
const CONF = join(APP, 'src-tauri', 'tauri.conf.json');
const STYLES = join(SRC, 'styles.css');

const CSP =
  "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; " +
  "font-src 'self'; media-src 'self' data: blob:; connect-src ipc: http://ipc.localhost; " +
  "object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

const HEX = /#[0-9a-f]{3,8}\b/i;
const WEIGHTS = new Set(['400', '500', '600']);
const TEXT_FILES = /\.(?:css|html|js|mjs|json|svg)$/i;

/** Every file under `dir`, recursively. */
function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

/**
 * The rules of a stylesheet as a tree: `{ prelude, body }` for a rule with
 * declarations, `{ prelude, rules }` for an at-rule that holds rules.
 * Comments are dropped first; strings in this stylesheet hold no braces.
 */
function parseRules(css) {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, '');
  let at = 0;
  function block() {
    const rules = [];
    let start = at;
    while (at < text.length) {
      const char = text[at];
      if (char === '}') {
        at += 1;
        return rules;
      }
      if (char === ';' && text.slice(start, at).trim().startsWith('@')) {
        at += 1;
        start = at;
        continue;
      }
      if (char !== '{') {
        at += 1;
        continue;
      }
      const prelude = text.slice(start, at).trim();
      at += 1;
      rules.push(prelude.startsWith('@media') || prelude.startsWith('@supports') ? { prelude, rules: block() } : { prelude, body: body() });
      start = at;
    }
    return rules;
  }
  function body() {
    const from = at;
    let depth = 1;
    while (at < text.length && depth > 0) {
      if (text[at] === '{') depth += 1;
      if (text[at] === '}') depth -= 1;
      at += 1;
    }
    return text.slice(from, at - 1);
  }
  return block();
}

/** `[property, value]` of each declaration of a rule body. */
function declarations(body) {
  return body
    .split(';')
    .map((part) => part.trim())
    .filter((part) => part.includes(':'))
    .map((part) => {
      const colon = part.indexOf(':');
      return [part.slice(0, colon).trim().toLowerCase(), part.slice(colon + 1).trim()];
    });
}

/** Rules with declarations, each with the at-rule preludes around it. */
function flatten(rules, within = []) {
  return rules.flatMap((rule) => (rule.rules ? flatten(rule.rules, [...within, rule.prelude]) : [{ ...rule, within }]));
}

/** Whether a rule is one of the two token blocks. */
function isTokenBlock(rule) {
  if (rule.prelude !== ':root') return false;
  if (rule.within.length === 0) return true;
  return rule.within.length === 1 && /^@media\s*\(\s*prefers-color-scheme\s*:\s*dark\s*\)$/.test(rule.within[0]);
}

const rules = flatten(parseRules(readFileSync(STYLES, 'utf8')));

test('the CSP of tauri.conf.json is the fixed one', () => {
  const conf = JSON.parse(readFileSync(CONF, 'utf8'));
  assert.equal(conf.app.security.csp, CSP);
});

test('no file under src/ nor tauri.conf.json mentions a Google Fonts host', () => {
  const files = [...walk(SRC).filter((path) => TEXT_FILES.test(path)), CONF];
  const offenders = files.filter((path) => /fonts\.(?:googleapis|gstatic)/i.test(readFileSync(path, 'utf8')));
  assert.deepEqual(offenders.map((path) => relative(APP, path)), []);
});

test('styles.css has both token blocks, and the accent is defined once', () => {
  const tokens = rules.filter(isTokenBlock);
  assert.equal(tokens.length, 2, 'a light :root and a dark :root');
  const accents = tokens.flatMap((rule) => declarations(rule.body)).filter(([property]) => property === '--accent');
  assert.deepEqual(accents, [['--accent', '#FF9248']]);
});

test('no hex colour in a declaration of styles.css outside the token blocks', () => {
  const offenders = rules
    .filter((rule) => !isTokenBlock(rule))
    .flatMap((rule) => declarations(rule.body).filter(([, value]) => HEX.test(value)).map(([property, value]) => `${rule.prelude} { ${property}: ${value} }`));
  assert.deepEqual(offenders, []);
});

test('every @font-face url() is a bundled file and every font-weight is 400, 500 or 600', () => {
  const faces = rules.filter((rule) => rule.prelude === '@font-face');
  assert.ok(faces.length >= 5, 'IBM Plex Sans 400/500/600 and Mono 400/500');
  const urls = faces.flatMap((rule) => [...rule.body.matchAll(/url\(\s*(['"]?)([^'")]+)\1\s*\)/g)].map((match) => match[2]));
  assert.ok(urls.length >= faces.length);
  for (const url of urls) {
    assert.match(url, /^assets\/fonts\/ibm-plex\/[\w-]+\.woff2$/);
    assert.ok(existsSync(join(SRC, url)), `${url} exists`);
  }
  for (const rule of faces) assert.match(rule.body, /font-display\s*:\s*swap/);
  const weights = rules.flatMap((rule) => declarations(rule.body).filter(([property]) => property === 'font-weight').map(([, value]) => [rule.prelude, value]));
  const wrong = weights.filter(([, value]) => !WEIGHTS.has(value) && !['inherit', 'normal'].includes(value));
  assert.deepEqual(wrong, []);
});

test('the parser keeps at-rules apart and sees through comments', () => {
  const parsed = flatten(parseRules(':root { --a: #fff; }\n/* #000 { } */\n@media (prefers-color-scheme: dark) { :root { --a: #000; } }\n.x { color: var(--a); }'));
  assert.deepEqual(parsed.map((rule) => [rule.prelude, rule.within.length, isTokenBlock(rule)]), [[':root', 0, true], [':root', 1, true], ['.x', 0, false]]);
});

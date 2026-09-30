// What every e2e spec shares: the `t` fixture (the studio's own translator
// in the project's language, so each test runs in pt-BR and en alike), the
// console watch, the axe check and pointer drags.
import { test as base, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import { pickLocale, translator } from '../../src/i18n/index.js';

/** The tests, with `t` and `lang` (`pt-BR` or `en`) for the project's locale. */
export const test = base.extend({
  lang: async ({ locale }, use) => use(pickLocale([locale ?? 'en'])),
  t: async ({ lang }, use) => use(translator(lang)),
});

export { expect };

/** Console errors and page errors, collected while a test runs. */
export function watchErrors(page) {
  const errors = [];
  page.on('console', (msg) => msg.type() === 'error' && errors.push(msg.text()));
  page.on('pageerror', (err) => errors.push(err.message));
  return errors;
}

/** No serious or critical accessibility violation on the page now. */
export async function expectAccessible(page) {
  const { violations } = await new AxeBuilder({ page }).analyze();
  const serious = violations.filter((v) => ['critical', 'serious'].includes(v.impact));
  expect(serious.map((v) => `${v.id}: ${v.help} @ ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`)).toEqual([]);
}

/** Drags `source` onto `target` at a fraction of its box, like a person. */
export async function dragTo(page, source, target, at = { x: 0.5, y: 0.5 }) {
  const from = await source.boundingBox();
  const to = await target.boundingBox();
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + from.width / 2 + 20, from.y + from.height / 2 + 20, { steps: 4 });
  await page.mouse.move(to.x + to.width * at.x, to.y + to.height * at.y, { steps: 8 });
  await page.mouse.up();
}

/** `text` as a regular expression that matches it literally. */
export function literally(text) {
  return new RegExp(text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
}

/** A placeholder no translation contains, to cut a text around a value. */
const CUT = '\u0001';

/** The part of `t(key)` before its first parameter (params filled with a cut mark). */
export function prefixOf(t, key, params) {
  return t(key, Object.fromEntries(params.map((p) => [p, CUT]))).split(CUT)[0];
}

/** The part of `t(key)` after its last parameter. */
export function suffixOf(t, key, params) {
  return t(key, Object.fromEntries(params.map((p) => [p, CUT]))).split(CUT).pop();
}

// Messages the backend sends as codes with arguments (D-2026-09-30-release-
// polish-6): every code of `fixtures/backend-codes.json` (kept in step with
// the Rust code by a test there) has a text in each language, both using the
// same {params}, none unknown to the backend.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { LOCALES, placeholders, translator } from '../../src/i18n/index.js';
import { warningText } from '../../src/messages.js';

const codes = JSON.parse(readFileSync(new URL('fixtures/backend-codes.json', import.meta.url), 'utf8'));
const pt = translator('pt-BR');
const en = translator('en');

/** Checks the texts of `prefix.<code>` for every code of `table`. */
function checkTexts(prefix, table) {
  for (const [code, params] of Object.entries(table)) {
    const key = `${prefix}.${code}`;
    for (const [locale, strings] of Object.entries(LOCALES)) {
      assert.ok(key in strings, `${locale}: ${key} is missing`);
      const used = placeholders(strings[key]);
      for (const name of used) assert.ok(params.includes(name), `${locale}: ${key} uses {${name}}, which the backend does not send`);
    }
    assert.deepEqual(placeholders(LOCALES.en[key]), placeholders(LOCALES['pt-BR'][key]), `${key}: en and pt-BR use other {params}`);
  }
  const known = new Set(Object.keys(table).map((code) => `${prefix}.${code}`));
  const stale = Object.keys(LOCALES.en).filter((k) => k.startsWith(`${prefix}.`) && !known.has(k) && !k.startsWith(`${prefix}.layer.`));
  assert.deepEqual(stale, [], `texts for codes the backend no longer sends (${prefix})`);
}

test('every import warning has a text in each language with its params', () => {
  assert.ok(Object.keys(codes.importWarnings).length > 40);
  checkTexts('importWarning', codes.importWarnings);
  for (const layer of codes.importLayers) {
    for (const [locale, strings] of Object.entries(LOCALES)) assert.ok(`importWarning.layer.${layer}` in strings, `${locale}: layer ${layer}`);
  }
});

test('import warnings read in the chosen language', () => {
  const led = { code: 'backplateLed', args: {}, message: 'the backplate LED color (XuanFang rev B) is not part of a Bezel theme' };
  assert.equal(warningText(pt, led), 'A cor do LED traseiro (XuanFang rev B) não faz parte de um tema do Bezel.');
  assert.equal(warningText(en, led), 'The backplate LED color (XuanFang rev B) is not part of a Bezel theme.');
  const needle = { code: 'layerWithoutPicture', args: { layer: 'needle' }, message: 'a needle layer without a picture was dropped' };
  assert.equal(warningText(pt, needle), 'Camada de ponteiro sem imagem ficou de fora.');
  assert.equal(warningText(en, needle), 'A needle layer without a picture was left out.');
  const video = { code: 'videoNotFound', args: { name: 'AMD.mp4', path: 'assets/AMD.mp4' } };
  assert.match(warningText(pt, video), /AMD\.mp4 não está dentro do \.turtheme.*como assets\/AMD\.mp4/);
});

test('an import warning this UI does not know keeps the backend text', () => {
  assert.equal(warningText(pt, { code: 'somethingNew', args: {}, message: 'the new thing' }), 'the new thing');
  assert.equal(warningText(pt, { code: 'somethingNew' }), 'somethingNew');
  assert.equal(warningText(en, 'plain text'), 'plain text');
  const odd = { code: 'layerWithoutData', args: { layer: 'hologram' } };
  assert.equal(warningText(en, odd), 'A hologram without a data source was left out.', 'an unknown layer stays as sent');
});

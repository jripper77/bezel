// Messages the backend sends as codes with arguments (D-2026-09-30-release-
// polish-6): every code of `fixtures/backend-codes.json` (kept in step with
// the Rust code by a test there) has a text in each language, both using the
// same {params}, none unknown to the backend.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { LOCALES, placeholders, translator } from '../../src/i18n/index.js';
import { errorText, sensorLabel, warningText } from '../../src/messages.js';
import { refusalText } from '../../src/ui/storage.js';
import {
  ENTRY_STATES, FINDING_CODES, HALT_CODES, PLAN_REFUSALS, SKIP_CODES, STAGES, TRANSFERS, WARNING_CODES, haltText, planRefusalText,
} from '../../src/storage-manager.js';

const codes = JSON.parse(readFileSync(new URL('fixtures/backend-codes.json', import.meta.url), 'utf8'));
const pt = translator('pt-BR');
const en = translator('en');

/**
 * Checks the texts of `prefix.<code>` for every code of `table`; `own`
 * lists the UI's own keys under `prefix`.
 */
function checkTexts(prefix, table, own = []) {
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
  const stale = Object.keys(LOCALES.en).filter((k) => k.startsWith(`${prefix}.`) && !known.has(k) && !own.some((o) => k.startsWith(`${prefix}.${o}`)));
  assert.deepEqual(stale, [], `texts for codes the backend no longer sends (${prefix})`);
}

test('every import warning has a text in each language with its params', () => {
  assert.ok(Object.keys(codes.importWarnings).length > 40);
  checkTexts('importWarning', codes.importWarnings, ['layer.']);
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

test('every error code has a text in each language with its params', () => {
  assert.ok(Object.keys(codes.errors).length > 25);
  checkTexts('error', codes.errors, ['unknown']);
});

test('errors read in the chosen language, with their arguments', () => {
  const denied = { code: 'accessDenied', args: { address: '/dev/ttyACM1', reason: 'Permission denied (os error 13)' }, message: 'access denied to /dev/ttyACM1: Permission denied (os error 13)' };
  assert.equal(errorText(pt, denied), 'O sistema não deixou o Bezel abrir /dev/ttyACM1 (Permission denied (os error 13)).');
  assert.equal(errorText(en, denied), 'The system did not let Bezel open /dev/ttyACM1 (Permission denied (os error 13)).');
  const misfit = { code: 'themeMisfit', args: { theme: '320x480', screen: '480x1920' } };
  assert.match(errorText(pt, misfit), /^Este tema tem 320x480, mas a tela tem 480x1920/);
  assert.match(errorText(pt, { code: 'busy', args: {} }), /ocupada/);
  assert.match(errorText(pt, { code: 'timeout' }), /não respondeu a tempo/, 'no args at all');
});

test('an error this UI does not know keeps its own text', () => {
  assert.equal(errorText(en, { code: 'somethingNew', args: {}, message: 'the new thing' }), 'It did not work: the new thing');
  assert.equal(errorText(pt, new Error('boom')), 'Não deu certo: boom');
  assert.equal(errorText(en, 'plain'), 'It did not work: plain');
  assert.equal(errorText(en, null), 'It did not work: null');
});

test('every refusal and every way a file differs has its own sentence', () => {
  for (const [locale, t] of [['pt-BR', pt], ['en', en]]) {
    for (const code of codes.refusals) {
      const text = refusalText(t, locale, { code, message: `core text of ${code}` });
      assert.ok(!text.includes('core text') && !text.includes('storage.'), `${locale}: ${code}`);
    }
    for (const code of codes.mismatches) assert.ok(t.has(`storage.mismatch.${code}`), `${locale}: ${code}`);
  }
});

test('well-known sensors are named in the chosen language, others as the machine names them', () => {
  assert.equal(sensorLabel(pt, { key: 'cpu.usage', label: 'CPU usage' }), 'Uso da CPU');
  assert.equal(sensorLabel(en, { key: 'cpu.usage', label: 'CPU usage' }), 'CPU usage');
  assert.equal(sensorLabel(pt, { key: 'hwmon.nvme0.composite', label: 'NVMe composite' }), 'NVMe composite');
});

test('every sensor named by the UI is a key of the core catalog', () => {
  const core = readFileSync(new URL('../../../../crates/bezel-core/src/domain/sensor.rs', import.meta.url), 'utf8');
  const block = core.slice(core.indexOf('pub mod keys'), core.indexOf('pub const IMPORTED'));
  const keys = new Set([...block.matchAll(/pub const [A-Z0-9_]+: &str = "([a-z0-9.]+)";/g)].map((m) => m[1]));
  assert.ok(keys.size > 40);
  const named = Object.keys(LOCALES.en).filter((k) => k.startsWith('sensor.')).map((k) => k.slice('sensor.'.length));
  assert.deepEqual(named.filter((k) => !keys.has(k)), []);
  assert.deepEqual([...keys].filter((k) => !named.includes(k)), [], 'every well-known key has a name');
});

test('every storage manager code has its sentence in each language and the UI knows it', () => {
  const { manager } = codes;
  // A new kind of code in the fixture needs its check below.
  assert.deepEqual(Object.keys(manager).sort(), ['entryStates', 'findings', 'halts', 'planRefusals', 'skips', 'stages', 'transfers', 'warnings']);
  const known = {
    entryStates: ENTRY_STATES, findings: FINDING_CODES, halts: HALT_CODES, planRefusals: PLAN_REFUSALS,
    skips: SKIP_CODES, stages: STAGES, transfers: TRANSFERS, warnings: WARNING_CODES,
  };
  for (const [kind, list] of Object.entries(known)) assert.deepEqual([...list].sort(), manager[kind], `storage-manager.js knows exactly the backend's ${kind}`);

  // Codes the UI reads through keys of their own.
  const keysOf = {
    entryStates: (c) => [`storage.state.${c}`],
    findings: (c) => [`storage.finding.${c}`, `storage.findingGroup.${c}`, `storage.findingHelp.${c}`],
    skips: (c) => [`storage.skip.${c}`],
    stages: (c) => [`storage.report.stage.${c}`],
    transfers: (c) => [`storage.plan.title.${c}`, `storage.plan.action.${c}`, `storage.plan.intro.${c}`, `storage.report.done.${c}`, `storage.job.${c}`],
    warnings: (c) => [`storage.warning.${c}`],
  };
  for (const [kind, keys] of Object.entries(keysOf)) {
    for (const key of manager[kind].flatMap(keys)) {
      for (const [locale, strings] of Object.entries(LOCALES)) assert.ok(key in strings, `${locale}: ${key} is missing`);
      assert.deepEqual(placeholders(LOCALES.en[key]), placeholders(LOCALES['pt-BR'][key]), `${key}: en and pt-BR use other {params}`);
    }
  }

  // Codes the UI reads through a function: a sentence of their own, no key
  // or {param} left, never the backend's English.
  const plain = (text) => !text.includes('core text') && !text.includes('storage.') && !/\{\w+\}/.test(text);
  for (const [locale, t] of [['pt-BR', pt], ['en', en]]) {
    for (const code of manager.planRefusals) {
      const text = planRefusalText(t, locale, { code, args: { path: 'sd/video/a.mp4', needed: 10, free: 4, refusal: { code: 'emptyFile' } }, message: 'core text' });
      assert.ok(plain(text), `${locale}: ${code}: ${text}`);
    }
    const unknown = errorText(t, { code: 'somethingNew', message: 'core text' });
    for (const halt of manager.halts) {
      const why = { halt, error: { code: 'timeout', args: { detail: 'x' }, message: 'core text' }, refusal: { code: 'noCard', message: 'core text' }, conflict: { path: 'sd/video/a.mp4' } };
      const text = haltText(t, locale, why, errorText);
      assert.ok(plain(text) && text !== unknown, `${locale}: ${halt}: ${text}`);
    }
  }
});

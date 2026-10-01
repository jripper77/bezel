// The storage manager's logic (`storage-manager.js`) and the demo's port of
// the core's rules (`demo-manager.js`): lists, selection, actions, the
// sentences of every code, and the cleanup findings and plans checked on the
// user's real 8.8" listing, like the core's tests.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { LOCALES, placeholders, translator } from '../../src/i18n/index.js';
import { errorText } from '../../src/messages.js';
import {
  CACHE_LIMITS, FINDING_CODES, KIND_FILTERS, ORIGIN_FILTERS, PLAN_REFUSALS, SKIP_CODES, SORTS, TRANSFERS, WARNING_CODES, actionsFor,
  cleanupGroups, compareFiles, emptySelection, findingText, formatExactBytes, formatLimit, formatSent, hasCopy, isBezel, keyAction,
  matchesFilter, otherMedium, placeText, planRefusalText, planTotals, precheckedPaths, reduceSelection, renamePreview, reportLines,
  restorableFor, restoreDefaults, restoreTotals, selectionInfo, skipText, stepText, visibleFiles, warningText,
} from '../../src/storage-manager.js';
import {
  DEMO_HANG_PARTIAL, demoArtifactBase, demoExtension, demoFindings, demoForUpload, demoPlanAcross, demoPlanRename, demoPlanRestore, demoRank,
  demoSameFile, demoSuggest, demoUploadName,
} from '../../src/demo-manager.js';
import { VENDOR_STORAGE } from '../../src/demo-data.js';

const pt = translator('pt-BR');
const en = translator('en');

const managed = (path, size, extra = {}) => {
  const [medium, kind, name] = path.split('/');
  return { path, medium, kind, name, size, entry: null, finding: null, protected: null, ...extra };
};
const sent = (sentAt, extra = {}) => ({ state: 'stored', localCopy: true, sentAt, source: null, durationMs: null, resolution: null, ...extra });

test('every code of the core has a sentence in each language, with the same params', () => {
  const keys = [
    ...FINDING_CODES.flatMap((c) => [`storage.finding.${c}`, `storage.findingGroup.${c}`, `storage.findingHelp.${c}`]),
    ...SKIP_CODES.map((c) => `storage.skip.${c}`),
    ...WARNING_CODES.map((c) => `storage.warning.${c}`),
    ...TRANSFERS.flatMap((c) => [`storage.plan.title.${c}`, `storage.plan.action.${c}`, `storage.plan.intro.${c}`, `storage.report.done.${c}`, `storage.job.${c}`]),
    ...['busy', 'none', 'single', 'noCard', 'deleteUnsupported', 'live', 'hasCopy', 'unknownSize'].map((c) => `storage.reason.${c}`),
    ...KIND_FILTERS.map((v) => `storage.filter.kind.${v}`),
    ...ORIGIN_FILTERS.map((v) => `storage.filter.origin.${v}`),
    ...SORTS.map((v) => `storage.filter.sort.${v}`),
    ...['pending', 'stored', 'missing', 'deleted'].map((s) => `storage.state.${s}`),
    'storage.restore.deletedTitle', 'storage.restore.deletedHelp',
  ];
  for (const key of keys) {
    for (const [locale, table] of Object.entries(LOCALES)) assert.ok(key in table, `${locale}: ${key}`);
    assert.deepEqual(placeholders(LOCALES.en[key]), placeholders(LOCALES['pt-BR'][key]), key);
  }
  for (const t of [pt, en]) {
    for (const code of PLAN_REFUSALS) {
      const text = planRefusalText(t, 'en', { code, args: { path: 'sd/video/a.mp4', needed: 10, free: 4, refusal: { code: 'emptyFile' } }, message: 'core text' });
      assert.ok(!text.includes('core text') && !text.includes('storage.') && !/\{\w+\}/.test(text), `${code}: ${text}`);
    }
  }
  assert.equal(planRefusalText(en, 'en', { code: 'somethingNew', message: 'the core says' }), 'the core says');
  assert.equal(planRefusalText(en, 'en', { code: 'somethingNew' }), 'somethingNew');
});

test('sizes, limits and dates read in the UI\'s language', () => {
  assert.equal(formatExactBytes(7_444_889, 'pt-BR'), '7.444.889');
  assert.equal(formatExactBytes(7_444_889, 'en'), '7,444,889');
  assert.deepEqual(CACHE_LIMITS.map((b) => formatLimit(b, 'en')), ['512 MiB', '1 GiB', '2 GiB', '5 GiB', '10 GiB']);
  assert.equal(formatLimit(1.5 * 2 ** 30, 'pt-BR'), '1,5 GiB');
  const noon = Date.parse('2026-09-12T12:00:00Z') / 1000;
  assert.match(formatSent(noon, 'en'), /Sep 12, 2026/);
  assert.match(formatSent(noon, 'pt-BR'), /12 de set\. de 2026/);
  assert.equal(otherMedium('sd'), 'internal');
  assert.equal(otherMedium('internal'), 'sd');
});

test('the lists sort by name, size, date sent and kind, and filter by text, kind and origin', () => {
  const files = [
    managed('sd/video/b10.mp4', 30, { entry: sent(200) }),
    managed('sd/video/B2.mp4', null),
    managed('sd/image/a.png', 50, { finding: { code: 'unused', prechecked: false } }),
    managed('sd/video/c.mp4', 30, { entry: sent(100, { state: 'deleted' }) }),
    managed('internal/video/z.mp4', 1, { entry: sent(300) }),
  ];
  const names = (sort, filter = {}) => visibleFiles(files, 'sd', filter, sort, 'en').map((f) => f.name);
  assert.deepEqual(names('name'), ['a.png', 'B2.mp4', 'b10.mp4', 'c.mp4'], 'letter case aside, numbers as numbers');
  assert.deepEqual(names('size'), ['a.png', 'b10.mp4', 'c.mp4', 'B2.mp4'], 'largest first, unknown last');
  assert.deepEqual(names('sent'), ['b10.mp4', 'a.png', 'B2.mp4', 'c.mp4'], 'newest first; a deleted entry is not Bezel\'s');
  assert.deepEqual(names('kind'), ['a.png', 'B2.mp4', 'b10.mp4', 'c.mp4']);
  assert.deepEqual(names('name', { text: ' B ' }), ['B2.mp4', 'b10.mp4']);
  assert.deepEqual(names('name', { kind: 'image' }), ['a.png']);
  assert.deepEqual(names('name', { kind: 'all', origin: 'bezel' }), ['b10.mp4']);
  assert.deepEqual(names('name', { origin: 'other' }), ['a.png', 'B2.mp4', 'c.mp4']);
  assert.deepEqual(names('name', { origin: 'findings' }), ['a.png']);
  assert.ok(matchesFilter(files[0]));
  assert.equal(compareFiles('name', 'en')({ name: 'x' }, { name: 'X' }) > 0, true, 'a stable order for names equal but for case');
  assert.equal(isBezel(files[3]), false);
  assert.equal(hasCopy(files[0]), true);
  assert.equal(hasCopy(managed('sd/video/p.mp4', 1, { entry: sent(1, { state: 'pending' }) })), false, 'a pending upload is no copy');
  assert.equal(hasCopy(managed('sd/video/p.mp4', 1, { entry: sent(1, { localCopy: false }) })), false);
});

test('a listbox selection follows arrows, Space, Shift+arrows, Ctrl+A, Esc and clicks', () => {
  const ids = ['a', 'b', 'c', 'd'];
  let s = emptySelection();
  const go = (action) => {
    s = reduceSelection(s, action, ids);
    return s;
  };
  assert.equal(go({ type: 'move', delta: 1 }).focus, 'a', 'the first arrow lands on the first');
  assert.deepEqual(go({ type: 'move', delta: 1 }), { selected: [], anchor: null, focus: 'b' }, 'arrows move the focus only');
  assert.deepEqual(go({ type: 'toggle' }).selected, ['b']);
  assert.deepEqual(go({ type: 'move', delta: 1, extend: true }).selected, ['b', 'c'], 'Shift+arrow selects from the anchor');
  assert.deepEqual(go({ type: 'move', to: 'last', extend: true }).selected, ['b', 'c', 'd']);
  assert.deepEqual(go({ type: 'move', delta: -3, extend: true }).selected, ['a', 'b']);
  assert.deepEqual(go({ type: 'move', delta: -5 }).focus, 'a', 'the focus stops at the ends');
  assert.deepEqual(go({ type: 'all' }).selected, ids);
  assert.deepEqual(go({ type: 'none' }).selected, []);
  assert.deepEqual(go({ type: 'click', id: 'c' }), { selected: ['c'], anchor: 'c', focus: 'c' });
  assert.deepEqual(go({ type: 'click', id: 'a', ctrl: true }).selected, ['c', 'a']);
  assert.deepEqual(go({ type: 'click', id: 'c', ctrl: true }).selected, ['a']);
  assert.deepEqual(go({ type: 'click', id: 'd', shift: true }).selected, ['c', 'd'], 'from the last clicked');
  assert.deepEqual(go({ type: 'only', ids: ['b', 'zz'] }).selected, ['b']);
  s = { selected: ['a', 'gone'], anchor: 'gone', focus: 'gone' };
  assert.deepEqual(go({ type: 'prune' }), { selected: ['a'], anchor: null, focus: 'a' });
  s = { selected: [], anchor: 'gone', focus: null };
  assert.deepEqual(go({ type: 'move', delta: -1, extend: true }), { selected: ['d'], anchor: 'd', focus: 'd' }, 'no focus: the last, alone');
  assert.deepEqual(go({ type: 'click', id: 'b', shift: true }).selected, ['b', 'c', 'd']);
  s = { selected: [], anchor: null, focus: 'zz' };
  assert.equal(go({ type: 'toggle' }).selected.length, 0, 'Space without a shown focus does nothing');
  assert.equal(reduceSelection(s, { type: 'move', delta: 1 }, []), s, 'an empty list keeps it');
  assert.equal(reduceSelection(s, { type: 'nothing' }, ids), s);
  assert.equal(reduceSelection(emptySelection(), { type: 'all' }, ids).focus, 'a');

  assert.deepEqual(keyAction({ key: 'ArrowDown' }), { type: 'move', delta: 1, extend: false });
  assert.deepEqual(keyAction({ key: 'ArrowUp', shiftKey: true }), { type: 'move', delta: -1, extend: true });
  assert.deepEqual(keyAction({ key: 'Home' }), { type: 'move', to: 'first', extend: false });
  assert.deepEqual(keyAction({ key: 'End', shiftKey: true }), { type: 'move', to: 'last', extend: true });
  assert.deepEqual(keyAction({ key: ' ' }), { type: 'toggle' });
  assert.equal(keyAction({ key: ' ', ctrlKey: true }), null);
  assert.deepEqual(keyAction({ key: 'a', ctrlKey: true }), { type: 'all' });
  assert.deepEqual(keyAction({ key: 'A', metaKey: true }), { type: 'all' });
  assert.equal(keyAction({ key: 'a' }), null);
  assert.deepEqual(keyAction({ key: 'Escape' }), { type: 'none' });
  assert.deepEqual(keyAction({ key: 'Delete' }), { command: 'delete' });
  assert.deepEqual(keyAction({ key: 'F2' }), { command: 'rename' });
});

test('each action says what it does with the selection, and why not', () => {
  const full = { remove: true, boot: true };
  const usb = { remove: false, boot: false };
  const mine = managed('internal/video/a.mp4', 10, { entry: sent(1) });
  const theirs = managed('internal/video/b.mp4', 20);
  const ctx = (extra) => ({ features: full, files: [mine], medium: 'internal', card: true, live: false, busy: false, ...extra });
  const why = (can) => Object.fromEntries(Object.entries(can).map(([k, v]) => [k, v.enabled || v.reason]));
  assert.deepEqual(why(actionsFor(ctx())), { move: true, copy: true, rename: true, play: true, boot: true, delete: true, associate: 'hasCopy' });
  assert.deepEqual(why(actionsFor(ctx({ busy: true }))).move, 'busy');
  assert.deepEqual(why(actionsFor(ctx({ files: [] }))), Object.fromEntries(Object.keys(actionsFor(ctx())).map((k) => [k, 'none'])));
  const two = why(actionsFor(ctx({ files: [mine, theirs] })));
  assert.deepEqual([two.move, two.rename, two.play, two.delete, two.associate], [true, 'single', 'single', true, 'single']);
  assert.deepEqual(why(actionsFor(ctx({ card: false }))).move, 'noCard', 'to a card that is not there');
  assert.deepEqual(why(actionsFor(ctx({ card: false, medium: 'sd' }))).copy, true, 'from the card to the internal memory');
  const limited = why(actionsFor(ctx({ features: usb })));
  assert.deepEqual([limited.move, limited.copy, limited.rename, limited.delete, limited.boot], ['deleteUnsupported', true, 'deleteUnsupported', 'deleteUnsupported', 'deleteUnsupported']);
  assert.equal(why(actionsFor(ctx({ live: true }))).play, 'live');
  assert.equal(why(actionsFor(ctx({ files: [theirs] }))).associate, true);
  assert.equal(why(actionsFor(ctx({ files: [managed('sd/video/u.mp4', null)] }))).associate, 'unknownSize');
  assert.deepEqual(selectionInfo([mine, theirs, managed('sd/video/u.mp4', null)], ['internal/video/b.mp4', 'sd/video/u.mp4']).bytes, 20);
});

test('plans read as source → target, with what is left out, warnings and refusals', () => {
  const step = { source: 'internal/video/earth.mp4', target: 'sd/video/earth.mp4', size: 2_516_582, replaces: null };
  assert.equal(stepText(en, 'en', step), 'earth.mp4 (Internal memory) → earth.mp4 (SD card) · 2.5 MB');
  assert.equal(stepText(pt, 'pt-BR', step), 'earth.mp4 (Memória interna) → earth.mp4 (Cartão SD) · 2,5 MB');
  assert.equal(placeText(en, 'sd/image/foto.png'), 'foto.png (SD card)');
  const conflict = { source: 'internal/video/NVI.mp4', target: 'sd/video/nvi.mp4', code: 'conflict', conflict: { path: 'sd/video/NVI.mp4', name: 'NVI.mp4', size: 5_680_675 } };
  assert.equal(skipText(en, 'en', conflict), '“NVI.mp4”: nvi.mp4 (SD card) already exists.');
  assert.equal(skipText(pt, 'pt-BR', { ...conflict, code: 'noLocalCopy', conflict: null }), '“NVI.mp4”: o Bezel não tem cópia dele. Associe o original antes.');
  assert.match(warningText(en, { code: 'bootMedia', path: 'internal/video/earth.mp4' }), /^“earth\.mp4” is what the screen shows at start/);
  assert.match(warningText(pt, { code: 'themeVideo', path: 'sd/video/amd_90.mp4' }), /“amd_90\.mp4” pelo nome/);
  const r = (code, args) => planRefusalText(en, 'en', { code, args });
  assert.equal(r('noSpace', { needed: 6_803_456, free: 6_000_000 }), 'It does not fit: the files need 6.8 MB and 6 MB are free (803 kB short). Nothing was sent or deleted.');
  assert.equal(r('invalidName', { char: 'ç' }), 'The name cannot have “ç”: use letters without accents, digits, “_”, “.” and “-”.');
  assert.match(r('invalidName', {}), /at most 208 characters/);
  assert.equal(r('extensionChanged', { expected: 'mp4' }), 'The new name must end in .mp4.');
  assert.equal(r('extensionChanged', { expected: null }), 'The new name cannot have an extension.');
  assert.equal(r('unsendable', { path: 'sd/video/big.mp4', refusal: { code: 'tooLarge', bytes: 31_457_280, limit: 26_214_400 } }), '“big.mp4” cannot go there: The file is 30 MiB; this screen takes files of up to 25 MiB.');
  assert.equal(r('notListed', { path: 'sd/video/x.mp4' }), '“x.mp4” is no longer on the screen. Refresh the list.');
  assert.equal(planRefusalText(pt, 'pt-BR', { code: 'sameMedium', args: { path: 'sd/video/x.mp4' } }), '“x.mp4” já está lá.');
  assert.deepEqual(planTotals({ steps: [step, { ...step, size: 10 }] }), { count: 2, bytes: 2_516_592 });
});

test('renaming previews the upload name and refuses like the core', () => {
  assert.deepEqual(renamePreview(' NVI_2.MP4 ', 'NVI.mp4'), { name: 'nvi_2.mp4', problem: null });
  assert.deepEqual(renamePreview('nvi.mp4', 'NVI.mp4').problem, { code: 'sameName', args: {} }, 'letter case aside');
  assert.deepEqual(renamePreview('nvi 2.mp4', 'NVI.mp4').problem, { code: 'invalidName', args: { char: ' ' } });
  assert.deepEqual(renamePreview('férias.mp4', 'NVI.mp4').problem.args, { char: 'é' });
  assert.equal(renamePreview('.nvi.mp4', 'NVI.mp4').problem.code, 'invalidName');
  assert.equal(renamePreview('', 'NVI.mp4').problem.code, 'invalidName');
  assert.equal(renamePreview(`${'a'.repeat(205)}.mp4`, 'NVI.mp4').problem.code, 'invalidName');
  assert.deepEqual(renamePreview('nvi.mov', 'NVI.mp4').problem, { code: 'extensionChanged', args: { expected: 'mp4' } });
  assert.deepEqual(renamePreview('notes.txt', 'README').problem, { code: 'extensionChanged', args: { expected: null } });
  assert.equal(renamePreview('readme2', 'README').problem, null);
});

test('a run report says what was done, what failed and where its original is, and what never started', () => {
  const step = (n) => ({ source: `internal/video/${n}.mp4`, target: `sd/video/${n}.mp4`, size: 1, replaces: null });
  const halt = (code, extra = {}) => ({ halt: code, error: null, refusal: null, conflict: null, ...extra });
  const failure = (stage, why) => ({ transfer: 'move', done: [step('a')], failed: { step: step('b'), stage, ...why }, cancelled: null, notStarted: [step('c'), step('d')] });
  const hung = failure('upload', halt('failed', { error: { code: 'hung', args: { detail: 'x' } } }));
  assert.deepEqual(reportLines(en, 'en', hung, errorText), [
    'Moved: 1',
    `“b.mp4” failed: ${errorText(en, hung.failed.error)}`,
    'The original is still there: b.mp4 (Internal memory).',
    'Not started: 2',
  ]);
  // The codes of the core read as sentences, never as its English.
  const changed = reportLines(pt, 'pt-BR', failure('preflight', halt('sourceChanged')), errorText);
  assert.equal(changed[1], '“b.mp4” falhou: Sumiu da tela ou mudou desde que a lista foi feita. Atualize a lista.');
  const gone = reportLines(en, 'en', failure('preflight', halt('noLocalCopy')), errorText);
  assert.equal(gone[1], '“b.mp4” failed: Bezel no longer has its local copy (the cache was cleared). Associate its original first.');
  const taken = reportLines(en, 'en', failure('preflight', halt('conflict', { conflict: { path: 'sd/video/b.mp4' } })), errorText);
  assert.equal(taken[1], '“b.mp4” failed: “b.mp4” is there now and replacing it was not confirmed.');
  // Past the check the copy is at the target; past the delete, only it.
  const copied = reportLines(en, 'en', failure('delete', halt('failed', { error: { code: 'timeout', args: {} } })), errorText);
  assert.equal(copied[2], 'Its copy is checked at b.mp4 (SD card), and the original is still there too: b.mp4 (Internal memory).');
  const lagging = reportLines(en, 'en', failure('catalog', halt('failed', { error: { code: 'fileError', args: {} } })), errorText);
  assert.equal(lagging[2], 'It is at b.mp4 (SD card) now and the original was deleted, but Bezel could not update its list of the files it sent.');
  assert.ok(!lagging.some((line) => line.includes('still there')));
  assert.equal(reportLines(en, 'en', failure('later', halt('failed')), errorText).length, 3, 'an unknown stage says nothing of the original');

  const cancelled = { transfer: 'rename', done: [], failed: null, cancelled: { step: step('a'), stage: 'upload', partial: 10 }, notStarted: [] };
  assert.deepEqual(reportLines(pt, 'pt-BR', cancelled, errorText), ['Renomeados: 0', 'Cancelado em “a.mp4”.', 'O original continua lá: a.mp4 (Memória interna).']);
  const refused = { transfer: 'restore', done: [], failed: { step: step('a'), stage: 'preflight', ...halt('refused', { refusal: { code: 'noSpace', bytes: 2_000_000, limit: 1_000_000 } }) }, cancelled: { step: step('b'), stage: 'upload', partial: null }, notStarted: [] };
  const lines = reportLines(en, 'en', refused, errorText);
  assert.equal(lines[1], '“a.mp4” failed: It does not fit: the file is 2 MB and 1 MB are free. Nothing was deleted; if you want, delete files below and try again.');
  assert.equal(lines[2], 'Cancelled at “b.mp4”.');
  assert.equal(lines.length, 3, 'a restore deletes no original');
});

test('the cleanup assistant groups findings, the exact signals first and only they checked', () => {
  const f = (path, size, code, extra = {}) => managed(path, size, { finding: { code, prechecked: ['duplicate', 'hangPartial', 'pending'].includes(code), kept: null, cataloged: null, ...extra } });
  const files = [
    f('sd/video/NVI.mp427034822.mp4', 5_352_433, 'variant', { kept: 'sd/video/NVI.mp4' }),
    f('sd/video/AMD.mp4', 4_079_432, 'unused'),
    f('sd/video/bezel_test_cancel.mp4', 29_577_216, 'hangPartial'),
    f('sd/video/x.mp4', 7, 'sizeDiffers', { cataloged: 9_000 }),
    managed('internal/video/earth.mp4', 2_516_582, { entry: sent(1) }),
  ];
  assert.deepEqual(cleanupGroups(files).map((g) => [g.code, g.files.map((x) => x.name)]), [
    ['hangPartial', ['bezel_test_cancel.mp4']], ['sizeDiffers', ['x.mp4']], ['variant', ['NVI.mp427034822.mp4']], ['unused', ['AMD.mp4']],
  ]);
  assert.deepEqual(precheckedPaths(files), ['sd/video/bezel_test_cancel.mp4']);
  assert.equal(findingText(en, 'en', files[0]), 'A re-converted copy of “NVI.mp4”, which stays; the sizes differ.');
  assert.equal(findingText(pt, 'pt-BR', files[3]), 'Não é o tamanho que o Bezel enviou para cá (9 kB).');
});

test('a restore offers what is missing or on another card, and checks the free space', () => {
  const restorable = [
    { id: 'a', medium: 'sd', size: 6_291_456, otherCard: false },
    { id: 'b', medium: 'sd', size: 512_000, otherCard: true },
    { id: 'c', medium: 'internal', size: 1, otherCard: false },
  ];
  assert.deepEqual(restorableFor(restorable, 'sd').map((r) => r.id), ['a', 'b']);
  assert.deepEqual(restorableFor(null, 'sd'), []);
  assert.deepEqual(restoreTotals(restorable, ['a', 'b'], 10_000_000), { count: 2, bytes: 6_803_456, fits: true });
  assert.equal(restoreTotals(restorable, ['a', 'b'], 6_803_456).fits, false, 'each upload needs less than the free space');
  assert.deepEqual(restoreTotals(restorable, [], 0), { count: 0, bytes: 0, fits: true });
});

test('a restore offers files deleted through Bezel too, unchecked', () => {
  const entries = [
    { id: 'missing', medium: 'sd', size: 1, localCopy: true, state: 'missing', otherCard: false },
    { id: 'other', medium: 'sd', size: 1, localCopy: true, state: 'stored', otherCard: true },
    { id: 'cleared', medium: 'sd', size: 1, localCopy: false, state: 'missing', otherCard: false },
    { id: 'deleted', medium: 'sd', size: 1, localCopy: true, state: 'deleted', otherCard: false },
  ];
  assert.deepEqual(restorableFor(entries, 'sd').map((e) => e.id), ['missing', 'other', 'cleared', 'deleted'], 'offered');
  assert.deepEqual(restoreDefaults(entries), ['missing', 'other'], 'checked at first: never a deleted one, nor one without a copy');
  for (const t of [pt, en]) assert.notEqual(t('storage.state.deleted'), 'storage.state.deleted');
});

// ------------------------------------------------ the demo's core rules --

/** The user's listing as `{path, size}`, the synthetic hang partial included. */
const userListing = () => VENDOR_STORAGE.files.map(([path, size]) => ({ path, size }));

test('the demo finds the user\'s vendor copies and the hang partial like the core', () => {
  const found = demoFindings(userListing(), () => null, () => false);
  const variants = [...found].filter(([, f]) => f.code === 'variant').map(([p, f]) => [p.split('/')[2], f.kept.split('/')[2]]);
  assert.deepEqual(variants, [
    ['demon_open.mp4.mp4.mp4', 'demon_open.mp4.mp4'],
    ['demon.mp401115025.mp4', 'demon.mp4.mp4.mp4'],
    ['NVI.mp427034822.mp4', 'NVI.mp4'],
    ['Rani.mp417075004.mp4', 'Rani.mp4'],
    ['m04.mp424045157.mp4', 'm04.mp4'],
  ]);
  const checked = [...found].filter(([, f]) => f.prechecked).map(([p, f]) => [p, f.code]);
  assert.deepEqual(checked, [['sd/video/bezel_test_cancel.mp4', 'hangPartial']], 'nothing of the user\'s is checked');
  assert.equal(found.get('sd/video/8.8APEX_2.mp4').code, 'unused');
  assert.equal(found.size, VENDOR_STORAGE.files.length, 'nothing left out');
  assert.equal(DEMO_HANG_PARTIAL, 29_577_216);
  // Bezel's verified files get none; protected ones never show.
  const mine = demoFindings(userListing(), (p) => (p === 'internal/video/earth.mp4' ? { state: 'stored', size: 2_516_582 } : null), (p) => p.endsWith('AMD.mp4'));
  assert.equal(mine.has('internal/video/earth.mp4'), false);
  assert.equal(mine.has('sd/video/AMD.mp4'), false);
});

test('the demo checks only exact signals, like the core\'s test', () => {
  const entries = {
    'sd/video/mine.mp4': { state: 'stored', size: 50 },
    'sd/video/changed.mp4': { state: 'stored', size: 60 },
    'sd/video/half.mp4': { state: 'pending', size: 70 },
  };
  const listing = [
    ['sd/video/mine.mp4', 50], ['sd/video/mine.mp4.mp4', 50], ['sd/video/changed.mp4', 61], ['sd/video/half.mp4', 12],
    ['sd/video/gone.mp4', 80], ['sd/video/x.mp4', 10], ['sd/video/x.mp4.mp4', 10], ['sd/video/x.mp4.mp4.mp4', 10],
    ['sd/video/x.mp41.mp4', 11], ['sd/video/x.mp42.mp4', 11], ['sd/video/x.mp43.mp4', null],
    ['internal/video/copy_of_changed.mp4', 61], ['internal/image/copy.png', 61],
  ].map(([path, size]) => ({ path, size }));
  const found = demoFindings(listing, (p) => entries[p] ?? null, () => false);
  assert.deepEqual([...found].map(([p, f]) => [p, f.code]), [
    ['sd/video/mine.mp4.mp4', 'duplicate'], ['sd/video/changed.mp4', 'sizeDiffers'], ['sd/video/half.mp4', 'pending'],
    ['sd/video/gone.mp4', 'unused'], ['sd/video/x.mp4', 'unused'], ['sd/video/x.mp4.mp4', 'duplicate'], ['sd/video/x.mp4.mp4.mp4', 'duplicate'],
    ['sd/video/x.mp41.mp4', 'variant'], ['sd/video/x.mp42.mp4', 'duplicate'], ['sd/video/x.mp43.mp4', 'variant'],
    ['internal/video/copy_of_changed.mp4', 'sameSize'], ['internal/image/copy.png', 'unused'],
  ]);
  assert.equal(found.get('sd/video/x.mp42.mp4').kept, 'sd/video/x.mp41.mp4');
  assert.equal(found.get('internal/video/copy_of_changed.mp4').kept, 'sd/video/changed.mp4');
  assert.equal(found.get('sd/video/changed.mp4').cataloged, 60);
});

test('the demo never checks the file a group keeps, like the core\'s test', () => {
  const pending = (path) => (p) => (p === path ? { state: 'pending', size: 100 } : null);
  const sized = (paths) => paths.map((path) => ({ path, size: 100 }));
  const checked = (found) => [...found].filter(([, f]) => f.prechecked).map(([p]) => p);
  // Two artifact names of equal size next to their base file, an unfinished upload.
  const clip = demoFindings(sized(['sd/video/clip.mp4', 'sd/video/clip.mp4.mp4', 'sd/video/clip.mp41.mp4']), pending('sd/video/clip.mp4'), () => false);
  assert.deepEqual(checked(clip), ['sd/video/clip.mp4', 'sd/video/clip.mp41.mp4']);
  assert.equal(clip.get('sd/video/clip.mp41.mp4').kept, 'sd/video/clip.mp4.mp4');
  // A group whose shortest name is itself an artifact.
  const demo = demoFindings(sized(['sd/video/demo.mp4.mp4', 'sd/video/demo.mp4.mp4.mp4', 'sd/video/demo.mp4123.mp4']), pending('sd/video/demo.mp4.mp4'), () => false);
  assert.deepEqual(checked(demo), ['sd/video/demo.mp4.mp4', 'sd/video/demo.mp4.mp4.mp4']);
  assert.equal(demo.get('sd/video/demo.mp4.mp4.mp4').kept, 'sd/video/demo.mp4123.mp4');
  // A protected file stays for its repeats even when its upload is unfinished.
  const theme = demoFindings([{ path: 'sd/video/amd_90.mp4', size: 200 }, { path: 'sd/video/amd_90.mp4.mp4', size: 201 }], pending('sd/video/amd_90.mp4'), (p) => p === 'sd/video/amd_90.mp4');
  assert.deepEqual([...theme].map(([p, f]) => [p, f.code, f.kept]), [['sd/video/amd_90.mp4.mp4', 'variant', 'sd/video/amd_90.mp4']]);
});

test('demo names follow the core: artifacts, upload names and the same file on FAT', () => {
  for (const [name, base] of [
    ['demon_open.mp4.mp4.mp4', 'demon_open.mp4'], ['NVI.mp427034822.mp4', 'nvi.mp4'], ['x.mp41.mp4.mp4', 'x.mp4'], ['logo.png.png', 'logo.png'],
    ['8.8APEX_2.mp4', null], ['m04.mp4', null], ['clip.mp4x.mp4', null], ['.mp4.mp4', null], ['noext', null], ['trailing.', null],
  ]) assert.equal(demoArtifactBase(name), base, name);
  assert.equal(demoUploadName('NVI.mp4'), 'nvi.mp4');
  assert.equal(demoUploadName('Férias Praia.MP4'), 'f_rias_praia.mp4');
  assert.equal(demoUploadName('...'), 'media');
  assert.deepEqual(demoForUpload('a b.mp4'), { error: { char: ' ' } });
  assert.deepEqual(demoForUpload(''), { error: {} });
  assert.equal(demoSuggest('x', ''), 'x');
  assert.equal(demoExtension('README'), null);
  assert.ok(demoSameFile('sd/video/NVI.mp4', 'sd/video/nvi.mp4'));
  assert.ok(!demoSameFile('sd/video/nvi.mp4', 'internal/video/nvi.mp4'));
});

/** A screen as the demo's plans see it. */
function view({ files, entries = {}, card = 31_890_132_172, deletes = true, boot = null, themeVideos = [] }) {
  const sizes = new Map(files);
  return {
    files: sizes,
    card,
    deletes,
    clash: (target) => {
      const path = [...sizes.keys()].find((p) => demoSameFile(p, target));
      return path ? { path, name: path.split('/')[2], size: sizes.get(path) } : null;
    },
    copyOf: (path) => entries[path] ?? null,
    isBoot: (path) => path === boot,
    isThemeVideo: (path) => themeVideos.includes(path),
  };
}

test('demo plans move, copy and rename one file at a time, like the core', () => {
  const copy = (size) => ({ size, content: `c${size}` });
  const v = view({
    files: [['internal/video/NVI.mp4', 5], ['internal/video/nvi.mp4', 6], ['internal/video/a.mp4', 7], ['internal/video/b.mp4', 8], ['sd/video/a.mp4', 1]],
    entries: { 'internal/video/NVI.mp4': copy(5), 'internal/video/nvi.mp4': copy(6), 'internal/video/a.mp4': copy(7) },
    boot: 'internal/video/a.mp4',
  });
  const plan = demoPlanAcross(v, 'move', ['internal/video/NVI.mp4', 'internal/video/nvi.mp4', 'internal/video/a.mp4', 'internal/video/b.mp4'], 'sd');
  assert.deepEqual(plan.steps.map((s) => [s.source, s.target]), [['internal/video/NVI.mp4', 'sd/video/nvi.mp4']]);
  assert.deepEqual(plan.skipped.map((s) => [s.source, s.code, s.conflict?.path ?? null]), [
    ['internal/video/nvi.mp4', 'conflict', 'sd/video/nvi.mp4'],
    ['internal/video/a.mp4', 'conflict', 'sd/video/a.mp4'],
    ['internal/video/b.mp4', 'noLocalCopy', null],
  ]);
  assert.deepEqual(plan.warnings, [], 'the boot media was left out');
  const over = demoPlanAcross(v, 'move', ['internal/video/a.mp4'], 'sd', ['sd/video/a.mp4']);
  assert.deepEqual(over.steps[0].replaces, { path: 'sd/video/a.mp4', name: 'a.mp4', size: 1 });
  assert.deepEqual(over.warnings, [{ code: 'bootMedia', path: 'internal/video/a.mp4' }]);
  assert.deepEqual(demoPlanAcross(v, 'copy', ['internal/video/a.mp4'], 'sd', ['sd/video/a.mp4']).warnings, [], 'a copy deletes nothing');
  assert.equal(demoPlanAcross({ ...v, card: null }, 'move', [], 'sd').code, 'noCard');
  assert.deepEqual(demoPlanAcross(v, 'move', ['sd/video/a.mp4'], 'sd').code, 'sameMedium');
  assert.deepEqual(demoPlanAcross(v, 'move', ['internal/video/none.mp4'], 'sd').args, { path: 'internal/video/none.mp4' });
  const usb = demoPlanAcross({ ...v, deletes: false }, 'move', ['internal/video/NVI.mp4'], 'sd');
  assert.deepEqual(usb.skipped.map((s) => s.code), ['deleteUnsupported']);
  assert.equal(demoPlanAcross({ ...v, deletes: false }, 'copy', ['internal/video/NVI.mp4'], 'sd').steps.length, 1, 'TUR_USB copies');

  const rename = (name, extra = {}) => demoPlanRename({ ...v, ...extra }, 'internal/video/NVI.mp4', name);
  assert.deepEqual(rename('NVI 2.mp4').args, { char: ' ' });
  assert.deepEqual(rename('nvi_2.mov').args, { expected: 'mp4' });
  assert.equal(rename('nvi.MP4').code, 'sameName');
  assert.deepEqual(rename('A.MP4').skipped.map((s) => s.code), ['conflict'], 'a.mp4 is there');
  const theme = rename('nvi_2.mp4', { isThemeVideo: () => true });
  assert.deepEqual([theme.steps[0].target, theme.warnings[0].code], ['internal/video/nvi_2.mp4', 'themeVideo']);
});

test('demo restores check every file and the free space before sending, oldest first', () => {
  const v = view({ files: [['sd/video/here.mp4', 10], ['sd/video/other.mp4', 3]] });
  const entry = (name, size, sentAt, extra = {}) => ({ path: `sd/video/${name}`, size, sentAt, localCopy: true, content: name, ...extra });
  const room = { free: 1_000, cap: 100 };
  const plan = demoPlanRestore(v, [entry('b.mp4', 20, 2), entry('a.mp4', 30, 1), entry('here.mp4', 10, 3), entry('other.mp4', 4, 4), entry('lost.mp4', 5, 5, { localCopy: false })], 'sd', room);
  assert.deepEqual(plan.steps.map((s) => s.target), ['sd/video/a.mp4', 'sd/video/b.mp4']);
  assert.deepEqual(plan.skipped.map((s) => [s.source, s.code]), [['sd/video/here.mp4', 'present'], ['sd/video/other.mp4', 'conflict'], ['sd/video/lost.mp4', 'noLocalCopy']]);
  const over = demoPlanRestore(v, [entry('other.mp4', 4, 4)], 'sd', room, ['sd/video/other.mp4']);
  assert.equal(over.steps[0].replaces.size, 3);
  const twice = demoPlanRestore(v, [entry('a.mp4', 30, 1), { ...entry('a.mp4', 31, 2), path: 'internal/video/a.mp4' }], 'sd', room);
  assert.deepEqual(twice.skipped.map((s) => s.code), ['conflict']);
  assert.deepEqual(demoPlanRestore(v, [entry('big.mp4', 101, 1)], 'sd', room).args, { path: 'sd/video/big.mp4', refusal: { code: 'tooLarge', bytes: 101, limit: 100 } });
  assert.deepEqual(demoPlanRestore(v, [entry('empty.mp4', 0, 1)], 'sd', room).args.refusal, { code: 'emptyFile' });
  assert.deepEqual(demoPlanRestore(v, [entry('a.mp4', 30, 1)], 'sd', { free: 30, cap: 100 }).args, { needed: 30, free: 30 });
  assert.equal(demoPlanRestore({ ...v, card: null }, [], 'sd', room).code, 'noCard');
});

test('the demo ranks a file\'s originals like the core: exact size and kind, then name, resolution and play time', () => {
  const sought = { path: 'internal/video/DARIUS.mp4', size: 100, resolution: { width: 480, height: 1920 }, durationMs: 12_000 };
  const c = (source, extra = {}) => ({ source, size: 100, kind: 'video', resolution: null, durationMs: null, ...extra });
  const ranked = demoRank(sought, [
    c('/v/abertura.mp4'),
    c('/v/darius_old.mp4', { resolution: { width: 480, height: 1920 } }),
    c('/v/DARIUS.mp4'),
    c('/v/dar.mp4', { durationMs: 12_500 }),
    c('/v/dar.mp4', { durationMs: 20_000, source: '/w/dar.mp4' }),
    c('/v/big.mp4', { size: 101 }),
    c('/v/pic.png', { kind: 'image' }),
  ]);
  assert.deepEqual(ranked.map((x) => [x.source, x.sameName]), [
    ['/v/DARIUS.mp4', true], ['/v/darius_old.mp4', false], ['/v/dar.mp4', false], ['/w/dar.mp4', false], ['/v/abertura.mp4', false],
  ]);
});

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { translator } from '../../src/i18n/index.js';
import { baseName, errorMessage, formatBytes, progressParts, refusalText, storageFeatures, usedFraction } from '../../src/ui/storage.js';
import { createDemoBackend, demoKindOf, demoSuggestName, demoTurns, demoVideoName } from '../../src/demo-backend.js';
import { DEMO_PICKED, DEMO_STORAGE, DEMO_VIDEO_THEME, SCENARIOS } from '../../src/demo-data.js';

const pt = translator('pt-BR');
const en = translator('en');
const instant = { now: () => 1000, delay: () => Promise.resolve() };
const KEY = '/dev/ttyACM1';

test('sizes are decimal, like the screens\' limits', () => {
  assert.equal(formatBytes(512, 'en'), '512 B');
  assert.equal(formatBytes(184_320, 'en'), '184 kB');
  assert.equal(formatBytes(18_874_368, 'en'), '18.9 MB');
  assert.equal(formatBytes(24_117_248, 'pt-BR'), '24,1 MB');
  assert.equal(formatBytes(120_000_000, 'en'), '120 MB');
  assert.equal(formatBytes(31_914_983_424, 'en'), '31.9 GB');
  assert.equal(formatBytes(null, 'en'), '—');
  assert.equal(usedFraction({ total: 200, used: 50 }), 0.25);
  assert.equal(usedFraction({ total: 0, used: 0 }), 0);
  assert.equal(usedFraction(null), 0);
});

test('the tab follows what the screen can do', () => {
  const [turing88] = SCENARIOS.turing88.screens;
  const [turzx] = SCENARIOS.turzx.screens;
  assert.deepEqual(storageFeatures(turing88), { storage: true, remove: true, boot: true });
  assert.deepEqual(storageFeatures(turzx), { storage: true, remove: false, boot: false }, 'TUR_USB: no delete, no boot');
  const serial = { family: 'turing-rev-a', models: [{ capabilities: { storage: false } }] };
  assert.deepEqual(storageFeatures(serial), { storage: false, remove: false, boot: false });
  assert.deepEqual(storageFeatures(null), { storage: false, remove: false, boot: false });
});

test('progress reads as a phase and an amount', () => {
  assert.deepEqual(progressParts(pt, 'pt-BR', { phase: 'upload', done: 500_000, total: 2_000_000 }), {
    phase: 'Enviando', amount: '500 kB de 2 MB (25%)', fraction: 0.25,
  });
  assert.deepEqual(progressParts(en, 'en', { phase: 'convert', done: 999, total: 1000 }), { phase: 'Converting', amount: '99%', fraction: 0.999 });
  assert.deepEqual(progressParts(en, 'en', { phase: 'convert', done: 5, total: 0 }), { phase: 'Converting', amount: '', fraction: null });
  assert.equal(progressParts(pt, 'pt-BR', { phase: 'verify', done: 0, total: 1 }).phase, 'Conferindo');
});

test('every refusal of the preflight has its own sentence', () => {
  const r = (code, extra = {}) => refusalText(pt, 'pt-BR', { code, message: code, ...extra });
  assert.match(r('noSpace', { bytes: 5_000_000, limit: 1_000_000 }), /5 MB e há 1 MB livres/);
  assert.match(r('tooLarge', { bytes: 130_000_000, limit: 120_000_000 }), /130 MB; a tela aceita até 120 MB/);
  assert.match(r('needsConverter', { mismatches: [{ code: 'audio' }, { code: 'resolution', found: '1920x1080', expected: '480x1920' }] }), /\(tem áudio; tem 1920x1080 em vez de 480x1920\)/);
  assert.match(r('wrongProfile', { mismatches: [{ code: 'format', found: 'WebP', expected: 'JPEG, PNG' }, { code: 'resolution', expected: '480x1920' }] }), /formato WebP.*tamanho desconhecido em vez de 480x1920/);
  assert.match(r('wrongExtension', { accepted: ['jpg', 'jpeg'] }), /\.jpg, \.jpeg/);
  assert.match(r('invalidName', { name: 'ç' }), /“ç”/);
  assert.match(r('invalidName'), /letras sem acento/);
  for (const code of ['wrongKind', 'emptyFile', 'noCard']) assert.doesNotMatch(r(code), /storage\./, code);
  for (const code of ['codec', 'pixelFormat', 'bFrames']) assert.doesNotMatch(r('wrongProfile', { mismatches: [{ code }] }), /storage\./, code);
  assert.equal(refusalText(en, 'en', { code: 'somethingNew', message: 'core text' }), 'core text');
});

test('storage errors are translated by code, others keep their text', () => {
  assert.match(errorMessage(pt, { code: 'unsupported', message: 'not supported: delete' }), /não permite/);
  assert.match(errorMessage(pt, { code: 'busy' }), /ocupada/);
  assert.equal(errorMessage(en, new Error('boom')), 'It did not work: boom');
  assert.equal(errorMessage(en, 'plain'), 'It did not work: plain');
  assert.equal(baseName('/home/me/Vídeos/clip.mp4'), 'clip.mp4');
  assert.equal(baseName('C:\\Users\\me\\clip.mp4'), 'clip.mp4');
});

test('demo names, kinds and theme videos follow the core', () => {
  assert.equal(demoSuggestName('Férias 2026.MOV', 'mp4'), 'f_rias_2026.mp4');
  assert.equal(demoSuggestName('...', 'png'), 'media.png');
  assert.equal(demoKindOf('a.JPG'), 'image');
  assert.equal(demoKindOf('a.webm'), 'video');
  assert.equal(demoKindOf('notes'), null);
  assert.deepEqual(['portrait', 'landscape', 'reverse-portrait', 'reverse-landscape'].map(demoTurns), [2, 1, 0, 3]);
  assert.equal(demoVideoName(DEMO_VIDEO_THEME), 'nebula_90.mp4');
});

test('demo storage lists, uploads with progress, cancels and deletes only when confirmed', async () => {
  const demo = createDemoBackend('turing88', instant);
  const overview = await demo.storageOverview(KEY);
  assert.equal(overview.folders.length, 4);
  assert.equal(overview.card.total, DEMO_STORAGE.cardTotal);
  assert.equal(overview.internal.used, 184_320 + 18_874_368);
  assert.equal((await demo.mediaTools()).ready, true);
  const seen = [];
  demo.onJobProgress((p) => seen.push(p));

  const source = await demo.pickMedia();
  assert.equal(source, DEMO_PICKED);
  const ready = await demo.prepareUpload(KEY, source, 'internal');
  assert.equal(ready.status, 'ready');
  assert.equal(ready.target.path, 'internal/video/ferias.mp4');
  assert.deepEqual(ready.convert, { width: 480, height: 1920, quarterTurns: 0, cropped: true });
  const done = await demo.runUpload(ready.ticket, false);
  assert.equal(done.status, 'done');
  assert.ok(done.file.size > 0);
  assert.deepEqual([...new Set(seen.map((p) => p.phase))], ['convert', 'upload', 'verify']);
  await assert.rejects(demo.runUpload(ready.ticket, false), (e) => e.code === 'stale');

  // The same file again replaces it: only with the overwrite confirmation.
  const again = await demo.prepareUpload(KEY, source, 'internal');
  assert.equal(again.replaces.name, 'ferias.mp4');
  await assert.rejects(demo.runUpload(again.ticket, false), (e) => e.code === 'notConfirmed');

  // Cancel in the middle of the upload: the partial file stays until deleted.
  const image = demo.fileSource({ name: 'Mapa.png', size: 800_000 });
  const pending = await demo.prepareUpload(KEY, image, 'sd');
  assert.equal(pending.target.path, 'sd/image/mapa.png');
  const unsubscribe = demo.onJobProgress((p) => { if (p.phase === 'upload' && p.done > 0) demo.cancelJob(); });
  const cancelled = await demo.runUpload(pending.ticket, false);
  unsubscribe();
  assert.equal(cancelled.status, 'cancelled');
  assert.equal(cancelled.partial, 50_000);
  assert.equal(await demo.cancelJob(), false, 'nothing runs');
  await assert.rejects(demo.deleteStored(KEY, cancelled.path, false), (e) => e.code === 'notConfirmed');
  await demo.deleteStored(KEY, cancelled.path, true);
  assert.equal(demo.storageState().files.has(cancelled.path), false);
  assert.equal(demo.fileSource(null), null);
  await assert.rejects(demo.prepareUpload(KEY, 'demo://missing.png', 'sd'), (e) => e.code === 'failed');
});

test('demo storage refuses like the preflight', async () => {
  const demo = createDemoBackend('noffmpeg', instant);
  assert.equal((await demo.storageOverview(KEY)).card, null);
  const video = await demo.prepareUpload(KEY, 'demo://ferias.mp4', 'internal');
  assert.equal(video.code, 'needsConverter');
  assert.equal(video.mismatches[1].found, '1920x1080');
  assert.equal((await demo.prepareUpload(KEY, 'demo://relogio.mp4', 'internal')).status, 'ready', 'already in the profile');
  assert.equal((await demo.prepareUpload(KEY, 'demo://foto.png', 'sd')).code, 'noCard');
  assert.equal((await demo.prepareUpload(KEY, demo.fileSource({ name: 'notes.txt', size: 3 }), 'internal')).code, 'wrongKind');
  assert.equal((await demo.prepareUpload(KEY, demo.fileSource({ name: 'vazio.png', size: 0 }), 'internal')).code, 'emptyFile');
  const huge = demo.fileSource({ name: 'huge.png', size: 8_000_000_000 });
  const full = await demo.prepareUpload(KEY, huge, 'internal');
  assert.equal(full.code, 'noSpace');
  assert.deepEqual(full.candidates.map((c) => c.name), ['amd_90.mp4', 'logo.png'], 'largest first');
  const tools = await demo.locateFfmpeg();
  assert.deepEqual([tools.ready, tools.configured], [true, '/opt/ffmpeg/bin/ffmpeg']);
  assert.equal((await demo.prepareUpload(KEY, 'demo://ferias.mp4', 'internal')).status, 'ready');
  await assert.rejects(demo.storageOverview('COM9'), (e) => e.code === 'unsupported');
});

test('demo playback, boot media and what live mode allows', async () => {
  const demo = createDemoBackend('turing88', instant);
  const logo = 'internal/image/logo.png';
  await demo.playStored(KEY, logo);
  assert.equal(demo.storageState().playback, logo);
  await demo.stopPlayback(KEY);
  assert.equal(demo.storageState().playback, null);
  await assert.rejects(demo.playStored(KEY, 'internal/image/none.png'), (e) => e.code === 'failed');
  await assert.rejects(demo.setBootMedia(KEY, logo, false), (e) => e.code === 'notConfirmed');
  await demo.setBootMedia(KEY, logo, true);
  assert.equal(demo.storageState().boot, logo);
  await demo.setBootMedia(KEY, null, true);
  assert.equal(demo.storageState().boot, null);
  await assert.rejects(demo.setBootMedia(KEY, 'internal/video/none.mp4', true), (e) => e.code === 'failed');
  await demo.setLive(true, KEY);
  await assert.rejects(demo.playStored(KEY, logo), (e) => e.code === 'live');
  await assert.rejects(demo.stopPlayback(KEY), (e) => e.code === 'live');

  const turzx = createDemoBackend('turzx', instant);
  const [screen] = await turzx.listScreens();
  await assert.rejects(turzx.deleteStored(screen.key, logo, true), (e) => e.code === 'unsupported');
  await assert.rejects(turzx.setBootMedia(screen.key, logo, true), (e) => e.code === 'unsupported');
});

test('a live theme video missing from the screen is sent on request', async () => {
  const demo = createDemoBackend('video', instant);
  assert.equal((await demo.session()).theme.background.type, 'video');
  assert.equal((await demo.sample()).video, null, 'not live');
  await assert.rejects(demo.prepareThemeVideo(KEY), (e) => e.code === 'noVideo');
  await demo.setLive(true, KEY);
  assert.deepEqual((await demo.sample()).video, { state: 'missing', path: 'sd/video/nebula_90.mp4' });
  const ready = await demo.prepareThemeVideo(KEY);
  assert.equal(ready.source, 'nebula.mp4');
  assert.equal(ready.convert.quarterTurns, 1);
  assert.equal((await demo.runUpload(ready.ticket, false)).status, 'done');
  assert.deepEqual((await demo.sample()).video, { state: 'onDevice', path: 'sd/video/nebula_90.mp4' });
  await assert.rejects(demo.prepareThemeVideo(KEY), (e) => e.code === 'noVideo');
});

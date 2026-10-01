import { test } from 'node:test';
import assert from 'node:assert/strict';
import { translator } from '../../src/i18n/index.js';
import { baseName, bootKeepsText, formatBytes, formatMiB, progressParts, refusalText, storageFeatures, usedFraction } from '../../src/ui/storage.js';
import { DEMO_REV_C_CAP, DEMO_USB_CAP, createDemoBackend, createDemoGate, demoHung, demoKindOf, demoSuggestName, demoTurns, demoVideoName } from '../../src/demo-backend.js';
import { DEMO_PICKED, DEMO_STORAGE, DEMO_VIDEO_THEME, SCENARIOS } from '../../src/demo-data.js';

const pt = translator('pt-BR');
const en = translator('en');
const instant = { now: () => 1000, delay: () => Promise.resolve() };
const KEY = '/dev/ttyACM1';

test('per-file limits read in MiB, rounded the safe way', () => {
  assert.equal(formatMiB(26_214_400, 'en'), '25 MiB');
  assert.equal(formatMiB(26_214_401, 'en', 'up'), '25.1 MiB');
  assert.equal(formatMiB(26_214_401, 'en', 'down'), '25 MiB');
  assert.equal(formatMiB(120_000_000, 'pt-BR'), '114,4 MiB');
  assert.equal(formatMiB(undefined, 'en'), '—');
});

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
  assert.match(r('tooLarge', { bytes: 130_000_000, limit: 120_000_000 }), /124 MiB; esta tela aceita arquivos de até 114,4 MiB/);
  assert.match(r('convertedTooLarge', { bytes: 26_214_401, limit: 26_214_400 }), /o vídeo tem 25,1 MiB; esta tela aceita arquivos de até 25 MiB\. Envie um trecho mais curto/);
  assert.equal(refusalText(en, 'en', { code: 'tooLarge', bytes: 31_457_280, limit: 26_214_400 }), 'The file is 30 MiB; this screen takes files of up to 25 MiB.');
  assert.match(r('needsConverter', { mismatches: [{ code: 'audio' }, { code: 'resolution', found: '1920x1080', expected: '480x1920' }] }), /\(tem áudio; tem 1920x1080 em vez de 480x1920\)/);
  assert.match(r('wrongProfile', { mismatches: [{ code: 'format', found: 'WebP', expected: 'JPEG, PNG' }, { code: 'resolution', expected: '480x1920' }] }), /formato WebP.*tamanho desconhecido em vez de 480x1920/);
  assert.match(r('wrongExtension', { accepted: ['jpg', 'jpeg'] }), /\.jpg, \.jpeg/);
  assert.match(r('invalidName', { name: 'ç' }), /“ç”/);
  assert.match(r('invalidName'), /letras sem acento/);
  for (const code of ['wrongKind', 'emptyFile', 'noCard']) assert.doesNotMatch(r(code), /storage\./, code);
  for (const code of ['codec', 'pixelFormat', 'bFrames']) assert.doesNotMatch(r('wrongProfile', { mismatches: [{ code }] }), /storage\./, code);
  assert.equal(refusalText(en, 'en', { code: 'somethingNew', message: 'core text' }), 'core text');
});

test('file names come from local paths', () => {
  assert.equal(baseName('/home/me/Vídeos/clip.mp4'), 'clip.mp4');
  assert.equal(baseName('C:\\Users\\me\\clip.mp4'), 'clip.mp4');
});

test('the boot dialog names the brightness the screen starts with', () => {
  const pt = translator('pt-BR');
  assert.equal(bootKeepsText(pt, 40), 'Ela liga com brilho de 40%, o nível que você ajustou no Bezel, e nunca entra em repouso sozinha.');
  assert.match(bootKeepsText(pt, null), /brilho padrão, cerca de 67%/);
  assert.match(bootKeepsText(pt, undefined), /cerca de 67%/);
  assert.equal(bootKeepsText(translator('en'), 0), 'It starts with brightness 0%, the level you set in Bezel, and never goes to sleep on its own.');
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
  await assert.rejects(demo.prepareUpload(KEY, 'demo://missing.png', 'sd'), (e) => e.code === 'fileError' && e.args.file === 'demo://missing.png');
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
  const huge = demo.fileSource({ name: 'huge.png', size: 20_000_000 });
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
  await assert.rejects(demo.playStored(KEY, 'internal/image/none.png'), (e) => e.code === 'invalidInput');
  await assert.rejects(demo.setBootMedia(KEY, logo, false), (e) => e.code === 'notConfirmed');
  await demo.setBootMedia(KEY, logo, true);
  assert.equal(demo.storageState().boot, logo);
  assert.equal(demo.storageState().bootBrightness, null, 'the screen keeps its own');
  await demo.setBootMedia(KEY, null, true, 40);
  assert.equal(demo.storageState().boot, null);
  assert.equal(demo.storageState().bootBrightness, 40);
  await assert.rejects(demo.setBootMedia(KEY, 'internal/video/none.mp4', true), (e) => e.code === 'invalidInput');
  await demo.setLive(true, KEY);
  await assert.rejects(demo.playStored(KEY, logo), (e) => e.code === 'live');
  await assert.rejects(demo.stopPlayback(KEY), (e) => e.code === 'live');

  const turzx = createDemoBackend('turzx', instant);
  const [screen] = (await turzx.listDevices()).screens;
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

/** Lets every pending promise callback run. */
const settle = async () => {
  for (let i = 0; i < 50; i += 1) await new Promise((resolve) => { setImmediate(resolve); });
};

test('a held demo upload waits in each phase until let go, or until cancelled', async () => {
  const demo = createDemoBackend('turing88', instant, { hold: true });
  const seen = [];
  demo.onJobProgress((p) => seen.push(p));
  const source = await demo.pickMedia();
  const first = await demo.prepareUpload(KEY, source, 'internal');
  const cancelled = demo.runUpload(first.ticket, false);
  await settle();
  assert.deepEqual(seen.at(-1), { phase: 'convert', done: 2500, total: 20_000 }, 'held after the first step');
  demo.letGo();
  await settle();
  const held = seen.at(-1);
  assert.equal(held.phase, 'upload');
  assert.equal(held.done, Math.round(held.total / 16), 'the upload holds after its first block');
  assert.equal(await demo.cancelJob(), true);
  const result = await cancelled;
  assert.equal(result.status, 'cancelled');
  assert.equal(result.partial, held.done);

  // Lets go given before the phases come are kept for them.
  const second = await demo.prepareUpload(KEY, source, 'internal');
  demo.letGo();
  demo.letGo();
  assert.equal((await demo.runUpload(second.ticket, true)).status, 'done');

  const free = createDemoGate(false);
  await free.hold();
  assert.equal(free.cancel(), false, 'nothing waits');
});

test('demo storage caps each file at the screen\'s limit, before and after converting', async () => {
  const demo = createDemoBackend('turing88', instant);
  const long = await demo.prepareUpload(KEY, 'demo://longo.mp4', 'internal');
  assert.deepEqual([long.code, long.bytes, long.limit], ['tooLarge', 31_457_280, DEMO_REV_C_CAP]);
  const show = await demo.prepareUpload(KEY, 'demo://show.mov', 'internal');
  assert.equal(show.status, 'ready', 'converted, it may fit');
  const result = await demo.runUpload(show.ticket, false);
  assert.deepEqual([result.status, result.code, result.bytes, result.limit], ['refused', 'convertedTooLarge', 27_262_976, DEMO_REV_C_CAP]);
  assert.equal(demo.storageState().files.has('internal/video/show.mp4'), false, 'nothing was sent');
  // A TUR_USB screen takes the vendor's 120 MB.
  const turzx = createDemoBackend('turzx', instant);
  const [screen] = (await turzx.listDevices()).screens;
  assert.equal((await turzx.prepareUpload(screen.key, 'demo://longo.mp4', 'internal')).status, 'ready');
  assert.equal(DEMO_USB_CAP, 120_000_000);
});

test('a file stored with the wrong size fails its check and stays for a delete', async () => {
  const demo = createDemoBackend('turing88', instant);
  const ready = await demo.prepareUpload(KEY, 'demo://torto.png', 'internal');
  await assert.rejects(
    demo.runUpload(ready.ticket, false),
    (e) => e.code === 'sizeMismatch' && e.args.file === 'internal/image/torto.png' && e.args.expected === '256000',
  );
  assert.equal(demo.storageState().files.get('internal/image/torto.png'), 255_990);
});

test('without the udev rule the demo denies the screen and names the fix', async () => {
  const demo = createDemoBackend('denied', instant);
  for (const call of [demo.setLive(true, KEY), demo.setBrightness(KEY, 50), demo.release(KEY), demo.storageOverview(KEY)]) {
    await assert.rejects(call, (e) => e.code === 'accessDenied' && e.args.address === KEY && e.udevCommand.startsWith('sudo install -m 644 '));
  }
  await demo.setLive(false, null);
});

test('a hung demo screen stops live mode and uploads until it is restarted', async () => {
  const demo = createDemoBackend('hung', instant);
  const [screen] = (await demo.listDevices()).screens;
  assert.equal(screen.restartable, true);
  await demo.setLive(true, KEY);
  const stopped = await demo.sample();
  assert.equal(stopped.live, null);
  assert.equal(stopped.liveError.code, 'hung');
  assert.equal(stopped.liveError.args.detail, demoHung().args.detail);
  const ready = await demo.prepareUpload(KEY, 'demo://foto.png', 'internal');
  await assert.rejects(demo.runUpload(ready.ticket, false), (e) => e.code === 'hung');

  // Restarted (not live any more, so it stays off), it works again.
  assert.deepEqual(await demo.restartScreen(KEY), { key: KEY, live: false });
  const again = await demo.prepareUpload(KEY, 'demo://foto.png', 'internal');
  assert.equal((await demo.runUpload(again.ticket, false)).status, 'done');
  await demo.setLive(true, KEY);
  assert.equal((await demo.sample()).live, KEY);
  // A live screen comes back live.
  assert.deepEqual(await demo.restartScreen(KEY), { key: KEY, live: true });
  assert.equal(demo.isLive(), true);
});

test('only screens with a wake chip restart in the demo', async () => {
  const turzx = createDemoBackend('turzx', instant);
  const [screen] = (await turzx.listDevices()).screens;
  assert.equal(screen.restartable, false);
  await assert.rejects(turzx.restartScreen(screen.key), (e) => e.code === 'unsupported' && /unplug the screen/.test(e.args.detail));
  await assert.rejects(turzx.restartScreen('COM9'), (e) => e.code === 'screenNotFound');
  const denied = createDemoBackend('denied', instant);
  await assert.rejects(denied.restartScreen(KEY), (e) => e.code === 'accessDenied');
  const two = createDemoBackend('two', instant);
  assert.equal((await two.restartScreen('COM3')).live, false, 'an asleep rev C screen restarts too');
});

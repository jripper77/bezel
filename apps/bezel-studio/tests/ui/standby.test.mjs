// "When the computer shuts down" (D-2026-10-03-power-off-standby-2, -4, -6):
// the logic (what each option offers and what activating it does, the
// request, what the confirmation says is written, the videos and the album,
// the shape a photo is framed in and its name on the card), the demo (the
// same answers and refusals as the studio's commands; nothing written or
// recorded without the confirmation; `keep` while it is the choice writes
// nothing) and the bridge (each call to its command, with its arguments).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { LOCALES, translator } from '../../src/i18n/index.js';
import { createBridge } from '../../src/bridge.js';
import { createDemoBackend } from '../../src/demo-backend.js';
import { DEMO_PICKED_PHOTO } from '../../src/demo-data.js';
import { DEMO_START_MODES, demoAlbumPicture, demoBootStartMode, demoPhotoBox, demoPlanB } from '../../src/demo-standby.js';
import {
  ALBUM, CHOICES, PHOTO_FITS, REASONS, SLEEP, activation, albumClash, albumName, albumPath, albumPhotos, complete, confirmationOf, createAnswers,
  currentDetail, dialogDefaults, inTurn, minutesText, offered, optionOf, requestOf, screenShape, sleepChoices, sleepMinutesOf, suggestPhotoName,
  translate, videoGroups,
} from '../../src/standby.js';

const pt = translator('pt-BR');
const en = translator('en');
const SCREEN = '/dev/ttyACM1';

const video = (path, size = 1000) => {
  const [medium, kind, name] = path.split('/');
  return { path, medium, kind, name, size };
};

/** An overview whose options are all offered but those in `refused` (choice → reason). */
function overviewOf(choice = 'keep', refused = {}, extra = {}) {
  return {
    choice,
    sleepMinutes: null,
    file: null,
    options: CHOICES.map((c) => ({ choice: c, enabled: !refused[c], reason: refused[c] ?? null })),
    videos: [video('internal/video/amd_90.mp4'), video('sd/video/chuva.mp4')],
    card: true,
    orientation: 'landscape',
    ...extra,
  };
}

/** Every key a message (and the messages in its params) names. */
const keysOf = (message) => [message.key, ...Object.values(message.params ?? {}).filter((v) => v?.key).flatMap(keysOf)];
const isKey = (key) => Object.values(LOCALES).every((table) => Object.hasOwn(table, key));

// --------------------------------------------------------------- logic --

test('four choices, keep first; an option the answer lacks is refused as unsupported', () => {
  assert.deepEqual(CHOICES, ['keep', 'off', 'video', 'album']);
  assert.deepEqual(REASONS, ['notConnected', 'unsupported', 'noCard', 'noVideo']);
  const data = overviewOf('keep', { album: 'noCard' });
  assert.deepEqual(optionOf(data, 'off'), { choice: 'off', enabled: true, reason: null });
  assert.deepEqual(optionOf(data, 'album'), { choice: 'album', enabled: false, reason: 'noCard' });
  assert.deepEqual(optionOf({ options: [{ choice: 'off', enabled: false, reason: null }] }, 'off'), { choice: 'off', enabled: false, reason: 'unsupported' });
  assert.deepEqual(optionOf(null, 'keep'), { choice: 'keep', enabled: false, reason: 'unsupported' });
  for (const reason of REASONS) assert.ok(isKey(`standby.reason.${reason}`), reason);
  for (const choice of CHOICES) {
    for (const key of [`standby.choice.${choice}`, `standby.explain.${choice}`]) assert.ok(isKey(key), key);
  }
});

test('a family that keeps no choice does not show it; an asleep screen does', () => {
  const unsupported = Object.fromEntries(CHOICES.map((c) => [c, 'unsupported']));
  assert.equal(offered(overviewOf('keep', unsupported)), false);
  assert.equal(offered(null), false);
  const asleep = Object.fromEntries(CHOICES.map((c) => [c, 'notConnected']));
  assert.equal(offered(overviewOf('off', asleep)), true);
  assert.equal(offered(overviewOf()), true);
});

test('activating an option: refused, nothing (keep to keep), the album manager, or the confirmation', () => {
  const data = overviewOf('keep', { album: 'noCard' });
  assert.equal(activation(data, 'album'), 'refused');
  assert.equal(activation(data, 'keep'), 'nothing', 'keep to keep sends nothing');
  assert.equal(activation(data, 'off'), 'ask');
  assert.equal(activation(data, 'video'), 'ask');
  const album = overviewOf('album');
  assert.equal(activation(album, 'album'), 'manage', 'the album as the choice: its photos, nothing written');
  assert.equal(activation(album, 'keep'), 'ask', 'keep undoes another choice, after the confirmation');
  assert.equal(activation(overviewOf('off', {}, { sleepMinutes: 5 }), 'off'), 'ask', 'off again: other minutes');
  assert.equal(activation(overviewOf('video', {}, { file: 'sd/video/chuva.mp4' }), 'video'), 'ask', 'video again: another video');
});

test('a write for a screen no longer shown leaves the reading of the shown one to be drawn', () => {
  // Review W2 of iteration 2: two rev C screens. A is shown and read; its
  // confirmation opens; B is chosen at the top and read; then A's is answered.
  const answers = createAnswers();
  const ofA = answers.reading();
  const ofB = answers.reading();
  assert.equal(answers.newest(ofA), false, "A's reading is not drawn on B");
  const forA = answers.writing('A', 'B');
  assert.equal(answers.newest(ofB), true, "A's write left B's section loading");
  assert.equal(answers.newest(forA), false, "A's answer is drawn on B");

  // A write for the screen shown outdates its reading in flight; a reading
  // that starts after it is newer still.
  const before = answers.reading();
  const forB = answers.writing('B', 'B');
  assert.equal(answers.newest(before), false);
  assert.equal(answers.newest(forB), true);
  const after = answers.reading();
  assert.equal(answers.newest(forB), false);
  assert.equal(answers.newest(after), true);
});

test('a write whose screen is shown again before its answer reads that screen again', () => {
  // Review W3 of iteration 3: A's confirmation is answered while B is shown;
  // A is chosen again at the top before the write's answer, and its reading
  // may read the catalog before the write saves it.
  const answers = createAnswers();
  answers.reading();
  const forA = answers.writing('A', 'B');
  assert.equal(forA, null);
  const ofA = answers.reading();
  assert.equal(answers.written(forA, 'A', 'A'), 'read', 'A keeps its choice from before the write');
  assert.equal(answers.written(forA, 'A', 'B'), 'none', "A's answer is drawn on B");
  assert.equal(answers.newest(ofA), true, 'the reading of the screen shown is not outdated by the answer');

  // A write for the screen shown and no reading since: its answer is drawn.
  const forB = answers.writing('B', 'B');
  assert.equal(answers.written(forB, 'B', 'B'), 'draw');
  // B chosen again while it runs (A, then B, at the top): read again.
  answers.reading();
  answers.reading();
  assert.equal(answers.written(forB, 'B', 'B'), 'read');
});

test('the sleep timer takes 1 to 10 whole minutes, 5 suggested', () => {
  assert.deepEqual(SLEEP, { min: 1, max: 10, suggested: 5 });
  assert.deepEqual(sleepChoices(), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
  assert.equal(sleepMinutesOf(3), 3);
  assert.equal(sleepMinutesOf('7'), 7);
  for (const wrong of [0, 11, 2.5, null, undefined, 'x']) assert.equal(sleepMinutesOf(wrong), null, String(wrong));
  assert.equal(translate(en, minutesText(1)), '1 minute');
  assert.equal(translate(pt, minutesText(5)), '5 minutos');
});

test('a dialog starts with the choice as recorded, else the suggestion and the first video', () => {
  const data = overviewOf('keep');
  assert.deepEqual(dialogDefaults(data, 'off'), { sleepMinutes: 5, file: 'internal/video/amd_90.mp4' });
  assert.deepEqual(dialogDefaults(overviewOf('off', {}, { sleepMinutes: 8 }), 'off').sleepMinutes, 8);
  assert.equal(dialogDefaults(overviewOf('video', {}, { file: 'sd/video/chuva.mp4' }), 'video').file, 'sd/video/chuva.mp4');
  assert.equal(dialogDefaults(overviewOf('video', {}, { file: 'sd/video/gone.mp4' }), 'video').file, 'internal/video/amd_90.mp4', 'a video no longer stored');
  assert.deepEqual(dialogDefaults(overviewOf('keep', {}, { videos: [] }), 'video'), { sleepMinutes: 5, file: null });
  assert.deepEqual(dialogDefaults(null, 'off'), { sleepMinutes: 5, file: null });
});

test('the request carries the minutes only with off and the video only with video', () => {
  const values = { sleepMinutes: 3, file: 'sd/video/chuva.mp4' };
  assert.deepEqual(requestOf('off', values), { choice: 'off', sleepMinutes: 3, file: null });
  assert.deepEqual(requestOf('video', values), { choice: 'video', sleepMinutes: null, file: 'sd/video/chuva.mp4' });
  assert.deepEqual(requestOf('album', values), { choice: 'album', sleepMinutes: null, file: null });
  assert.deepEqual(requestOf('keep'), { choice: 'keep', sleepMinutes: null, file: null });
  assert.equal(complete(requestOf('off', { sleepMinutes: 12 })), false);
  assert.equal(complete(requestOf('video', {})), false);
  assert.equal(complete({ choice: 'nap' }), false);
  for (const choice of CHOICES) assert.equal(complete(requestOf(choice, values)), true, choice);
});

test('the confirmation says what happens at shutdown and every setting written to the screen', () => {
  const said = (t, request) => {
    const { atShutdown, written } = confirmationOf(request);
    return [translate(t, atShutdown), ...written.map((line) => translate(t, line))];
  };
  assert.deepEqual(said(en, requestOf('off', { sleepMinutes: 5 })), [
    'Bezel turns the screen off.',
    'At power-up: its boot media, as set under Storage',
    'Sleep timer: 5 minutes without the computer',
    'Brightness: the level it has now',
  ]);
  assert.deepEqual(said(pt, requestOf('keep')), [
    'Nada é enviado: a tela continua mostrando o que mostra.',
    'Ao ligar: a mídia de inicialização dela, como definida em Armazenamento',
    'Temporizador de repouso: desligado',
    'Brilho: o nível que ela tem agora',
  ]);
  const [, startVideo, sleepVideo] = said(en, requestOf('video', { file: 'sd/video/chuva.mp4' }));
  assert.equal(said(en, requestOf('video', { file: 'sd/video/chuva.mp4' }))[0], 'The screen plays “chuva.mp4” in a loop.');
  assert.match(startVideo, /first video of the video folder of the card/);
  assert.equal(sleepVideo, 'Sleep timer: off');
  assert.deepEqual(said(pt, requestOf('album')).slice(0, 2), ['A tela reinicia no álbum de fotos.', 'Ao ligar: o álbum de fotos do cartão']);
  // Every key it can name is in both dictionaries.
  for (const choice of CHOICES) {
    const { atShutdown, written } = confirmationOf(requestOf(choice, { sleepMinutes: 1, file: 'internal/video/a.mp4' }));
    for (const key of [atShutdown, ...written].flatMap(keysOf)) assert.ok(isKey(key), key);
  }
});

test('the current choice shows its minutes or its video under its label', () => {
  assert.equal(translate(en, currentDetail(overviewOf('off', {}, { sleepMinutes: 1 }), 'off')), 'Sleep timer: 1 minute');
  assert.equal(translate(pt, currentDetail(overviewOf('video', {}, { file: 'sd/video/chuva.mp4' }), 'video')), 'chuva.mp4, no cartão SD');
  assert.equal(translate(en, currentDetail(overviewOf('video', {}, { file: 'internal/video/a.mp4' }), 'video')), 'a.mp4, in the internal memory');
  assert.equal(currentDetail(overviewOf('keep'), 'off'), null, 'not the choice');
  assert.equal(currentDetail(overviewOf('keep'), 'keep'), null);
  assert.equal(currentDetail(overviewOf('album'), 'album'), null);
  assert.equal(currentDetail(overviewOf('video', {}, { file: null }), 'video'), null);
});

test('translate fills a param that is a message itself', () => {
  const t = (key, params = {}) => `${key}(${Object.entries(params).map(([k, v]) => `${k}=${v}`).join(',')})`;
  assert.equal(translate(t, { key: 'a', params: { n: { key: 'b', params: { c: 2 } }, s: 'x' } }), 'a(n=b(c=2),s=x)');
  assert.equal(translate(t, { key: 'a' }), 'a()');
});

test('videos are grouped by medium, internal memory first, empty groups left out', () => {
  const videos = [video('sd/video/b.mp4'), video('internal/video/a.mp4'), video('sd/video/c.mp4')];
  assert.deepEqual(videoGroups(videos).map((g) => [g.medium, g.files.map((f) => f.name)]), [['internal', ['a.mp4']], ['sd', ['b.mp4', 'c.mp4']]]);
  assert.deepEqual(videoGroups([video('sd/video/b.mp4')]).map((g) => g.medium), ['sd']);
  assert.deepEqual(videoGroups(null), []);
});

test('the album is the card image files of the storage manager overview, by name', () => {
  // The manager overview (`manager_overview`): its listing is what `manager_thumbnail` answers for.
  const overview = {
    files: [video('internal/image/logo.png'), video('sd/image/praia.png'), video('sd/video/chuva.mp4'), video('sd/image/img_0042.jpg')],
  };
  assert.deepEqual(ALBUM, { medium: 'sd', kind: 'image' });
  assert.deepEqual(albumPhotos(overview).map((p) => p.name), ['img_0042.jpg', 'praia.png']);
  assert.deepEqual(albumPhotos({ files: [video('internal/image/logo.png')] }), [], 'no card');
  assert.deepEqual(albumPhotos(null), []);
  assert.equal(albumPath('praia.png'), 'sd/image/praia.png');
  assert.equal(albumClash(albumPhotos(overview), 'PRAIA.png')?.path, 'sd/image/praia.png', 'the same name but for letter case');
  assert.equal(albumClash(albumPhotos(overview), 'mar.png'), null);
});

test('a photo is framed in the shape the screen stands in: horizontal and vertical alike', () => {
  const panel = { width: 480, height: 1920 };
  assert.deepEqual(screenShape(panel, 'landscape'), { width: 1920, height: 480, axis: 'horizontal' });
  assert.deepEqual(screenShape(panel, 'reverse-landscape'), { width: 1920, height: 480, axis: 'horizontal' });
  assert.deepEqual(screenShape(panel, 'portrait'), { width: 480, height: 1920, axis: 'vertical' });
  assert.deepEqual(screenShape(panel, 'reverse-portrait'), { width: 480, height: 1920, axis: 'vertical' });
  assert.deepEqual(screenShape({ width: 1920, height: 480 }, 'portrait'), { width: 480, height: 1920, axis: 'vertical' });
  assert.deepEqual(screenShape({ width: 480, height: 480 }, 'landscape'), { width: 480, height: 480, axis: 'horizontal' });
  assert.deepEqual(PHOTO_FITS, ['cover', 'contain'], 'Fill is the default');
});

test('a photo gets a name the card takes, as a PNG; a typed name is checked like a rename', () => {
  assert.equal(suggestPhotoName('/home/me/Imagens/Praia do Forte.JPG'), 'praia_do_forte.png');
  assert.equal(suggestPhotoName('C:\\Fotos\\Férias 2026.jpeg'), 'f_rias_2026.png');
  assert.equal(suggestPhotoName('/x/IMG-0042.heic.jpg'), 'img-0042_heic.png');
  assert.equal(suggestPhotoName('/x/...'), 'photo.png');
  assert.equal(suggestPhotoName('/x/.hidden'), 'hidden.png');
  assert.deepEqual(albumName(' Praia.PNG '), { name: 'praia.png', problem: null });
  assert.deepEqual(albumName('praia.jpg').problem, { code: 'extensionChanged', args: { expected: 'png' } });
  assert.deepEqual(albumName('férias.png').problem, { code: 'invalidName', args: { char: 'é' } });
  assert.deepEqual(albumName('.png').problem, { code: 'invalidName', args: {} });
  assert.deepEqual(albumName('').problem, { code: 'invalidName', args: {} });
});

test('a suggested name keeps the runs of [a-z0-9-] joined by one `_`, none at its ends, and the card takes it', () => {
  // The rule, written another way (the suggestion trimmed with `/^_+|_+$/g`
  // before Sonar S8786): the stem in lower case, every run of anything but
  // `[a-z0-9-]` (`_` included) one `_`, none leading or trailing.
  const rule = (source) => {
    const file = source.split('/').pop();
    const dot = file.lastIndexOf('.');
    const stem = (dot > 0 ? file.slice(0, dot) : file).replace(/[A-Z]+/g, (c) => c.toLowerCase());
    return `${stem.split(/[^a-z0-9-]+/).filter(Boolean).join('_') || 'photo'}.png`;
  };
  assert.equal(suggestPhotoName('/x/__Praia__.jpg'), 'praia.png');
  assert.equal(suggestPhotoName('/x/_ (Praia) _.jpg'), 'praia.png');
  assert.equal(suggestPhotoName('/x/a__ _b.jpg'), 'a_b.png');
  assert.equal(suggestPhotoName('/x/-a-.jpg'), '-a-.png');
  assert.equal(suggestPhotoName('/x/_.jpg'), 'photo.png');
  assert.equal(suggestPhotoName('/x/___'), 'photo.png');
  // Every name of up to 5 characters, kept, folded or replaced ones, with and without an extension.
  const alphabet = ['a', 'Z', '0', '_', '-', '.', ' ', 'é', '\u{1F600}'];
  let level = [''];
  let names = level;
  for (let length = 1; length <= 5; length += 1) {
    level = level.flatMap((name) => alphabet.map((c) => name + c));
    names = names.concat(level);
  }
  const differ = [];
  const refused = [];
  for (const source of names.flatMap((name) => [`/x/${name}`, `/x/${name}.jpg`])) {
    const suggested = suggestPhotoName(source);
    if (suggested !== rule(source)) differ.push(source);
    if (albumName(suggested).problem !== null) refused.push(source);
  }
  assert.deepEqual(differ, [], 'the rule, for every one');
  assert.deepEqual(refused, [], 'lower case, only [a-z0-9_.-], no leading dot, .png');
});

test('inTurn runs each step once the one before settled; a failure stops the rest', async () => {
  const next = () => new Promise((resolve) => { setImmediate(resolve); });
  const log = [];
  const ends = [];
  const step = (item) => new Promise((resolve) => {
    log.push(`start ${item}`);
    ends.push(() => {
      log.push(`end ${item}`);
      resolve();
    });
  });
  const done = inTurn(new Set(['a', 'b', 'c']), step);
  await next();
  assert.deepEqual(log, ['start a'], 'b waits for a');
  ends.shift()();
  await next();
  assert.deepEqual(log, ['start a', 'end a', 'start b']);
  ends.shift()();
  await next();
  ends.shift()();
  assert.equal(await done, undefined);
  assert.deepEqual(log, ['start a', 'end a', 'start b', 'end b', 'start c', 'end c']);
  const ran = [];
  await assert.rejects(inTurn([1, 2, 3], async (n) => {
    ran.push(n);
    if (n === 2) throw new Error('no thumbnail');
  }), /no thumbnail/);
  assert.deepEqual(ran, [1, 2], 'nothing after the failure');
  assert.equal(await inTurn([], step), undefined);
});

// ---------------------------------------------------------------- demo --

/** A demo backend in `scenario`, with every plan B written kept. */
function demoIn(scenario) {
  const writes = [];
  const demo = createDemoBackend(scenario, { delay: async () => {} }, { onStandby: (all) => writes.splice(0, writes.length, ...all) });
  return { demo, writes };
}

test('the demo overview: the choice, the options, the videos, the card and how the screen stands', async () => {
  const { demo } = demoIn('turing88');
  const data = await demo.standbyOverview(SCREEN);
  assert.equal(data.choice, 'keep');
  assert.deepEqual([data.sleepMinutes, data.file], [null, null]);
  assert.deepEqual(data.options, CHOICES.map((choice) => ({ choice, enabled: true, reason: null })));
  assert.deepEqual(data.videos.map((v) => v.path), ['internal/video/amd_90.mp4', 'sd/video/chuva.mp4']);
  assert.deepEqual(Object.keys(data.videos[0]).sort(), ['kind', 'medium', 'name', 'path', 'size']);
  assert.equal(data.card, true);
  assert.equal(data.orientation, 'landscape', 'the 8.8" stands horizontally unless it was used otherwise');
  // A new vertical theme for the screen: it now stands vertically.
  await demo.newTheme(SCREEN, 'Novo', 'portrait');
  assert.equal((await demo.standbyOverview(SCREEN)).orientation, 'portrait');
  await assert.rejects(demo.standbyOverview('nope'), (e) => e.code === 'screenNotFound');
});

test('the demo says why an option is not offered: no card, no video, asleep, another family, a denied port', async () => {
  const reasons = (data) => Object.fromEntries(data.options.map((o) => [o.choice, o.reason]));
  const noCard = await demoIn('noCard').demo.standbyOverview(SCREEN);
  assert.deepEqual(reasons(noCard), { keep: null, off: null, video: null, album: 'noCard' });
  assert.equal(noCard.card, false);
  assert.deepEqual(noCard.videos.map((v) => v.path), ['internal/video/amd_90.mp4'], 'no card: only the internal videos');
  // The asleep 2.1" of `two`: nothing can change until it wakes.
  const asleep = await demoIn('two').demo.standbyOverview('COM3');
  assert.deepEqual(reasons(asleep), { keep: 'notConnected', off: 'notConnected', video: 'notConnected', album: 'notConnected' });
  assert.deepEqual([asleep.videos, asleep.card, asleep.orientation], [[], false, 'portrait']);
  const usb = await demoIn('turzx').demo.standbyOverview('3-1.4');
  assert.deepEqual(Object.values(reasons(usb)), ['unsupported', 'unsupported', 'unsupported', 'unsupported']);
  assert.equal(offered(usb), false);
  // An 8.8" that stores no video.
  const { demo } = demoIn('turing88');
  await demo.deleteStored(SCREEN, 'internal/video/amd_90.mp4', true);
  await demo.deleteStored(SCREEN, 'sd/video/chuva.mp4', true);
  assert.equal(reasons(await demo.standbyOverview(SCREEN)).video, 'noVideo');
  await assert.rejects(demoIn('denied').demo.standbyOverview(SCREEN), (e) => e.code === 'accessDenied');
});

test('the demo writes nothing and records nothing without the confirmation', async () => {
  const { demo, writes } = demoIn('turing88');
  await assert.rejects(demo.setStandby(SCREEN, requestOf('off', { sleepMinutes: 5 }), false), (e) => e.code === 'notConfirmed');
  await assert.rejects(demo.setStandby(SCREEN, requestOf('album'), undefined), (e) => e.code === 'notConfirmed');
  assert.deepEqual(writes, []);
  assert.deepEqual(demo.standbyWrites(), []);
  assert.equal((await demo.standbyOverview(SCREEN)).choice, 'keep', 'the catalog is as it was');
});

test('the demo writes each choice as its plan B, and keep undoes it', async () => {
  const { demo, writes } = demoIn('turing88');
  let data = await demo.setStandby(SCREEN, requestOf('off', { sleepMinutes: 3 }), true);
  assert.deepEqual([data.choice, data.sleepMinutes, data.file], ['off', 3, null]);
  assert.deepEqual(writes.at(-1), { screen: SCREEN, choice: 'off', sleepMinutes: 3, file: null, startMode: DEMO_START_MODES.default });
  data = await demo.setStandby(SCREEN, requestOf('video', { file: 'sd/video/chuva.mp4' }), true);
  assert.deepEqual([data.choice, data.sleepMinutes, data.file], ['video', null, 'sd/video/chuva.mp4']);
  assert.deepEqual(writes.at(-1), { screen: SCREEN, choice: 'video', sleepMinutes: 0, file: 'sd/video/chuva.mp4', startMode: DEMO_START_MODES.video });
  data = await demo.setStandby(SCREEN, requestOf('album'), true);
  assert.deepEqual(writes.at(-1), { screen: SCREEN, choice: 'album', sleepMinutes: 0, file: null, startMode: DEMO_START_MODES.image });
  // Keep undoes: the boot media's start mode (an image here), no timer.
  await demo.setBootMedia(SCREEN, 'internal/image/logo.png', true);
  data = await demo.setStandby(SCREEN, requestOf('keep'), true);
  assert.equal(data.choice, 'keep');
  assert.deepEqual(writes.at(-1), { screen: SCREEN, choice: 'keep', sleepMinutes: 0, file: null, startMode: DEMO_START_MODES.image });
  // Keep to keep: nothing is written.
  await demo.setStandby(SCREEN, requestOf('keep'), true);
  assert.equal(writes.length, 4);
  assert.deepEqual(demo.standbyWrites(), writes);
});

test('the demo refuses what the screen cannot take, before writing', async () => {
  const { demo, writes } = demoIn('turing88');
  await assert.rejects(demo.setStandby(SCREEN, requestOf('off', { sleepMinutes: 0 }), true), (e) => e.code === 'invalidInput');
  await assert.rejects(demo.setStandby(SCREEN, { choice: 'off', sleepMinutes: 11 }, true), (e) => e.code === 'invalidInput');
  await assert.rejects(demo.setStandby(SCREEN, requestOf('video', { file: 'sd/video/gone.mp4' }), true), (e) => e.code === 'invalidInput');
  await assert.rejects(demo.setStandby(SCREEN, { choice: 'nap' }, true), (e) => e.code === 'invalidInput' && /nap/.test(e.args.detail));
  await assert.rejects(demo.setStandby('nope', requestOf('keep'), true), (e) => e.code === 'screenNotFound');
  await assert.rejects(demoIn('noCard').demo.setStandby(SCREEN, requestOf('album'), true), (e) => e.code === 'unsupported');
  await assert.rejects(demoIn('two').demo.setStandby('COM3', requestOf('off', { sleepMinutes: 5 }), true), (e) => e.code === 'unsupported' && /asleep/.test(e.args.detail));
  await assert.rejects(demoIn('turzx').demo.setStandby('3-1.4', requestOf('off', { sleepMinutes: 5 }), true), (e) => e.code === 'unsupported');
  await assert.rejects(demoIn('denied').demo.setStandby(SCREEN, requestOf('keep'), true), (e) => e.code === 'accessDenied');
  assert.deepEqual(writes, []);
});

test('the demo keeps the choice per model, as the shared catalog does; the scenarios start with theirs', async () => {
  const off = await demoIn('standbyOff').demo.standbyOverview(SCREEN);
  assert.deepEqual([off.choice, off.sleepMinutes], ['off', 5]);
  const album = await demoIn('album').demo.standbyOverview(SCREEN);
  assert.equal(album.choice, 'album');
  // Another family never takes a scenario's choice.
  assert.equal((await demoIn('turzx').demo.standbyOverview('3-1.4')).choice, 'keep');
});

test('the demo plan B: the boot media decides the start mode of keep and off', () => {
  assert.equal(demoBootStartMode(null), 0);
  assert.equal(demoBootStartMode('internal/image/logo.png'), 1);
  assert.equal(demoBootStartMode('sd/video/chuva.mp4'), 2);
  assert.deepEqual(demoPlanB(requestOf('off', { sleepMinutes: 7 }), 'internal/video/a.mp4'), { startMode: 2, sleepMinutes: 7 });
  assert.deepEqual(demoPlanB(requestOf('keep'), null), { startMode: 0, sleepMinutes: 0 });
  assert.deepEqual(demoPlanB(requestOf('album'), 'internal/video/a.mp4'), { startMode: 1, sleepMinutes: 0 });
  assert.deepEqual(demoPlanB(requestOf('video', { file: 'x' }), 'internal/image/a.png'), { startMode: 2, sleepMinutes: 0 });
});

test('the demo frames a photo: Fill covers the shape, Fit shows it whole', () => {
  const phone = { width: 3024, height: 4032 };
  const wide = { width: 1920, height: 480 };
  const cover = demoPhotoBox(phone, wide, 'cover');
  assert.equal(cover.width, 1920);
  assert.ok(cover.height > 480 && cover.y < 0, 'its top and bottom are cut');
  const contain = demoPhotoBox(phone, wide, 'contain');
  assert.equal(contain.height, 480);
  assert.ok(contain.width < 1920 && contain.x > 0, 'black on both sides');
  const picture = demoAlbumPicture(phone, { width: 480, height: 1920 }, 'contain');
  assert.match(decodeURIComponent(picture), /^data:image\/svg\+xml,<svg [^>]*width="120" height="480"/);
});

test('the demo previews a picked photo in the shape the screen stands in', async () => {
  const { demo } = demoIn('turing88');
  const photo = await demo.pickPhoto();
  assert.equal(photo, DEMO_PICKED_PHOTO);
  const size = (url) => decodeURIComponent(url).match(/width="(\d+)" height="(\d+)"/).slice(1).map(Number);
  const [w, h] = size(await demo.albumPreview(SCREEN, photo, 'cover'));
  assert.ok(w > h, 'horizontal');
  await demo.newTheme(SCREEN, 'Novo', 'portrait');
  const [w2, h2] = size(await demo.albumPreview(SCREEN, photo, 'contain'));
  assert.ok(h2 > w2, 'vertical');
  await assert.rejects(demo.albumPreview(SCREEN, '/home/demo/nope.jpg', 'cover'), (e) => e.code === 'fileError');
  await assert.rejects(demo.albumPreview(SCREEN, photo, 'stretch'), (e) => e.code === 'invalidInput');
  await assert.rejects(demo.albumPreview('nope', photo, 'cover'), (e) => e.code === 'screenNotFound');
});

test('the demo adds a photo to the card album only when confirmed, cataloged with its thumbnail', async () => {
  const { demo } = demoIn('turing88');
  const photo = await demo.pickPhoto();
  await assert.rejects(demo.albumAdd(SCREEN, photo, 'cover', 'praia.png', false), (e) => e.code === 'notConfirmed');
  const before = await demo.managerOverview(SCREEN);
  assert.deepEqual(albumPhotos(before), []);
  const added = await demo.albumAdd(SCREEN, photo, 'cover', 'praia.png', true);
  assert.deepEqual(added, { path: 'sd/image/praia.png', bytes: Math.round(480 * 1920 * 1.2) });
  assert.equal(await demo.managerThumbnail(SCREEN, 'sd/image/praia.png'), null, 'not listed yet');
  const after = albumPhotos(await demo.managerOverview(SCREEN));
  assert.deepEqual(after.map((p) => [p.path, p.size]), [['sd/image/praia.png', added.bytes]]);
  assert.match(await demo.managerThumbnail(SCREEN, 'sd/image/praia.png'), /^data:image\/svg\+xml,/);
  // The same name again needs the confirmation of replacing it (D-2026-10-03-power-off-standby-4 (3)):
  // without it nothing is sent, whatever the window knew.
  await assert.rejects(
    demo.albumAdd(SCREEN, photo, 'contain', 'praia.png', true),
    (e) => e.code === 'notConfirmed' && e.args.detail === 'replacing sd/image/praia.png',
  );
  await assert.rejects(demo.albumAdd(SCREEN, photo, 'contain', 'praia.png', true, false), (e) => e.code === 'notConfirmed');
  await demo.albumAdd(SCREEN, photo, 'contain', 'praia.png', true, true);
  assert.equal(albumPhotos(await demo.managerOverview(SCREEN)).length, 1);
  // Removing is the confirmed delete of today.
  await demo.deleteStored(SCREEN, 'sd/image/praia.png', true);
  assert.deepEqual(albumPhotos(await demo.managerOverview(SCREEN)), []);
});

test('the demo refuses an album photo it cannot send, before sending', async () => {
  const { demo } = demoIn('turing88');
  const photo = await demo.pickPhoto();
  await assert.rejects(demo.albumAdd(SCREEN, photo, 'cover', 'praia.jpg', true), (e) => e.code === 'invalidInput');
  await assert.rejects(demo.albumAdd(SCREEN, photo, 'cover', null, true), (e) => e.code === 'invalidInput');
  await assert.rejects(demo.albumAdd(SCREEN, '/home/demo/nope.jpg', 'cover', 'x.png', true), (e) => e.code === 'fileError');
  await assert.rejects(demoIn('noCard').demo.albumAdd(SCREEN, photo, 'cover', 'x.png', true), (e) => e.code === 'unsupported' && /SD card/.test(e.args.detail));
  await assert.rejects(demoIn('turzx').demo.albumAdd('3-1.4', photo, 'cover', 'x.png', true), (e) => e.code === 'unsupported');
  assert.deepEqual(albumPhotos(await demo.managerOverview(SCREEN)), []);
});

test('the album scenario has a photo Bezel sent and one it did not; thumbnails follow the manager overview', async () => {
  const { demo } = demoIn('album');
  // Like the studio, a thumbnail only comes for a file the last manager overview of the screen listed:
  // the storage tab's listing is not enough.
  await demo.storageOverview(SCREEN);
  assert.equal(await demo.managerThumbnail(SCREEN, 'sd/image/praia.png'), null, 'the storage listing does not count');
  const photos = albumPhotos(await demo.managerOverview(SCREEN));
  assert.deepEqual(photos.map((p) => p.name), ['img_0042.jpg', 'praia.png']);
  assert.match(await demo.managerThumbnail(SCREEN, 'sd/image/praia.png'), /^data:/);
  assert.equal(await demo.managerThumbnail(SCREEN, 'sd/image/img_0042.jpg'), null, 'the vendor app put it there: by its name');
  assert.equal(await demo.managerThumbnail('/dev/ttyACM9', 'sd/image/praia.png'), null, 'another screen was not listed');
});

// -------------------------------------------------------------- bridge --

test('tauri mode maps the standby calls to their commands, with their arguments', async () => {
  const calls = [];
  const invoke = async (cmd, args) => {
    calls.push([cmd, args]);
    return null;
  };
  const bridge = createBridge({ location: { hostname: 'localhost', search: '' }, __TAURI__: { core: { invoke } } });
  await bridge.standbyOverview('k');
  await bridge.setStandby('k', requestOf('off', { sleepMinutes: 5 }), true);
  await bridge.setStandby('k', { choice: 'video', file: 'sd/video/chuva.mp4' }, false);
  await bridge.setStandby('k', { choice: 'keep' }, true);
  await bridge.pickPhoto();
  await bridge.albumPreview('k', '/home/me/praia.jpg', 'contain');
  await bridge.albumAdd('k', '/home/me/praia.jpg', 'cover', 'praia.png', true);
  await bridge.albumAdd('k', '/home/me/praia.jpg', 'contain', 'praia.png', true, true);
  assert.deepEqual(calls, [
    ['standby_overview', { screen: 'k' }],
    ['set_standby', { screen: 'k', choice: 'off', sleepMinutes: 5, file: null, confirmed: true }],
    ['set_standby', { screen: 'k', choice: 'video', sleepMinutes: null, file: 'sd/video/chuva.mp4', confirmed: false }],
    ['set_standby', { screen: 'k', choice: 'keep', sleepMinutes: null, file: null, confirmed: true }],
    ['pick_photo', undefined],
    ['album_preview', { screen: 'k', source: '/home/me/praia.jpg', fit: 'contain' }],
    ['album_add', { screen: 'k', source: '/home/me/praia.jpg', fit: 'cover', name: 'praia.png', confirmed: true, replace: false }],
    ['album_add', { screen: 'k', source: '/home/me/praia.jpg', fit: 'contain', name: 'praia.png', confirmed: true, replace: true }],
  ]);
});

test('demo mode shows on the page every plan B written', async () => {
  const attributes = {};
  const bridge = createBridge({ location: { hostname: 'localhost', search: '?demo=turing88' }, document: { documentElement: { setAttribute: (k, v) => { attributes[k] = v; } } } });
  assert.equal(bridge.mode, 'demo');
  await bridge.standbyOverview(SCREEN);
  assert.equal(attributes['data-demo-standby'], undefined, 'reading writes nothing');
  await bridge.setStandby(SCREEN, requestOf('off', { sleepMinutes: 2 }), true);
  assert.deepEqual(JSON.parse(attributes['data-demo-standby']), [{ screen: SCREEN, choice: 'off', sleepMinutes: 2, file: null, startMode: 0 }]);
});

// A video background from the studio: what the Media panel lists, the
// background an asset makes, what the inspector says about the screen, the
// undo of a new background, and the demo backend adding videos and GIFs.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  backgroundOf, droppable, extensionOf, fileNameOf, formatDuration, mediaItems, moves, playsVideos, videoFacts, videoStatus,
} from '../../src/editor/background.js';
import { createStore } from '../../src/editor/store.js';
import { createDemoBackend, demoAssetName } from '../../src/demo-backend.js';
import { DEMO_POSTER_URL, SCENARIOS } from '../../src/demo-data.js';
import { DEMO_THEME } from '../../src/demo-theme.js';

const clip = { ref: 'assets/ferias.mp4', kind: 'video', poster: 'assets/ferias-poster.png', bytes: 24_117_248, durationMs: 12_400 };
const poster = { ref: 'assets/ferias-poster.png', kind: 'image', dataUrl: 'data:x' };
const gif = { ref: 'assets/ondas.gif', kind: 'image', animated: true, poster: null, bytes: 3_145_728, durationMs: 2_400 };
const photo = { ref: 'assets/foto.png', kind: 'image', dataUrl: 'data:y' };
const font = { ref: 'assets/inter.ttf', kind: 'font' };

test('files are told apart by their extension', () => {
  assert.equal(extensionOf('C:\\Clips\\a.b\\Ondas.MOV'), 'mov');
  assert.equal(extensionOf('/home/me/.hidden'), '');
  assert.equal(extensionOf('noext'), '');
  for (const name of ['a.mp4', 'b.MKV', 'c.webm', 'd.gif', 'e.png', 'f.JPEG']) assert.ok(droppable(name), name);
  for (const name of ['song.mp3', 'notes.txt', 'theme.turtheme', 'x.bmp']) assert.ok(!droppable(name), name);
  assert.equal(fileNameOf('assets/ferias.mp4'), 'ferias.mp4');
  assert.equal(fileNameOf('C:\\Clips\\Ondas.gif'), 'Ondas.gif');
});

test('videos and animated GIFs make a video background with their poster', () => {
  assert.ok(moves(clip) && moves(gif) && moves({ kind: 'video' }));
  assert.ok(!moves(photo) && !moves(null));
  assert.deepEqual(backgroundOf(clip), { type: 'video', asset: 'assets/ferias.mp4', poster: 'assets/ferias-poster.png' });
  assert.deepEqual(backgroundOf(gif), { type: 'video', asset: 'assets/ondas.gif' }, 'no poster: none named');
  assert.deepEqual(backgroundOf(photo), { type: 'image', asset: 'assets/foto.png', fit: 'cover' });
});

test('the Media panel lists pictures and videos, posters inside their video', () => {
  assert.deepEqual(mediaItems([clip, poster, gif, photo, font]), [clip, gif, photo]);
  // A poster nobody lists as its own is a picture like any other.
  assert.deepEqual(mediaItems([poster]), [poster]);
});

test('play time and size read like a clock and a file size', () => {
  assert.equal(formatDuration(0), '0:00');
  assert.equal(formatDuration(7_400), '0:07');
  assert.equal(formatDuration(65_000), '1:05');
  assert.equal(formatDuration(3_723_000), '1:02:03');
  for (const unknown of [null, undefined, -1, Number.NaN]) assert.equal(formatDuration(unknown), null);
  const bytes = (n) => `${n} B`;
  assert.deepEqual(videoFacts(clip, bytes), ['0:12', '24117248 B']);
  assert.deepEqual(videoFacts({ bytes: 10 }, bytes), ['10 B'], 'play time unknown');
  assert.deepEqual(videoFacts({}, bytes), []);
});

const plays = SCENARIOS.turing88.screens[0];
const host = { key: 'wch', models: [{ capabilities: { storage: false, videoPlayback: false } }] };

test('the inspector says what the connected screen does with the video', () => {
  assert.ok(playsVideos(plays));
  assert.ok(!playsVideos(host) && !playsVideos({ models: [] }) && !playsVideos(null));
  const status = (screen, live, state) => videoStatus({ screen, live, liveVideo: state ? { state } : null });
  assert.equal(status(null, false), 'noScreen');
  assert.equal(status(plays, false, 'missing'), 'checkWhenLive', 'not live: nothing checked');
  assert.equal(status(plays, true, null), 'checking');
  assert.equal(status(plays, true, 'notStarted'), 'checking');
  assert.equal(status(plays, true, 'onDevice'), 'stored');
  assert.equal(status(plays, true, 'missing'), 'missing');
  assert.equal(status(host, false), 'host', 'live mode decodes it here');
  assert.equal(status(host, true, 'host'), 'hostPlaying');
  assert.equal(status(host, true, 'noConverter'), 'hostNoConverter');
});

test('a new background is one undo step, and undo brings the old one back', () => {
  const store = createStore(DEMO_THEME);
  store.dispatch('setTheme', { patch: { background: backgroundOf(clip) } });
  assert.deepEqual(store.getState().theme.background, backgroundOf(clip));
  assert.ok(store.isDirty() && store.canUndo());
  // The poster-less GIF replaces the whole background: no poster left over.
  store.dispatch('setTheme', { patch: { background: backgroundOf(gif) } });
  assert.deepEqual(store.getState().theme.background, { type: 'video', asset: 'assets/ondas.gif' });
  store.undo();
  assert.deepEqual(store.getState().theme.background, backgroundOf(clip));
  store.undo();
  assert.deepEqual(store.getState().theme.background, DEMO_THEME.background);
  assert.ok(!store.isDirty());
  store.redo();
  assert.equal(store.getState().theme.background.type, 'video');
});

test('the demo adds videos with a poster, GIFs as videos and pictures as images', async () => {
  assert.equal(demoAssetName('Férias na Praia.MP4'), 'f-rias-na-praia.mp4');
  assert.equal(demoAssetName('noext'), 'noext');
  assert.equal(demoAssetName('???.gif'), 'file.gif');
  const demo = createDemoBackend('turing88');
  const picked = await demo.addMedia();
  assert.deepEqual(picked, {
    ref: 'assets/ferias.mp4', kind: 'video', poster: 'assets/ferias-poster.png', bytes: 24_117_248, durationMs: 12_400, posterError: null,
  });
  const again = await demo.addMedia();
  assert.equal(again.ref, 'assets/ferias-2.mp4', 'names stay unique');
  const dropped = demo.fileSource({ name: 'Ondas Mar.gif', size: 2048 });
  const waves = await demo.addMedia(dropped);
  assert.deepEqual([waves.kind, waves.ref, waves.bytes], ['video', 'assets/ondas-mar.gif', 2048]);
  const still = await demo.addMedia('demo://parado.gif');
  assert.deepEqual([still.kind, still.poster], ['image', null]);
  const photo = await demo.addMedia(demo.fileSource({ name: 'Foto.PNG', size: 10 }));
  assert.deepEqual([photo.kind, photo.ref], ['image', 'assets/foto.png']);
  await assert.rejects(demo.addMedia(demo.fileSource({ name: 'song.mp3', size: 1 })), (e) => e.code === 'notMedia' && e.args.file === 'song.mp3');
  const assets = await demo.assets();
  const byRef = Object.fromEntries(assets.map((a) => [a.ref, a]));
  assert.equal(byRef['assets/ferias-poster.png'].dataUrl, DEMO_POSTER_URL);
  assert.deepEqual([byRef['assets/ondas-mar.gif'].kind, byRef['assets/ondas-mar.gif'].animated], ['image', true]);
  assert.equal(byRef['assets/ferias.mp4'].kind, 'video');
  assert.deepEqual(mediaItems(assets).map((a) => a.ref), [
    'assets/parado.gif', 'assets/foto.png', 'assets/ferias.mp4', 'assets/ferias-2.mp4', 'assets/ondas-mar.gif',
  ]);
  assert.ok(!DEMO_POSTER_URL.includes('(') && !DEMO_POSTER_URL.includes(')'), 'safe inside CSS url()');
});

test('without ffmpeg the demo adds the video without a poster and says why', async () => {
  const demo = createDemoBackend('noffmpeg');
  const added = await demo.addMedia();
  assert.deepEqual([added.kind, added.poster, added.posterError.code], ['video', null, 'unsupported']);
  assert.deepEqual((await demo.assets()).map((a) => a.ref), ['assets/ferias.mp4']);
});

// Scenarios for demo mode (browser without Tauri): `?demo=<name>`.
const turing88 = Object.freeze({
  key: '/dev/ttyACM1',
  state: 'awake',
  family: 'turing-rev-c',
  models: [
    {
      id: 'turing-8.8',
      name: 'Turing Smart Screen 8.8"',
      diagonal: '8.8"',
      width: 480,
      height: 1920,
      capabilities: {
        brightness: true,
        deviceRotation: false,
        partialUpdate: true,
        backplateLed: false,
        storage: true,
        videoPlayback: true,
      },
      hardwareValidated: true,
    },
  ],
  display: { address: '/dev/ttyACM1', usb: '0525:a4a7', serial: null, manufacturer: null, product: null, location: '3-1.2' },
  wake: { address: '/dev/ttyACM0', usb: '1a86:ca88', serial: 'CT88INCH', manufacturer: 'Turing', product: 'UsbMonitor', location: '3-1.1' },
  restartable: true,
});

const asleep21 = Object.freeze({
  key: 'COM3',
  state: 'asleep',
  family: 'turing-rev-c',
  models: [
    { ...turing88.models[0], id: 'turing-2.1', name: 'Turing Smart Screen 2.1"', diagonal: '2.1"', width: 480, height: 480 },
    { ...turing88.models[0], id: 'turing-2.8', name: 'Turing Smart Screen 2.8"', diagonal: '2.8"', width: 480, height: 480 },
  ],
  display: null,
  wake: { address: 'COM3', usb: '1a86:ca21', serial: 'CT21INCH', manufacturer: 'Turing', product: 'UsbMonitor', location: null },
  restartable: true,
});

// A TURZX USB screen: it stores and plays files, but Bezel neither deletes
// them nor sets its boot media (D-2026-09-30-storage-video-7).
const turzx = Object.freeze({
  key: '3-1.4',
  state: 'awake',
  family: 'turing-usb',
  models: [
    {
      ...turing88.models[0],
      id: 'turing-usb-2.1-round',
      name: 'Turing 2.1" Round (USB)',
      diagonal: '2.1"',
      width: 480,
      height: 480,
      capabilities: { ...turing88.models[0].capabilities, partialUpdate: false },
      hardwareValidated: false,
    },
  ],
  display: { address: '3-1.4', usb: '1cbe:0088', serial: null, manufacturer: 'Turing', product: null, location: '3-1.4' },
  wake: null,
  restartable: false,
});

// A Turing USB panel the vendor app left in Windows' desktop mode: listed
// and switched back on request (not validated on hardware).
const desktopPanel = Object.freeze({
  key: 'hid:/dev/hidraw7',
  usb: '1a86:ad11',
  family: 'turing-usb',
  models: [
    { ...turing88.models[0], id: 'turing-usb-8.8', name: 'Turing 8.8" V1.x (USB)', hardwareValidated: false },
    { ...turing88.models[0], id: 'turing-usb-5.2', name: 'Turing 5.2" (USB)', diagonal: '5.2"', width: 720, height: 1280, hardwareValidated: false },
  ],
  hardwareValidated: false,
});

/** The screen a panel in desktop mode comes back as. */
export const DEMO_BACK_FROM_DESKTOP = Object.freeze({
  ...turzx,
  key: '3-1.6',
  models: [{ ...desktopPanel.models[0], capabilities: { ...turzx.models[0].capabilities } }],
  display: { ...turzx.display, address: '3-1.6', usb: '1cbe:0088', location: '3-1.6' },
});

/** Noon UTC of a day, in seconds: when the demo's files were sent. */
const day = (date) => Date.parse(`${date}T12:00:00Z`) / 1000;

/**
 * What the demo screens store: sizes in bytes (internal already net of the
 * reserve), and the catalog of what Bezel sent them, with local copies
 * (D-2026-09-30-storage-manager-5). A catalog entry is stored and has its
 * copy unless it says otherwise; a card entry names its card's capacity.
 */
export const DEMO_STORAGE = Object.freeze({
  internalTotal: 7_516_192_768,
  cardTotal: 31_914_983_424,
  files: Object.freeze([
    ['internal/image/logo.png', 184_320],
    ['internal/video/amd_90.mp4', 18_874_368],
    ['sd/video/chuva.mp4', 23_068_672],
  ]),
  catalog: Object.freeze([
    { path: 'internal/image/logo.png', size: 184_320, sentAt: day('2026-09-20'), source: '/home/demo/Imagens/logo.png', resolution: { width: 480, height: 480 } },
    { path: 'internal/video/amd_90.mp4', size: 18_874_368, sentAt: day('2026-09-14'), source: '/home/demo/Vídeos/amd.mp4', durationMs: 15_000, resolution: { width: 480, height: 1920 } },
  ]),
});

/**
 * The 8.8" of the Dragon Ball scenario: the demo's storage plus the video
 * the vendor app sent for that theme, as it is (the asset's own bytes).
 */
const DRAGON_STORAGE = Object.freeze({
  ...DEMO_STORAGE,
  files: Object.freeze([...DEMO_STORAGE.files, ['internal/video/dragon.mp4', 2_588_343]]),
});

/** The capacity of the user's card (29.7 GiB), the only trait the protocol shows of a card. */
export const VENDOR_CARD_TOTAL = 31_890_132_172;
/** Another card the demo's catalog remembers a file on. */
export const OTHER_CARD_TOTAL = 7_948_206_080;
const SENT_VIDEO = { durationMs: 10_000, resolution: { width: 480, height: 1920 } };

/**
 * The user's real 8.8" (`bezel storage ls --json`, 2026-09-30, exact bytes):
 * the vendor app's videos on the card, re-converted copies included
 * (`demon_open.mp4.mp4.mp4`, `NVI.mp427034822.mp4`, ...), next to an
 * internal memory Bezel filled (but `DARIUS.mp4`, whose original is on the
 * PC: it can be associated). One synthetic file of exactly 29,577,216 bytes is
 * what a rev C upload that hung leaves. The screen reports 18.6 MiB used of
 * 65.9 MiB inside and 103.8 MiB of 29.7 GiB on the card for the real files:
 * `usedExtra` is what it counts beyond (or short of, the listing rounds the
 * internal sizes) their sum. The catalog also remembers a file of this card
 * that is gone and an image on another card: both restorable.
 */
export const VENDOR_STORAGE = Object.freeze({
  internalTotal: 69_101_158,
  cardTotal: VENDOR_CARD_TOTAL,
  usedExtra: Object.freeze({ internal: -524_286, sd: 317_992 }),
  files: Object.freeze([
    ['internal/video/earth.mp4', 2_516_582],
    ['internal/video/DARIUS.mp4', 7_444_889],
    ['internal/video/jyanme.mp4', 4_404_019],
    ['internal/video/dragon.mp4', 2_621_440],
    ['internal/video/aniya.mp4', 3_040_870],
    ['sd/video/demon_open.mp4.mp4.mp4', 25_483_784],
    ['sd/video/demon.mp4.mp4.mp4', 13_237_564],
    ['sd/video/demon.mp401115025.mp4', 13_257_991],
    ['sd/video/8.8APEX_2.mp4', 2_259_535],
    ['sd/video/demon_open.mp4.mp4', 25_800_984],
    ['sd/video/AMD.mp4', 4_079_432],
    ['sd/video/NVI.mp427034822.mp4', 5_352_433],
    ['sd/video/NVI.mp4', 5_680_675],
    ['sd/video/Rani.mp4', 6_007_182],
    ['sd/video/m04.mp4', 876_578],
    ['sd/video/Rani.mp417075004.mp4', 5_646_986],
    ['sd/video/m04.mp424045157.mp4', 841_053],
    ['sd/video/bezel_test_cancel.mp4', 29_577_216],
  ]),
  catalog: Object.freeze([
    { path: 'internal/video/earth.mp4', size: 2_516_582, sentAt: day('2026-09-12'), source: '/home/demo/Vídeos/earth.mp4', ...SENT_VIDEO },
    { path: 'internal/video/jyanme.mp4', size: 4_404_019, sentAt: day('2026-09-14'), source: '/home/demo/Vídeos/jyanme.mp4', ...SENT_VIDEO },
    { path: 'internal/video/dragon.mp4', size: 2_621_440, sentAt: day('2026-09-15'), source: '/home/demo/Vídeos/dragon.mp4', ...SENT_VIDEO },
    { path: 'internal/video/aniya.mp4', size: 3_040_870, sentAt: day('2026-09-18'), source: '/home/demo/Vídeos/aniya.mp4', ...SENT_VIDEO },
    { path: 'sd/video/relogio.mp4', size: 6_291_456, sentAt: day('2026-09-05'), source: '/home/demo/Vídeos/relogio.mp4', card: VENDOR_CARD_TOTAL, state: 'missing', ...SENT_VIDEO },
    { path: 'sd/image/foto.png', size: 512_000, sentAt: day('2026-08-30'), source: '/home/demo/Imagens/foto.png', card: OTHER_CARD_TOTAL, resolution: { width: 480, height: 1920 } },
  ]),
  boot: 'internal/video/earth.mp4',
});

/** Where the demo's originals on the PC are (the folder its picker returns). */
export const DEMO_ORIGINALS_FOLDER = '/home/demo/Vídeos';

/**
 * Files on the PC the demo offers as originals of screen files
 * (D-2026-09-30-storage-manager-10): `DARIUS.mp4` is the internal file's,
 * `abertura.mp4` only has its size, `darius_final.mp4` is a byte longer.
 */
export const DEMO_ORIGINALS = Object.freeze({
  '/home/demo/Vídeos/DARIUS.mp4': { size: 7_444_889, kind: 'video', durationMs: 12_000, resolution: { width: 480, height: 1920 } },
  '/home/demo/Vídeos/abertura.mp4': { size: 7_444_889, kind: 'video', durationMs: 31_000, resolution: { width: 1920, height: 1080 } },
  '/home/demo/Vídeos/darius_final.mp4': { size: 7_444_890, kind: 'video', durationMs: 12_000, resolution: { width: 480, height: 1920 } },
  '/home/demo/Vídeos/AMD.mp4': { size: 4_079_432, kind: 'video', durationMs: 9_000, resolution: { width: 480, height: 1920 } },
});

/**
 * Local files the demo's file picker and drops know: size, and whether a
 * video is already in the 8.8"'s profile (else it is converted).
 */
export const DEMO_LOCAL_FILES = Object.freeze({
  'ferias.mp4': { size: 24_117_248, format: 'MP4', width: 1920, height: 1080, native: false, durationMs: 12_400 },
  'relogio.mp4': { size: 6_291_456, format: 'MP4', width: 480, height: 1920, native: true, durationMs: 8_000 },
  // An animated GIF (a video background) and a GIF of one picture (an image).
  'ondas.gif': { size: 3_145_728, format: 'GIF', width: 1920, height: 480, durationMs: 2_400 },
  'parado.gif': { size: 98_304, format: 'GIF', width: 480, height: 480, still: true },
  'foto.png': { size: 512_000, format: 'PNG', width: 1080, height: 1080 },
  // Stored with the wrong size, like a file bytes of a cancelled upload landed in.
  'torto.png': { size: 256_000, format: 'PNG', width: 480, height: 480, storedShort: true },
  // Over the 8.8"'s 25 MiB per file (D-2026-09-30-release-polish-12): as it
  // is, and once converted.
  'longo.mp4': { size: 31_457_280, format: 'MP4', width: 480, height: 1920, native: true },
  'show.mov': { size: 52_428_800, format: 'MOV', width: 1920, height: 1080, native: false, convertedSize: 27_262_976 },
});

/** The file the demo's picker returns. */
export const DEMO_PICKED = 'demo://ferias.mp4';

/** The video the demo's "Add video…" dialog returns. */
export const DEMO_PICKED_VIDEO = 'demo://ferias.mp4';

/** A poster the demo shows for every video it adds: a dusk gradient. */
const POSTER_SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="192" height="48" viewBox="0 0 192 48">'
  + '<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#1e1b4b"/>'
  + '<stop offset="0.55" stop-color="#7c3aed"/><stop offset="1" stop-color="#22d3ee"/></linearGradient></defs>'
  + '<rect width="192" height="48" fill="url(#g)"/><path d="M0 36 Q48 20 96 34 T192 30 V48 H0Z" fill="#0f172a" opacity="0.6"/></svg>';

/** The demo poster as a data URL (parentheses escaped for CSS `url()`). */
export const DEMO_POSTER_URL = `data:image/svg+xml,${encodeURIComponent(POSTER_SVG).replaceAll('(', '%28').replaceAll(')', '%29')}`;

/** A theme with a video background (the TURZX kind), for `?demo=video`. */
export const DEMO_VIDEO_THEME = Object.freeze({
  schema: 1,
  name: 'Vídeo',
  canvas: { width: 1920, height: 480 },
  orientation: 'landscape',
  refreshSeconds: 1,
  background: { type: 'video', asset: 'assets/nebula.mp4' },
  elements: [
    {
      id: 1,
      name: 'Clock',
      frame: { x: 1460, y: 150, width: 400, height: 180 },
      opacity: 1,
      visible: true,
      locked: false,
      kind: {
        type: 'text',
        content: { type: 'clock', pattern: '%H:%M' },
        style: { font: { family: 'Inter', weight: 700, italic: false }, size: 140, paint: '#ffffffff', align: 'center', valign: 'middle', letterSpacing: 0 },
      },
    },
  ],
});

/**
 * The poster of the demo's Dragon Ball-like video: its first picture upright
 * on the landscape canvas (a dusk sky, hills on grass, an orange orb).
 */
const DRAGON_POSTER_SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="192" height="48" viewBox="0 0 192 48">'
  + '<defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#1e3a8a"/>'
  + '<stop offset="0.55" stop-color="#7c3aed"/><stop offset="0.85" stop-color="#f59e0b"/></linearGradient></defs>'
  + '<rect width="192" height="48" fill="url(#s)"/><circle cx="150" cy="30" r="8" fill="#fde68a"/>'
  + '<path d="M0 34 Q30 24 60 33 T120 31 T192 30 V48 H0Z" fill="#312e81"/><rect y="41" width="192" height="7" fill="#16a34a"/>'
  + '<circle cx="48" cy="15" r="5" fill="#f97316"/></svg>';

/** The Dragon Ball-like poster as a data URL (parentheses escaped for CSS `url()`). */
export const DEMO_DRAGON_POSTER_URL = `data:image/svg+xml,${encodeURIComponent(DRAGON_POSTER_SVG).replaceAll('(', '%28').replaceAll(')', '%29')}`;

/**
 * The videos of the demo's themes, by asset: their own size (what
 * `video_auto` reads from the MP4 header), play time and bytes, the poster a
 * theme names, and whether the picture is stored turned for the 8.8"'s
 * panel, the vendor's way for horizontal themes (the user's Dragon Ball
 * video: 480x1920 in a 1920x480 theme).
 */
export const DEMO_THEME_VIDEOS = Object.freeze({
  'assets/dragon.mp4': Object.freeze({ width: 480, height: 1920, durationMs: 10_200, bytes: 2_588_343, poster: 'assets/poster-195.png', posterUrl: DEMO_DRAGON_POSTER_URL, preTurned: true }),
  'assets/nebula.mp4': Object.freeze({ width: 1920, height: 480, durationMs: 15_000, bytes: 18_874_368 }),
});

/**
 * A theme like the user's imported "Dragon Ball", for `?demo=dragon`: a
 * landscape canvas whose video is panel-native (480x1920, the picture turned
 * for the panel): Auto turns it back upright (D-2026-10-01-video-background-framing-2).
 * Its one element stays still, so a still preview keeps every pixel.
 */
export const DEMO_DRAGON_THEME = Object.freeze({
  schema: 1,
  name: 'Dragon Ball',
  canvas: { width: 1920, height: 480 },
  orientation: 'landscape',
  refreshSeconds: 1,
  background: { type: 'video', asset: 'assets/dragon.mp4', poster: 'assets/poster-195.png' },
  elements: [
    {
      id: 1,
      name: 'Title',
      frame: { x: 1380, y: 330, width: 500, height: 110 },
      opacity: 1,
      visible: true,
      locked: false,
      kind: {
        type: 'text',
        content: { type: 'static', text: 'Dragon Ball' },
        style: { font: { family: 'Inter', weight: 800, italic: false }, size: 80, paint: '#ffffffff', align: 'right', valign: 'middle', letterSpacing: 0 },
      },
    },
  ],
});

/** A theme with an animated GIF element, for `?demo=gif` (T-7.11). */
export const DEMO_GIF_THEME = Object.freeze({
  ...DEMO_VIDEO_THEME,
  name: 'GIF',
  refreshSeconds: 5,
  background: { type: 'color', color: '#0c0e16ff' },
  elements: [
    {
      id: 1,
      name: 'Spinner',
      frame: { x: 200, y: 140, width: 200, height: 200 },
      opacity: 1,
      visible: true,
      locked: false,
      kind: { type: 'image', asset: 'assets/spinner.gif', fit: 'contain' },
    },
  ],
});

/**
 * The catalog models the demo's screens and themes use: id, diagonal in
 * hundredths of an inch, and panel in portrait form (the core's catalog).
 */
export const DEMO_PANELS = Object.freeze([
  { id: 'turing-8.8', diagonalHundredths: 880, width: 480, height: 1920 },
  { id: 'turing-usb-8.8', diagonalHundredths: 880, width: 480, height: 1920 },
  { id: 'turing-3.5', diagonalHundredths: 350, width: 320, height: 480 },
  { id: 'turing-5', diagonalHundredths: 500, width: 480, height: 800 },
  { id: 'turing-2.1', diagonalHundredths: 210, width: 480, height: 480 },
  { id: 'turing-2.8', diagonalHundredths: 280, width: 480, height: 480 },
  { id: 'turing-usb-2.1-round', diagonalHundredths: 210, width: 480, height: 480 },
  { id: 'turing-usb-5.2', diagonalHundredths: 520, width: 720, height: 1280 },
]);

/** A small dark theme of `canvas` for the demo's library: a clock, a CPU ring, a card and a bar. */
function libraryTheme(name, canvas, orientation) {
  const { width: w, height: h } = canvas;
  const unit = Math.min(w, h);
  const at = (x, y, width, height) => ({ x: Math.round(x), y: Math.round(y), width: Math.round(width), height: Math.round(height) });
  const element = (id, label, frame, kind) => ({ id, name: label, frame, opacity: 1, visible: true, locked: false, kind });
  return Object.freeze({
    schema: 1,
    name,
    canvas: { ...canvas },
    orientation,
    refreshSeconds: 1,
    background: { type: 'color', color: '#05070cff' },
    elements: [
      element(1, 'Card', at(w * 0.04, h * 0.04, w * 0.92, h * 0.92), { type: 'shape', shape: 'rect', radius: Math.round(unit * 0.04), fill: '#111827ff', strokeWidth: 0 }),
      element(2, 'Clock', at(w * 0.08, h * 0.08, w * 0.5, unit * 0.22), {
        type: 'text',
        content: { type: 'clock', pattern: '%H:%M' },
        style: { font: { family: 'Inter', weight: 700, italic: false }, size: Math.round(unit * 0.18), paint: '#e2e8f0ff', align: 'left', valign: 'middle', letterSpacing: 0 },
      }),
      element(3, 'CPU', at(w * 0.1, h * 0.5, unit * 0.36, unit * 0.36), { type: 'ring', binding: { key: 'cpu.usage', min: 0, max: 100 }, startAngle: -135, sweep: 270, thickness: Math.round(unit * 0.05), clockwise: true, fill: '#38bdf8ff', track: '#ffffff26', roundCaps: true }),
      element(4, 'GPU', at(w * 0.55, h * 0.62, w * 0.36, unit * 0.06), { type: 'bar', binding: { key: 'gpu.usage', min: 0, max: 100 }, direction: 'leftToRight', fill: '#a78bfaff', track: '#ffffff1a', radius: 4 }),
    ],
  });
}

/**
 * The themes the demo's library has besides `Demo`, for other screens than
 * the 8.8": built in, and a theme of the user's that cannot be drawn
 * (`drawable: false`: no thumbnail).
 */
export const DEMO_LIBRARY = Object.freeze([
  { theme: libraryTheme('Midnight 3.5" vertical', { width: 320, height: 480 }, 'portrait'), bundled: true },
  { theme: libraryTheme('Midnight 5" horizontal', { width: 800, height: 480 }, 'landscape'), bundled: true },
  { theme: libraryTheme('Midnight 2.1" round', { width: 480, height: 480 }, 'portrait'), bundled: true },
  { theme: libraryTheme('TURZX 3.5"', { width: 480, height: 320 }, 'landscape'), bundled: false, drawable: false },
]);

/** The command the app shows to install its udev rule (Linux). */
export const DEMO_UDEV_COMMAND = 'sudo install -m 644 /home/demo/.cache/io.github.slipalison.bezel/60-bezel.rules /etc/udev/rules.d/60-bezel.rules && sudo udevadm control --reload && sudo udevadm trigger';

/**
 * @type {Record<string, {screens?: object[], desktopMode?: object[], error?: string, storage?: object, ffmpeg?: boolean, card?: boolean, internalTotal?: number, theme?: object, denied?: boolean, hung?: boolean, flaky?: boolean}>}
 */
export const SCENARIOS = Object.freeze({
  turing88: { screens: [turing88] },
  // The user's real card with the vendor app's copies, beside an internal
  // memory Bezel filled (D-2026-09-30-storage-manager-13).
  vendorCard: { screens: [turing88], storage: VENDOR_STORAGE },
  two: { screens: [turing88, asleep21] },
  empty: { screens: [] },
  error: { error: 'serial port enumeration: permission denied' },
  // Refusals: no ffmpeg, no card and an internal flash of 32 MB.
  noffmpeg: { screens: [turing88], ffmpeg: false, card: false, internalTotal: 32_000_000 },
  video: { screens: [turing88], theme: DEMO_VIDEO_THEME },
  turzx: { screens: [turzx] },
  // Linux without Bezel's udev rule: the screen is listed, opening it is denied.
  denied: { screens: [turing88], denied: true },
  // A screen whose firmware hangs: live mode and uploads stop until it is
  // restarted (D-2026-09-30-release-polish-13).
  hung: { screens: [turing88], hung: true },
  // A theme with an animated GIF, which moves in the preview (T-7.11).
  gif: { screens: [turing88], theme: DEMO_GIF_THEME },
  // The user's Dragon Ball: a pre-turned video Auto turns upright, which the
  // screen already stores as the vendor sent it (dragon.mp4, the asset's
  // exact bytes); without ffmpeg the preview shows its poster.
  dragon: { screens: [turing88], theme: DEMO_DRAGON_THEME, storage: DRAGON_STORAGE },
  dragonNoFfmpeg: { screens: [turing88], theme: DEMO_DRAGON_THEME, storage: DRAGON_STORAGE, ffmpeg: false },
  // A live screen that drops once and is connected again by itself (T-7.11).
  flaky: { screens: [turing88], flaky: true },
  desktop: { screens: [turing88], desktopMode: [desktopPanel] },
});

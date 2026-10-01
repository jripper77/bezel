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

/** What the demo screens store: sizes in bytes (internal already net of the reserve). */
export const DEMO_STORAGE = Object.freeze({
  internalTotal: 7_516_192_768,
  cardTotal: 31_914_983_424,
  files: Object.freeze([
    ['internal/image/logo.png', 184_320],
    ['internal/video/amd_90.mp4', 18_874_368],
    ['sd/video/chuva.mp4', 23_068_672],
  ]),
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
export const DEMO_POSTER_URL = `data:image/svg+xml,${encodeURIComponent(POSTER_SVG).replace(/\(/g, '%28').replace(/\)/g, '%29')}`;

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
 * @type {Record<string, {screens?: object[], desktopMode?: object[], error?: string, storage?: boolean, ffmpeg?: boolean, card?: boolean, internalTotal?: number, theme?: object, denied?: boolean, hung?: boolean, flaky?: boolean}>}
 */
export const SCENARIOS = Object.freeze({
  turing88: { screens: [turing88] },
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
  // A live screen that drops once and is connected again by itself (T-7.11).
  flaky: { screens: [turing88], flaky: true },
  desktop: { screens: [turing88], desktopMode: [desktopPanel] },
});

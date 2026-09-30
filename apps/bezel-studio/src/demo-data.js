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
});

/** What the demo screens store: sizes in bytes (internal already net of the reserve). */
export const DEMO_STORAGE = Object.freeze({
  internalTotal: 7_516_192_768,
  cardTotal: 31_914_983_424,
  files: Object.freeze([
    ['internal/image/logo.png', 184_320],
    ['internal/video/amd_90.mp4', 18_874_368],
    ['sd/video/chuva.mp4', 67_108_864],
  ]),
});

/**
 * Local files the demo's file picker and drops know: size, and whether a
 * video is already in the 8.8"'s profile (else it is converted).
 */
export const DEMO_LOCAL_FILES = Object.freeze({
  'ferias.mp4': { size: 24_117_248, format: 'MP4', width: 1920, height: 1080, native: false },
  'relogio.mp4': { size: 6_291_456, format: 'MP4', width: 480, height: 1920, native: true },
  'foto.png': { size: 512_000, format: 'PNG', width: 1080, height: 1080 },
  // Stored with the wrong size, like a file bytes of a cancelled upload landed in.
  'torto.png': { size: 256_000, format: 'PNG', width: 480, height: 480, storedShort: true },
});

/** The file the demo's picker returns. */
export const DEMO_PICKED = 'demo://ferias.mp4';

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

/** The command the app shows to install its udev rule (Linux). */
export const DEMO_UDEV_COMMAND = 'sudo install -m 644 /home/demo/.cache/io.github.slipalison.bezel/60-bezel.rules /etc/udev/rules.d/60-bezel.rules && sudo udevadm control --reload && sudo udevadm trigger';

/**
 * @type {Record<string, {screens?: object[], error?: string, storage?: boolean, ffmpeg?: boolean, card?: boolean, theme?: object, denied?: boolean}>}
 */
export const SCENARIOS = Object.freeze({
  turing88: { screens: [turing88] },
  two: { screens: [turing88, asleep21] },
  empty: { screens: [] },
  error: { error: 'serial port enumeration: permission denied' },
  noffmpeg: { screens: [turing88], ffmpeg: false, card: false },
  video: { screens: [turing88], theme: DEMO_VIDEO_THEME },
  turzx: { screens: [turzx] },
  // Linux without Bezel's udev rule: the screen is listed, opening it is denied.
  denied: { screens: [turing88], denied: true },
});

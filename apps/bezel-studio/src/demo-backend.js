// An in-memory backend for demo mode: a simulated screen, sensors that move,
// themes, media and an approximate renderer. Nothing here reaches hardware.
import { DEMO_BACK_FROM_DESKTOP, DEMO_LOCAL_FILES, DEMO_PICKED, DEMO_STORAGE, DEMO_UDEV_COMMAND, SCENARIOS } from './demo-data.js';
import { DEMO_THEME } from './demo-theme.js';
import { renderApprox } from './demo-render.js';
import { isHorizontal } from './editor/geometry.js';
import { pickLocale } from './i18n/index.js';

/** Well-known demo sensors: key, category, label, quantity, base value, swing. */
export const DEMO_SENSORS = Object.freeze([
  ['cpu.usage', 'cpu', 'CPU usage', 'percent', 32, 20],
  ['cpu.temperature', 'cpu', 'CPU temperature', 'celsius', 52, 8],
  ['cpu.frequency', 'cpu', 'CPU frequency', 'megahertz', 4720, 300],
  ['cpu.power', 'cpu', 'CPU power', 'watts', 64, 20],
  ['gpu.usage', 'gpu', 'GPU usage', 'percent', 41, 30],
  ['gpu.temperature', 'gpu', 'GPU temperature', 'celsius', 47, 6],
  ['gpu.power', 'gpu', 'GPU power', 'watts', 180, 60],
  ['memory.percent', 'memory', 'Memory in use', 'percent', 38, 3],
  ['memory.used', 'memory', 'Memory used', 'bytes', 24e9, 1e9],
  ['net.down', 'network', 'Download', 'bytesPerSecond', 2.4e6, 2e6],
  ['net.up', 'network', 'Upload', 'bytesPerSecond', 3.1e5, 2e5],
  ['disk.read', 'disk', 'Disk read', 'bytesPerSecond', 8e6, 7e6],
  ['hwmon.nvme0.composite', 'board', 'NVMe composite', 'celsius', 40, 2],
  ['system.uptime', 'system', 'Uptime', 'seconds', 93784, 0],
]);

const UNIT = { percent: '%', celsius: '°C', watts: ' W' };

/** Formats a demo value roughly like the core does. */
export function demoFormat(value, quantity) {
  if (quantity === 'megahertz') return value >= 1000 ? `${(value / 1000).toFixed(2)} GHz` : `${Math.round(value)} MHz`;
  if (quantity === 'bytes' || quantity === 'bytesPerSecond') {
    const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
    let v = value;
    let i = 0;
    while (v >= 1024 && i < units.length - 1) { v /= 1024; i += 1; }
    return `${i === 0 || v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}${quantity === 'bytesPerSecond' ? '/s' : ''}`;
  }
  if (quantity === 'seconds') {
    const d = Math.floor(value / 86400);
    const h = Math.floor((value % 86400) / 3600);
    const m = Math.floor((value % 3600) / 60);
    return `${d ? `${d}d ` : ''}${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
  }
  return `${Math.round(value)}${UNIT[quantity] ?? ''}`;
}

/** Sensor value at time t (seconds): a smooth wobble around the base. */
export function demoValue(base, swing, t, seed) {
  return Math.max(0, base + swing * Math.sin(t / 3 + seed) * 0.8 + swing * 0.2 * Math.sin(t * 1.7 + seed * 2));
}

/** What the demo import reports, like the Python theme importer does. */
export const DEMO_IMPORT_WARNINGS = Object.freeze([
  { code: 'backplateLed', args: {}, message: 'the backplate LED color (XuanFang rev B) is not part of a Bezel theme' },
  {
    code: 'cpuFanGuessed',
    args: { name: 'CPU.FAN_SPEED.TEXT' },
    message: "STATS.CPU.FAN_SPEED.TEXT: the Python app estimates this percent from the fan's RPM; Bezel measures the RPM (cpu.fan): rebind the widget to it and set its range",
  },
]);

/**
 * Orientation of a new theme when none is asked for, like the backend: the
 * last one used with the screen, else horizontal for bar-shaped panels (long
 * side at least twice the short one, like the 8.8" or no screen at all) and
 * vertical otherwise.
 * @param {{width:number, height:number}|undefined} model
 * @param {string|undefined} remembered
 */
export function demoOrientation(model, remembered) {
  if (remembered) return remembered;
  if (!model) return 'landscape';
  return Math.max(model.width, model.height) >= 2 * Math.min(model.width, model.height) ? 'landscape' : 'portrait';
}

/** The host `net.ping` measures unless the user picks another. */
export const DEMO_PING_HOST = '8.8.8.8';
/** The folder the demo's folder picker returns. */
export const DEMO_FOLDER = '/home/demo/mangohud';

/** Whether `text` can name a host to ping, like the backend checks it. */
export function demoIsHost(text) {
  return text.length <= 253 && /^[A-Za-z0-9:][A-Za-z0-9.:-]*$/.test(text);
}

/** The system refused to open `address` (the `denied` scenario). */
export function demoDenied(address) {
  const reason = 'Permission denied (os error 13)';
  return Object.assign(new Error(`access denied to ${address}: ${reason}`), {
    code: 'accessDenied',
    args: { address, reason },
    udevCommand: DEMO_UDEV_COMMAND,
  });
}

/** Pause between two simulated progress reports, ms. */
export const DEMO_STEP_MS = 150;
const CONVERT_STEPS = 8;
const UPLOAD_STEPS = 16;
/** The 8.8"'s videos: its panel in its native orientation. */
const NATIVE = { width: 480, height: 1920 };
const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'bmp', 'gif'];
const VIDEO_EXTENSIONS = ['mp4', 'mov', 'm4v', 'mkv', 'webm', 'avi'];
const INSTALL_HINTS = Object.freeze(['sudo dnf install ffmpeg', 'sudo apt install ffmpeg', 'winget install Gyan.FFmpeg']);

/** `image`, `video` or `null`, from a file name. */
export function demoKindOf(name) {
  const ext = name.includes('.') ? name.slice(name.lastIndexOf('.') + 1).toLowerCase() : '';
  if (IMAGE_EXTENSIONS.includes(ext)) return 'image';
  if (VIDEO_EXTENSIONS.includes(ext)) return 'video';
  return null;
}

/** The name a file gets on a screen, like the core suggests it. */
export function demoSuggestName(name, extension) {
  const dot = name.lastIndexOf('.');
  const stem = dot > 0 ? name.slice(0, dot) : name;
  let out = '';
  for (const c of stem.toLowerCase()) {
    const kept = /[a-z0-9_-]/.test(c) ? c : '_';
    if (!(kept === '_' && out.endsWith('_'))) out += kept;
  }
  out = out.replace(/^_+|_+$/g, '') || 'media';
  return `${out}.${extension}`;
}

const QUARTERS = Object.freeze({ portrait: 0, landscape: 1, 'reverse-portrait': 2, 'reverse-landscape': 3 });

/** Clockwise quarter turns from a theme orientation to the 8.8"'s panel. */
export function demoTurns(orientation) {
  return (2 + 4 - (QUARTERS[orientation] ?? 2)) % 4;
}

/** Where the 8.8" keeps a theme's video (`assets/nebula.mp4`, landscape: `nebula_90.mp4`). */
export function demoVideoName(theme) {
  const file = theme.background.asset.split('/').pop();
  const stem = file.includes('.') ? file.slice(0, file.lastIndexOf('.')) : file;
  return demoSuggestName(`${stem}${['', '_90', '_180', '_270'][demoTurns(theme.orientation)]}.mp4`, 'mp4');
}

/**
 * A simulated screen storage: capacity, files, uploads with progress over
 * time and cancel, deletes, playback and the boot media, with the same
 * confirmations and refusals as the app.
 */
function createDemoStorage(chosen, { delay, live, theme, screens }) {
  const card = chosen.card !== false;
  const files = new Map(DEMO_STORAGE.files.filter(([p]) => card || !p.startsWith('sd/')));
  const locals = new Map(Object.entries(DEMO_LOCAL_FILES).map(([name, f]) => [`demo://${name}`, { name, ...f }]));
  const listeners = new Set();
  const pending = new Map();
  const tools = { ready: chosen.ffmpeg !== false, configured: null };
  let tickets = 0;
  let job = null;
  const state = { playback: null, boot: null, bootBrightness: null };

  // Errors like the app's: a code, its arguments and the English text.
  const refuse = (code, message, args = {}) => Promise.reject(Object.assign(new Error(message), { code, args }));
  const screenOf = (key) => screens().find((s) => s.key === key);
  const limited = (key) => screenOf(key)?.family === 'turing-usb';
  const entry = (path, size = files.get(path) ?? null) => {
    const [medium, kind, name] = path.split('/');
    return { path, medium, kind, name, size };
  };
  const totalOf = (medium) => (medium === 'sd' ? DEMO_STORAGE.cardTotal : DEMO_STORAGE.internalTotal);
  function capacity(medium) {
    const used = [...files].filter(([p]) => p.startsWith(`${medium}/`)).reduce((sum, [, size]) => sum + size, 0);
    const total = totalOf(medium);
    return { total, used, free: total - used };
  }

  function videoOfTheme() {
    const current = theme();
    if (!live() || current.background?.type !== 'video') return null;
    const name = demoVideoName(current);
    const stored = ['internal', ...(card ? ['sd'] : [])].map((m) => `${m}/video/${name}`).find((p) => files.get(p) > 0);
    if (stored) return { state: 'onDevice', path: stored };
    return { state: 'missing', path: `${card ? 'sd' : 'internal'}/video/${name}` };
  }

  function readyAnswer(local, path, convert) {
    const ticket = (tickets += 1);
    const replaces = files.get(path) > 0 ? entry(path) : null;
    pending.set(ticket, { path, bytes: local.size, convert, storedShort: Boolean(local.storedShort) });
    return {
      status: 'ready',
      ticket,
      source: local.name,
      target: entry(path, null),
      bytes: local.size,
      format: local.format,
      dimensions: { width: local.width, height: local.height },
      convert,
      replaces,
    };
  }

  function check(local, medium, name) {
    const kind = demoKindOf(local.name);
    const refused = (code, extra = {}) => ({ status: 'refused', code, message: code, mismatches: [], candidates: [], accepted: [], ...extra });
    if (!kind) return refused('wrongKind');
    if (!local.size) return refused('emptyFile');
    if (medium === 'sd' && !card) return refused('noCard');
    const needsConversion = kind === 'video' && !local.native;
    if (needsConversion && !tools.ready) {
      const found = `${local.width}x${local.height}`;
      return refused('needsConverter', { mismatches: [{ code: 'audio' }, { code: 'resolution', found, expected: `${NATIVE.width}x${NATIVE.height}` }] });
    }
    const extension = kind === 'video' ? 'mp4' : local.name.slice(local.name.lastIndexOf('.') + 1).toLowerCase();
    const path = `${medium}/${kind}/${name ?? demoSuggestName(local.name, extension)}`;
    const free = capacity(medium).free + (files.get(path) ?? 0);
    if (!needsConversion && local.size >= free) {
      const candidates = [...files.keys()].filter((p) => p.startsWith(`${medium}/`)).map((p) => entry(p)).sort((a, b) => b.size - a.size);
      return refused('noSpace', { bytes: local.size, limit: free, candidates });
    }
    const convert = needsConversion ? { ...NATIVE, quarterTurns: demoTurns(theme().orientation), cropped: local.width * NATIVE.height !== local.height * NATIVE.width } : null;
    return readyAnswer(local, path, convert);
  }

  const emit = (phase, done, total) => listeners.forEach((cb) => cb({ phase, done: Math.round(done), total }));

  /** Reports `steps` steps of `phase`; `true` when cancelled meanwhile. */
  async function steps(phase, total, count, each = () => {}) {
    for (let i = 0; i <= count; i += 1) {
      each((total * i) / count);
      emit(phase, (total * i) / count, total);
      if (i === count) return false;
      await delay(DEMO_STEP_MS);
      if (job.cancelled) return true;
    }
    return false;
  }

  async function upload(p) {
    let size = p.bytes;
    if (p.convert) {
      if (await steps('convert', 20_000, CONVERT_STEPS)) return { status: 'cancelled', path: p.path, partial: null };
      size = Math.round(p.bytes * 0.6);
    }
    const cancelled = await steps('upload', size, UPLOAD_STEPS, (done) => files.set(p.path, Math.round(done)));
    if (cancelled) {
      const partial = files.get(p.path) || null;
      if (!partial) files.delete(p.path);
      return { status: 'cancelled', path: p.path, partial };
    }
    emit('verify', 0, 1);
    await delay(DEMO_STEP_MS);
    if (p.storedShort) {
      const stored = size - 10;
      files.set(p.path, stored);
      const message = `${p.path} was stored with ${stored} bytes, not the file's ${size}: the stored size differs; delete it and send it again`;
      throw Object.assign(new Error(message), { code: 'sizeMismatch', args: { file: p.path, stored: String(stored), expected: String(size) } });
    }
    emit('verify', 1, 1);
    state.playback = null;
    return { status: 'done', file: entry(p.path, size), converted: Boolean(p.convert) };
  }

  const toolsDto = () => ({
    ready: tools.ready,
    version: tools.ready ? '7.1' : null,
    installHints: tools.ready ? [] : [...INSTALL_HINTS],
    configured: tools.configured,
    rejected: null,
  });

  return {
    storageOverview: (key) => {
      if (chosen.denied) return Promise.reject(demoDenied(key));
      if (!screenOf(key)?.models.every((m) => m.capabilities.storage)) return refuse('unsupported', 'not supported: no storage', { detail: 'no storage' });
      if (job) return refuse('busy', 'a storage operation is using the screen');
      const folders = (card ? ['internal', 'sd'] : ['internal']).flatMap((medium) => ['image', 'video'].map((kind) => ({
        medium,
        kind,
        files: [...files.keys()].filter((p) => p.startsWith(`${medium}/${kind}/`)).map((p) => entry(p)),
        error: null,
      })));
      return Promise.resolve({ internal: capacity('internal'), card: card ? capacity('sd') : null, folders });
    },
    mediaTools: () => Promise.resolve(toolsDto()),
    locateFfmpeg: () => {
      Object.assign(tools, { ready: true, configured: '/opt/ffmpeg/bin/ffmpeg' });
      return Promise.resolve(toolsDto());
    },
    pickMedia: () => Promise.resolve(DEMO_PICKED),
    /** A dropped file as a source the demo can prepare. */
    fileSource: (file) => {
      if (!file) return null;
      const source = `demo://${file.name}`;
      if (!locals.has(source)) locals.set(source, { name: file.name, size: file.size, format: file.name.split('.').pop().toUpperCase(), width: 1080, height: 1080, native: false });
      return source;
    },
    prepareUpload: (key, source, medium) => {
      if (job) return refuse('busy', 'a storage operation is using the screen');
      const local = locals.get(source);
      if (!local || !screenOf(key)) return refuse('fileError', `${source}: no such file`, { file: source, reason: 'no such file' });
      return Promise.resolve(check(local, medium));
    },
    prepareThemeVideo: (key) => {
      const video = videoOfTheme();
      if (live() !== key || video?.state !== 'missing') return refuse('noVideo', 'the live screen is not missing the theme video');
      const [medium, , name] = video.path.split('/');
      const local = { name: theme().background.asset.split('/').pop(), size: 18_874_368, format: 'MP4', width: 1920, height: 480, native: false };
      return Promise.resolve(check(local, medium, name));
    },
    runUpload: async (ticket, overwrite) => {
      const p = pending.get(ticket);
      if (!p) return refuse('stale', 'this upload is no longer prepared');
      pending.delete(ticket);
      if (files.get(p.path) > 0 && !overwrite) return refuse('notConfirmed', `replacing ${p.path} needs confirmation`);
      if (job) return refuse('busy', 'a storage operation is using the screen');
      job = { cancelled: false };
      try {
        return await upload(p);
      } finally {
        job = null;
      }
    },
    cancelJob: () => {
      if (job) job.cancelled = true;
      return Promise.resolve(Boolean(job));
    },
    deleteStored: (key, path, confirmed) => {
      if (limited(key)) return refuse('unsupported', 'not supported: deleting files', { detail: 'deleting files' });
      if (!confirmed) return refuse('notConfirmed', `deleting ${path} needs confirmation`);
      files.delete(path);
      return Promise.resolve();
    },
    playStored: (key, path) => {
      if (live() === key) return refuse('live', 'turn live mode off to play files');
      if (!(files.get(path) > 0)) return refuse('invalidInput', `invalid input: ${path} is not stored on the screen`, { detail: `${path} is not stored on the screen` });
      state.playback = path;
      return Promise.resolve();
    },
    stopPlayback: (key) => {
      if (live() === key) return refuse('live', 'turn live mode off to stop files');
      state.playback = null;
      return Promise.resolve();
    },
    setBootMedia: (key, path, confirmed, brightness = null) => {
      if (limited(key)) return refuse('unsupported', 'not supported: the boot media', { detail: 'the boot media' });
      if (!confirmed) return refuse('notConfirmed', 'the boot media needs confirmation');
      if (path && !(files.get(path) > 0)) return refuse('invalidInput', `invalid input: ${path} is not stored on the screen`, { detail: `${path} is not stored on the screen` });
      state.boot = path;
      if (brightness !== null) state.bootBrightness = brightness;
      if (path) state.playback = path;
      return Promise.resolve();
    },
    onJobProgress: (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    videoOfTheme,
    /** What the simulated screen plays, shows at power-up and starts with. */
    storageState: () => ({ ...state, files: new Map(files) }),
  };
}

/**
 * @param {string} scenario key of SCENARIOS
 * @param {{now?: () => number, delay?: (ms: number) => Promise<void>}} [clock]
 * @param {{onWindow?: (state: 'hidden'|'closed') => void, languages?: readonly string[]}} [hooks]
 *   what the window does, and the system's languages
 */
export function createDemoBackend(scenario, clock = {}, hooks = {}) {
  const now = clock.now ?? (() => Date.now() / 1000);
  const delay = clock.delay ?? ((ms) => new Promise((resolve) => { setTimeout(resolve, ms); }));
  const chosen = SCENARIOS[scenario] ?? SCENARIOS.turing88;
  let theme = structuredClone(chosen.theme ?? DEMO_THEME);
  // What the bus lists: screens and panels in desktop mode (a panel
  // switched back comes back as a screen).
  const devices = { screens: [...(chosen.screens ?? [])], desktopMode: [...(chosen.desktopMode ?? [])] };
  let live = false;
  let autostart = false;
  const saved = [{ location: 'demo://Demo', theme: structuredClone(DEMO_THEME) }];
  const images = [];
  /** Screen key → last orientation shown on it or chosen for it. */
  const remembered = new Map();
  const modelOf = (key) => devices.screens.find((s) => s.key === key)?.models[0];
  const storage = createDemoStorage(chosen, { delay, live: () => live, theme: () => theme, screens: () => devices.screens });
  const { videoOfTheme, ...storageApi } = storage;
  // The window, like the app: the close button hides it while a screen is
  // live, asks the UI when edits are unsaved, and closes it otherwise.
  let unsaved = false;
  let windowState = 'open';
  // The language the user chose (`null`: the system's, from the browser).
  let language = null;
  const systemLanguage = pickLocale(hooks.languages ?? []);
  const sensorOptions = { pingHost: null, mangohudDir: null };
  const closeListeners = new Set();
  const windowGoes = (state) => {
    windowState = state;
    hooks.onWindow?.(state);
  };

  return {
    ...storageApi,
    listDevices: () => (chosen.error ? Promise.reject(new Error(chosen.error)) : Promise.resolve(structuredClone(devices))),
    leaveDesktopMode: (key, confirmed) => {
      const at = devices.desktopMode.findIndex((p) => p.key === key);
      if (!confirmed) {
        const message = 'switching a panel in desktop mode back to USB monitor mode (not validated on hardware) needs confirmation';
        return Promise.reject(Object.assign(new Error(message), { code: 'notConfirmed', args: { detail: message } }));
      }
      if (at < 0) return Promise.reject(Object.assign(new Error(`screen not found: no panel in desktop mode at ${key}`), { code: 'screenNotFound', args: { screen: `no panel in desktop mode at ${key}` } }));
      const [panel] = devices.desktopMode.splice(at, 1);
      devices.screens.push(structuredClone(DEMO_BACK_FROM_DESKTOP));
      return Promise.resolve({ model: panel.models[0].name });
    },
    catalog: () => Promise.resolve(DEMO_SENSORS.map(([key, category, label, quantity]) => ({ key, category, label, quantity, source: 'demo' }))),
    sample: () => {
      const t = now();
      const readings = {};
      DEMO_SENSORS.forEach(([key, , , quantity, base, swing], i) => {
        const value = demoValue(base, swing, t, i);
        readings[key] = { value, display: demoFormat(value, quantity) };
      });
      readings['gpu.1.fan'] = { unavailable: 'no fan sensor', display: '—' };
      return Promise.resolve({ sampleMillis: 3, readings, live: live || null, liveError: null, video: videoOfTheme() });
    },
    session: () => Promise.resolve({ theme: structuredClone(theme), location: chosen.theme ? null : saved[0].location }),
    render: (next) => {
      const started = performance.now();
      const frame = renderApprox(next, now());
      return Promise.resolve({ ...frame, millis: performance.now() - started });
    },
    pushTheme: (next) => {
      theme = structuredClone(next);
      if (live) remembered.set(live, theme.orientation);
      return Promise.resolve();
    },
    setLive: (on, screen) => {
      if (on && chosen.denied) return Promise.reject(demoDenied(screen));
      live = on ? screen : null;
      if (live) remembered.set(live, theme.orientation);
      return Promise.resolve({ live });
    },
    setBrightness: (screen) => (chosen.denied ? Promise.reject(demoDenied(screen)) : Promise.resolve()),
    release: (screen) => (chosen.denied ? Promise.reject(demoDenied(screen)) : Promise.resolve()),
    saveTheme: (next, saveAs) => {
      theme = structuredClone(next);
      const location = `demo://${next.name}`;
      const at = saved.findIndex((s) => s.location === location);
      if (saveAs || at < 0) saved.push({ location, theme: structuredClone(next) });
      else saved[at].theme = structuredClone(next);
      return Promise.resolve({ location });
    },
    listThemes: () => Promise.resolve(saved.map((s, i) => ({
      name: s.theme.name, location: s.location, canvas: { ...s.theme.canvas }, orientation: s.theme.orientation, bundled: i === 0,
    }))),
    openTheme: (location) => {
      const found = saved.find((s) => s.location === location);
      if (found) return Promise.resolve(structuredClone(found.theme));
      return Promise.reject(Object.assign(new Error(`${location} is not in the theme library; import it instead`), { code: 'notInLibrary', args: { location } }));
    },
    newTheme: (screen, name = 'Untitled', orientation = null) => {
      const model = modelOf(screen);
      const chosenOrientation = orientation ?? demoOrientation(model, remembered.get(screen));
      if (orientation && screen) remembered.set(screen, orientation);
      const short = model ? Math.min(model.width, model.height) : 480;
      const long = model ? Math.max(model.width, model.height) : 1920;
      const canvas = isHorizontal(chosenOrientation) ? { width: long, height: short } : { width: short, height: long };
      return Promise.resolve({ ...structuredClone(DEMO_THEME), name, orientation: chosenOrientation, canvas, elements: [] });
    },
    importTheme: () => Promise.resolve({ theme: { ...structuredClone(DEMO_THEME), name: 'Imported' }, warnings: structuredClone(DEMO_IMPORT_WARNINGS) }),
    addImage: () => {
      const ref = `assets/image-${images.length + 1}.png`;
      images.push(ref);
      return Promise.resolve({ ref });
    },
    assets: () => Promise.resolve(images.map((ref) => ({ ref, kind: 'image' }))),
    fonts: () => Promise.resolve(['Inter', 'JetBrains Mono']),
    getAutostart: () => Promise.resolve(autostart),
    setAutostart: (on) => {
      autostart = on;
      return Promise.resolve();
    },
    setUnsaved: (on) => {
      unsaved = Boolean(on);
      return Promise.resolve();
    },
    closeWindow: () => {
      windowGoes(live ? 'hidden' : 'closed');
      return Promise.resolve();
    },
    onCloseRequested: (cb) => {
      closeListeners.add(cb);
      return Promise.resolve(() => closeListeners.delete(cb));
    },
    /** The window's close button. */
    requestClose: () => {
      if (live) windowGoes('hidden');
      else if (unsaved) for (const cb of closeListeners) cb();
      else windowGoes('closed');
    },
    windowState: () => windowState,
    preferences: () => Promise.resolve({
      language,
      systemLanguage,
      pingHost: sensorOptions.pingHost ?? DEMO_PING_HOST,
      defaultPingHost: DEMO_PING_HOST,
      mangohudDir: sensorOptions.mangohudDir,
      mangohud: true,
    }),
    setSensorOptions: (pingHost, mangohudDir) => {
      const host = pingHost.trim();
      if (host && !demoIsHost(host)) {
        return Promise.reject(Object.assign(new Error(`"${host}" is not a host name or an IP address`), { code: 'invalidHost', args: { host } }));
      }
      if (mangohudDir !== null && !mangohudDir.startsWith('/')) {
        return Promise.reject(Object.assign(new Error(`"${mangohudDir}" is not a folder`), { code: 'invalidFolder', args: { folder: mangohudDir } }));
      }
      Object.assign(sensorOptions, { pingHost: host && host !== DEMO_PING_HOST ? host : null, mangohudDir });
      return Promise.resolve();
    },
    pickFolder: () => Promise.resolve(DEMO_FOLDER),
    setLanguage: (next) => {
      if (next !== null && !['pt-BR', 'en'].includes(next)) {
        return Promise.reject(Object.assign(new Error(`unknown language "${next}"`), { code: 'unknownLanguage', args: { language: next } }));
      }
      language = next;
      return Promise.resolve();
    },
    isLive: () => Boolean(live),
  };
}

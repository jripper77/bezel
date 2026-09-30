// An in-memory backend for demo mode: a simulated screen, sensors that move,
// themes, media and an approximate renderer. Nothing here reaches hardware.
import { SCENARIOS } from './demo-data.js';
import { DEMO_THEME } from './demo-theme.js';
import { renderApprox } from './demo-render.js';
import { isHorizontal } from './editor/geometry.js';

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
  'the backplate LED color (XuanFang rev B) is not part of a Bezel theme',
  'STATS.CPU.FAN_SPEED: Bezel does not measure this yet; the widget shows it as unavailable',
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

/**
 * @param {string} scenario key of SCENARIOS
 * @param {{now?: () => number}} [clock]
 */
export function createDemoBackend(scenario, clock = {}) {
  const now = clock.now ?? (() => Date.now() / 1000);
  const chosen = SCENARIOS[scenario] ?? SCENARIOS.turing88;
  let theme = structuredClone(DEMO_THEME);
  let live = false;
  let autostart = false;
  const saved = [{ location: 'demo://Demo', theme: structuredClone(DEMO_THEME) }];
  const images = [];
  /** Screen key → last orientation shown on it or chosen for it. */
  const remembered = new Map();
  const modelOf = (key) => (chosen.screens ?? []).find((s) => s.key === key)?.models[0];

  return {
    listScreens: () => (chosen.error ? Promise.reject(new Error(chosen.error)) : Promise.resolve(structuredClone(chosen.screens))),
    catalog: () => Promise.resolve(DEMO_SENSORS.map(([key, category, label, quantity]) => ({ key, category, label, quantity, source: 'demo' }))),
    sample: () => {
      const t = now();
      const readings = {};
      DEMO_SENSORS.forEach(([key, , , quantity, base, swing], i) => {
        const value = demoValue(base, swing, t, i);
        readings[key] = { value, display: demoFormat(value, quantity) };
      });
      readings['gpu.1.fan'] = { unavailable: 'no fan sensor', display: '—' };
      return Promise.resolve({ sampleMillis: 3, readings, live: live || null, liveError: null });
    },
    session: () => Promise.resolve({ theme: structuredClone(theme), location: saved[0].location }),
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
      live = on ? screen : null;
      if (live) remembered.set(live, theme.orientation);
      return Promise.resolve({ live });
    },
    setBrightness: () => Promise.resolve(),
    release: () => Promise.resolve(),
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
      return found ? Promise.resolve(structuredClone(found.theme)) : Promise.reject(new Error(`no theme at ${location}`));
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
    importTheme: () => Promise.resolve({ theme: { ...structuredClone(DEMO_THEME), name: 'Imported' }, warnings: [...DEMO_IMPORT_WARNINGS] }),
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
    isLive: () => Boolean(live),
  };
}

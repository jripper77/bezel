// An in-memory backend for demo mode: a simulated screen, sensors that move,
// themes, media and an approximate renderer. Nothing here reaches hardware.
import { SCENARIOS } from './demo-data.js';
import { DEMO_THEME } from './demo-theme.js';
import { renderApprox } from './demo-render.js';

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
  const saved = [{ name: 'Demo', location: 'demo://Demo', canvas: DEMO_THEME.canvas }];
  const images = [];

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
      return Promise.resolve();
    },
    setLive: (on, screen) => {
      live = on ? screen : null;
      return Promise.resolve({ live });
    },
    setBrightness: () => Promise.resolve(),
    release: () => Promise.resolve(),
    saveTheme: (next, saveAs) => {
      theme = structuredClone(next);
      const location = `demo://${next.name}`;
      if (saveAs || !saved.some((s) => s.location === location)) saved.push({ name: next.name, location, canvas: next.canvas });
      return Promise.resolve({ location });
    },
    listThemes: () => Promise.resolve(structuredClone(saved).map((s, i) => ({ ...s, bundled: i === 0 }))),
    openTheme: (location) => {
      const found = saved.find((s) => s.location === location);
      return found ? Promise.resolve(structuredClone({ ...DEMO_THEME, name: found.name })) : Promise.reject(new Error(`no theme at ${location}`));
    },
    newTheme: (screen, name = 'Untitled') => Promise.resolve({ ...structuredClone(DEMO_THEME), name, elements: [] }),
    importTheme: () => Promise.resolve(null),
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

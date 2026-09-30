// The only module that knows where data comes from: Tauri's IPC in the app,
// or an in-memory demo backend in a browser on localhost (Playwright, UI work
// without hardware). Both expose the same async API.
import { createDemoBackend } from './demo-backend.js';

/** Parses the renderer's frame: u32 LE width, u32 LE height, then RGBA8. */
export function parseFrame(buffer) {
  const bytes = buffer instanceof ArrayBuffer ? new Uint8Array(buffer) : new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength);
  if (bytes.length < 8) throw new Error('frame too short');
  const view = new DataView(bytes.buffer, bytes.byteOffset, 8);
  const width = view.getUint32(0, true);
  const height = view.getUint32(4, true);
  const rgba = new Uint8ClampedArray(bytes.buffer, bytes.byteOffset + 8, bytes.length - 8);
  if (rgba.length !== width * height * 4) throw new Error(`frame is ${rgba.length} bytes, expected ${width * height * 4}`);
  return { width, height, rgba };
}

function tauriBridge(invoke) {
  return {
    mode: 'tauri',
    listScreens: () => invoke('list_screens'),
    catalog: () => invoke('sensor_catalog'),
    sample: () => invoke('sample_sensors'),
    session: () => invoke('editor_session'),
    render: async (theme) => {
      const started = performance.now();
      const frame = parseFrame(await invoke('render_preview', { theme }));
      return { ...frame, millis: performance.now() - started };
    },
    pushTheme: (theme) => invoke('push_theme', { theme }),
    setLive: (on, screen) => invoke('set_live', { on, screen }),
    setBrightness: (screen, percent) => invoke('set_brightness', { screen, percent }),
    release: (screen) => invoke('release_screen', { screen }),
    saveTheme: (theme, saveAs) => invoke('save_theme', { theme, saveAs }),
    listThemes: () => invoke('list_themes'),
    openTheme: (location) => invoke('open_theme', { location }),
    newTheme: (screen, name, orientation) => invoke('new_theme', { screen, name, orientation }),
    importTheme: () => invoke('import_theme'),
    addImage: () => invoke('add_image'),
    assets: () => invoke('list_assets'),
    fonts: () => invoke('list_fonts'),
    getAutostart: () => invoke('get_autostart'),
    setAutostart: (on) => invoke('set_autostart', { on }),
  };
}

/**
 * Picks the backend for this page.
 * @param {{__TAURI__?: any, location: {hostname: string, search: string}}} win
 */
export function createBridge(win) {
  const invoke = win.__TAURI__?.core?.invoke;
  if (typeof invoke === 'function') return tauriBridge(invoke);
  const local = ['localhost', '127.0.0.1'].includes(win.location.hostname);
  if (!local) {
    const fail = () => Promise.reject(new Error('no backend'));
    return new Proxy({ mode: 'unavailable' }, { get: (t, k) => (k in t ? t[k] : fail) });
  }
  const scenario = new URLSearchParams(win.location.search).get('demo') ?? 'turing88';
  return { mode: 'demo', ...createDemoBackend(scenario) };
}

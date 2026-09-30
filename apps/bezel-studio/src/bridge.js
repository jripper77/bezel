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

/** Event of a running upload's progress (`storage-progress` in the backend). */
export const PROGRESS_EVENT = 'storage-progress';

/**
 * Event the app sends when the window's close button is pressed with unsaved
 * edits and no screen live: the UI asks, then calls `closeWindow`.
 */
export const CLOSE_EVENT = 'close-requested';

/** Demo mode: the window event that stands for the close button. */
export const DEMO_CLOSE_EVENT = 'bezel-demo-close';

/**
 * Subscribes to files dropped on the window from the system: Tauri owns the
 * drag and reports the paths and the pointer (physical pixels).
 */
function onFileDrop(tauri, cb) {
  const webview = tauri.webview?.getCurrentWebview?.();
  if (webview?.onDragDropEvent) return webview.onDragDropEvent((e) => cb(e.payload));
  const listen = tauri.event?.listen;
  if (typeof listen !== 'function') return Promise.resolve(() => {});
  const events = { 'tauri://drag-over': 'over', 'tauri://drag-drop': 'drop', 'tauri://drag-leave': 'leave' };
  return Promise.all(Object.entries(events).map(([name, type]) => listen(name, (e) => cb({ type, ...e.payload }))));
}

function tauriBridge(invoke, tauri = {}) {
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
    storageOverview: (screen) => invoke('storage_overview', { screen }),
    mediaTools: () => invoke('media_tools'),
    locateFfmpeg: () => invoke('locate_ffmpeg'),
    pickMedia: () => invoke('pick_media'),
    prepareUpload: (screen, source, medium) => invoke('prepare_upload', { screen, source, medium }),
    prepareThemeVideo: (screen) => invoke('prepare_theme_video', { screen }),
    runUpload: (ticket, overwrite) => invoke('run_upload', { ticket, overwrite }),
    cancelJob: () => invoke('cancel_job'),
    deleteStored: (screen, path, confirmed) => invoke('delete_stored', { screen, path, confirmed }),
    playStored: (screen, path) => invoke('play_stored', { screen, path }),
    stopPlayback: (screen) => invoke('stop_playback', { screen }),
    setBootMedia: (screen, path, confirmed, brightness = null) => invoke('set_boot_media', { screen, path, confirmed, brightness }),
    setUnsaved: (unsaved) => invoke('set_unsaved', { unsaved }),
    closeWindow: () => invoke('close_window'),
    preferences: () => invoke('preferences'),
    setLanguage: (language) => invoke('set_language', { language }),
    setSensorOptions: (pingHost, mangohudDir) => invoke('set_sensor_options', { pingHost, mangohudDir }),
    pickFolder: () => invoke('pick_folder'),
    onJobProgress: (cb) => (typeof tauri.event?.listen === 'function' ? tauri.event.listen(PROGRESS_EVENT, (e) => cb(e.payload)) : Promise.resolve(() => {})),
    onCloseRequested: (cb) => (typeof tauri.event?.listen === 'function' ? tauri.event.listen(CLOSE_EVENT, () => cb()) : Promise.resolve(() => {})),
    onFileDrop: (cb) => onFileDrop(tauri, cb),
    // Files dropped in the webview carry no path: the system drop above does.
    fileSource: () => null,
  };
}

/**
 * Picks the backend for this page.
 * @param {{__TAURI__?: any, location: {hostname: string, search: string}}} win
 */
export function createBridge(win) {
  const invoke = win.__TAURI__?.core?.invoke;
  if (typeof invoke === 'function') return tauriBridge(invoke, win.__TAURI__);
  const local = ['localhost', '127.0.0.1'].includes(win.location.hostname);
  if (!local) {
    const fail = () => Promise.reject(new Error('no backend'));
    return new Proxy({ mode: 'unavailable' }, { get: (t, k) => (k in t ? t[k] : fail) });
  }
  const scenario = new URLSearchParams(win.location.search).get('demo') ?? 'turing88';
  // What the window does is shown on the page (`data-demo-window`), and the
  // close button is a window event: Playwright drives and checks both.
  const root = win.document?.documentElement;
  const demo = createDemoBackend(scenario, {}, {
    onWindow: (state) => root?.setAttribute('data-demo-window', state),
    languages: win.navigator?.languages ?? [],
  });
  win.addEventListener?.(DEMO_CLOSE_EVENT, () => demo.requestClose());
  return { mode: 'demo', ...demo };
}

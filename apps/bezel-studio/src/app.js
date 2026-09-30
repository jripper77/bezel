// Bezel Studio: wires the store, the bridge and the views together.
import { applyTranslations, pickLocale, translator } from './i18n/index.js';
import { createBridge } from './bridge.js';
import { createStore } from './editor/store.js';
import { isHorizontal, isTurned, orientationOf } from './editor/geometry.js';
import { createCanvasView } from './ui/canvas.js';
import { createLibrary } from './ui/library.js';
import { createInspector } from './ui/inspector.js';
import { el } from './ui/dom.js';
import { shortcutFor } from './shortcuts.js';

const locale = pickLocale(navigator.languages ?? [navigator.language]);
const t = translator(locale);
const bridge = createBridge(window);
const $ = (id) => document.getElementById(id);

document.documentElement.lang = locale;
applyTranslations(document, t);

const state = {
  screens: [],
  screen: null,
  screenError: null,
  live: false,
  autostart: false,
  catalog: [],
  fonts: ['Inter', 'JetBrains Mono', 'Roboto', 'Roboto Mono'],
  assets: [],
  location: null,
};

function toast(message) {
  const box = $('toast');
  box.textContent = message;
  box.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { box.hidden = true; }, 3500);
}

const errorText = (e) => e?.message ?? String(e);

// ------------------------------------------------------------ store ----
const session = await bridge.session().catch(() => null);
// Without a session: a blank theme for the 8.8", horizontal like the backend's
// default for bar-shaped screens.
const store = createStore(session?.theme ?? { schema: 1, name: 'Untitled', canvas: { width: 1920, height: 480 }, orientation: 'landscape', refreshSeconds: 1, background: { type: 'color', color: '#0c0e16ff' }, elements: [] });
state.location = session?.location ?? null;

const canvasView = createCanvasView({
  store,
  scroll: $('stage-scroll'),
  box: $('canvas-box'),
  canvas: $('preview'),
  overlay: $('overlay'),
  onZoom: (z) => { $('zoom-label').textContent = `${Math.round(z * 100)}%`; },
  describe: (name) => t('stage.selected', { name }),
});

const library = createLibrary({
  store,
  canvas: canvasView,
  stage: $('stage'),
  t,
  actions: {
    openTheme: (location) => openTheme(location),
    newTheme: (axis) => newTheme(axis),
    refreshThemes: () => refreshThemes(),
    importTheme: () => importTheme(),
    addImage: () => addImage(),
    setBrightness: (screen, percent) => bridge.setBrightness(screen, percent).catch((e) => toast(t('toast.error', { message: errorText(e) }))),
    release: (screen) => bridge.release(screen).then(() => setLive(false)).catch((e) => toast(t('toast.error', { message: errorText(e) }))),
    setAutostart: (on) => bridge.setAutostart(on).then(() => { state.autostart = on; }).catch((e) => toast(t('toast.error', { message: errorText(e) }))),
    autostart: () => state.autostart,
  },
});

const inspector = createInspector({
  root: $('inspector'),
  store,
  t,
  sensors: { catalog: () => state.catalog, fonts: () => state.fonts },
});

// ----------------------------------------------------------- render ----
let rendering = false;
let pending = false;

async function renderNow() {
  if (rendering) {
    pending = true;
    return;
  }
  rendering = true;
  try {
    const frame = await bridge.render(store.getState().theme);
    canvasView.drawFrame(frame);
    $('status-render').textContent = t('status.render', { ms: Math.round(frame.millis) });
  } catch (e) {
    $('status-render').textContent = t('status.renderError', { message: errorText(e) });
  } finally {
    rendering = false;
    if (pending) {
      pending = false;
      requestAnimationFrame(renderNow);
    }
  }
}

let liveTimer = null;
function pushLive() {
  if (!state.live) return;
  clearTimeout(liveTimer);
  liveTimer = setTimeout(() => bridge.pushTheme(store.getState().theme).catch((e) => toast(t('toast.error', { message: errorText(e) }))), 150);
}

// The canvas is fitted again whenever the theme turns between vertical and
// horizontal (a button, the inspector, undo or another theme).
let shownAxis = null;

function refreshOrientation(theme) {
  const horizontal = isHorizontal(theme.orientation);
  $('orient-vertical').setAttribute('aria-pressed', String(!horizontal));
  $('orient-horizontal').setAttribute('aria-pressed', String(horizontal));
  $('orient-turn').setAttribute('aria-pressed', String(isTurned(theme.orientation)));
  const axis = horizontal ? 'horizontal' : 'vertical';
  if (shownAxis !== null && axis !== shownAxis) canvasView.fit();
  shownAxis = axis;
}

function refreshChrome(reason) {
  const { theme } = store.getState();
  $('undo').disabled = !store.canUndo();
  $('redo').disabled = !store.canRedo();
  if (document.activeElement !== $('theme-name')) $('theme-name').value = theme.name;
  $('save').classList.toggle('dirty', store.isDirty());
  $('status-main').textContent = store.isDirty() ? t('status.unsaved') : t('status.saved');
  canvasView.setCanvasSize(theme.canvas);
  refreshOrientation(theme);
  canvasView.drawOverlay();
  library.renderLayers();
  inspector.render(state.assets);
  if (reason !== 'select') {
    renderNow();
    pushLive();
  }
}

store.subscribe((_, reason) => refreshChrome(reason));

// Clock and sensor elements change every refresh even without edits.
function scheduleTick() {
  const seconds = Math.max(0.25, store.getState().theme.refreshSeconds || 1);
  setTimeout(() => {
    renderNow();
    scheduleTick();
  }, seconds * 1000);
}

// ---------------------------------------------------------- sensors ----
async function loadCatalog() {
  try {
    state.catalog = await bridge.catalog();
    library.setCatalog(state.catalog);
  } catch (e) {
    toast(t('toast.error', { message: errorText(e) }));
  }
}

async function sampleLoop() {
  try {
    const s = await bridge.sample();
    library.updateReadings(s.readings);
    syncLive(s);
    $('status-sensors').textContent = t('status.sensors', { ms: Math.round(s.sampleMillis) });
  } catch {
    $('status-sensors').textContent = t('status.sensorsError');
  }
  setTimeout(sampleLoop, 1000);
}

// ---------------------------------------------------------- screens ----
function renderScreenSelect() {
  const select = $('screen-select');
  const options = state.screens.map((s) => {
    const model = s.models.length === 1 ? s.models[0] : null;
    return el('option', { value: s.key, text: model ? `${model.name} · ${model.width}×${model.height}` : s.key, selected: s.key === state.screen });
  });
  if (options.length === 0) options.push(el('option', { value: '', text: t('top.noScreen') }));
  select.replaceChildren(...options);
  const current = state.screens.find((s) => s.key === state.screen);
  $('screen-dot').className = `dot${state.live ? ' live' : current?.state === 'awake' ? ' awake' : ''}`;
  let device = t('top.noScreen');
  if (state.screenError) device = t('status.devicesError', { message: state.screenError });
  else if (current) device = state.live ? t('status.live') : t(`screen.state.${current.state}`);
  $('status-device').textContent = device;
  library.renderScreen(state.screens, state.screen, state.live);
}

async function refreshScreens() {
  try {
    state.screens = await bridge.listScreens();
    state.screenError = null;
  } catch (e) {
    state.screens = [];
    state.screenError = errorText(e);
  }
  if (!state.screens.some((s) => s.key === state.screen)) state.screen = state.screens[0]?.key ?? null;
  renderScreenSelect();
}

// The backend owns live mode: it restores it at start and stops it when the
// screen fails; the switch follows what each sample reports.
function syncLive(s) {
  const live = Boolean(s.live);
  if (live === state.live && (!live || s.live === state.screen)) return;
  if (!live && state.live && s.liveError) toast(t('toast.liveStopped', { message: s.liveError }));
  state.live = live;
  if (live) state.screen = s.live;
  $('live').checked = live;
  renderScreenSelect();
}

async function setLive(on) {
  if (on && !state.screen) {
    toast(t('toast.noScreen'));
    $('live').checked = false;
    return;
  }
  try {
    await bridge.setLive(on, state.screen);
    state.live = on;
    if (on) await bridge.pushTheme(store.getState().theme);
  } catch (e) {
    state.live = false;
    toast(t('toast.error', { message: errorText(e) }));
  }
  $('live').checked = state.live;
  renderScreenSelect();
}

$('screen-select').addEventListener('change', (evt) => {
  state.screen = evt.target.value || null;
  if (state.live) setLive(true);
  renderScreenSelect();
});
$('live').addEventListener('change', (evt) => setLive(evt.target.checked));

// ----------------------------------------------------------- themes ----
async function refreshThemes() {
  try {
    library.renderThemes(await bridge.listThemes());
  } catch (e) {
    toast(t('toast.error', { message: errorText(e) }));
  }
}

async function refreshAssets() {
  try {
    state.assets = await bridge.assets();
    library.renderMedia(state.assets);
  } catch (e) {
    toast(t('toast.error', { message: errorText(e) }));
  }
}

async function openTheme(location) {
  try {
    const theme = await bridge.openTheme(location);
    state.location = location;
    library.showImportReport(null);
    store.load(theme);
    await refreshAssets();
    canvasView.fit();
    toast(t('toast.opened', { name: theme.name }));
  } catch (e) {
    toast(t('toast.error', { message: errorText(e) }));
  }
}

// A new theme keeps the 180° turn of the edited one: it follows how the
// screen is mounted.
async function newTheme(axis) {
  try {
    const orientation = orientationOf(axis, isTurned(store.getState().theme.orientation));
    store.load(await bridge.newTheme(state.screen, t('themes.untitled'), orientation));
    state.location = null;
    library.showImportReport(null);
    await refreshAssets();
    canvasView.fit();
  } catch (e) {
    toast(t('toast.error', { message: errorText(e) }));
  }
}

async function importTheme() {
  try {
    const result = await bridge.importTheme();
    if (!result) return;
    store.load(result.theme);
    state.location = null;
    await refreshAssets();
    canvasView.fit();
    const warnings = result.warnings ?? [];
    library.showImportReport({ name: result.theme.name, warnings });
    if (warnings.length === 0) toast(t('toast.imported'));
    else toast(warnings.length === 1 ? t('toast.importedWithOneWarning') : t('toast.importedWithWarnings', { count: warnings.length }));
  } catch (e) {
    toast(t('toast.error', { message: errorText(e) }));
  }
}

async function addImage() {
  try {
    const added = await bridge.addImage();
    if (added) await refreshAssets();
  } catch (e) {
    toast(t('toast.error', { message: errorText(e) }));
  }
}

async function save(saveAs = false) {
  try {
    const saved = await bridge.saveTheme(store.getState().theme, saveAs);
    if (!saved) return;
    state.location = saved.location;
    store.markSaved();
    toast(t('toast.saved'));
    refreshThemes();
  } catch (e) {
    toast(t('toast.error', { message: errorText(e) }));
  }
}

// ---------------------------------------------------------- chrome -----
$('undo').addEventListener('click', () => store.undo());
$('redo').addEventListener('click', () => store.redo());
$('save').addEventListener('click', () => save());
$('zoom-in').addEventListener('click', () => canvasView.setZoom(canvasView.zoom() * 1.25));
$('zoom-out').addEventListener('click', () => canvasView.setZoom(canvasView.zoom() / 1.25));
$('zoom-fit').addEventListener('click', () => canvasView.fit());
// Vertical | Horizontal keep the 180° turn; the turn keeps the axis.
function turnTheme(axis, turned) {
  store.dispatch('setOrientation', { orientation: orientationOf(axis, turned) });
}
const orientation = () => store.getState().theme.orientation;
$('orient-vertical').addEventListener('click', () => turnTheme('vertical', isTurned(orientation())));
$('orient-horizontal').addEventListener('click', () => turnTheme('horizontal', isTurned(orientation())));
$('orient-turn').addEventListener('click', () => turnTheme(isHorizontal(orientation()) ? 'horizontal' : 'vertical', !isTurned(orientation())));
$('theme-name').addEventListener('change', (evt) => {
  const name = evt.target.value.trim();
  if (name) store.dispatch('setTheme', { patch: { name } });
});

document.addEventListener('keydown', (evt) => {
  const action = shortcutFor(evt, document.activeElement);
  if (!action) return;
  evt.preventDefault();
  const ids = store.getState().selection;
  switch (action.type) {
    case 'undo': store.undo(); break;
    case 'redo': store.redo(); break;
    case 'save': save(); break;
    case 'saveAs': save(true); break;
    case 'remove': if (ids.length) store.dispatch('remove', { ids }); break;
    case 'duplicate': if (ids.length) store.dispatch('duplicate', { ids }); break;
    case 'selectAll': store.select(store.getState().theme.elements.map((e) => e.id)); break;
    case 'deselect': store.select([]); break;
    case 'nudge': if (ids.length) store.dispatch('move', { ids, dx: action.dx, dy: action.dy }); break;
    default: break;
  }
});

// ------------------------------------------------------------ start ----
library.renderWidgets();
refreshChrome('load');
canvasView.fit();
await Promise.all([loadCatalog(), refreshScreens(), refreshThemes(), refreshAssets()]);
bridge.getAutostart().then((on) => { state.autostart = on; renderScreenSelect(); }).catch(() => {});
bridge.fonts().then((f) => { if (f?.length) state.fonts = f; }).catch(() => {});
inspector.render(state.assets);
sampleLoop();
scheduleTick();
setInterval(refreshScreens, 5000);

// Bezel Studio: wires the store, the bridge and the views together.
import { applyTranslations, pickLocale, translator } from './i18n/index.js';
import { createBridge } from './bridge.js';
import { createStore } from './editor/store.js';
import { isHorizontal, isTurned, orientationOf } from './editor/geometry.js';
import { createCanvasView } from './ui/canvas.js';
import { createLibrary } from './ui/library.js';
import { createInspector } from './ui/inspector.js';
import { createConfirm, createStoragePanel, formatBytes, wireSubtabs } from './ui/storage.js';
import { el, icon } from './ui/dom.js';
import { ICONS } from './ui/icons.js';
import { askChoice } from './ui/dialog.js';
import { createPreferences } from './ui/preferences.js';
import { showAccessHelp } from './ui/udev.js';
import { shortcutFor } from './shortcuts.js';
import { createRenderScheduler } from './render-scheduler.js';
import { createPreviewAnimation } from './preview-animation.js';
import { errorText, sensorLabel } from './messages.js';
import { backgroundOf, droppable, fileNameOf, videoFacts } from './editor/background.js';
import { framingOf, framingPercents } from './editor/video-framing.js';

const bridge = createBridge(window);
const $ = (id) => document.getElementById(id);

// The language: the one chosen in the preferences, else the system's. It can
// change while the app runs, so every module translates through `t`.
const prefs = await bridge.preferences().catch(() => null);
let locale = prefs?.language ?? prefs?.systemLanguage ?? pickLocale(navigator.languages ?? [navigator.language]);
let current = translator(locale);
const t = (key, params) => current(key, params);
t.has = (key) => current.has(key);

document.documentElement.lang = locale;
applyTranslations(document, t);

const state = {
  screens: [],
  // Panels the vendor app left in desktop mode (not screens).
  desktopMode: [],
  screen: null,
  screenError: null,
  live: false,
  autostart: false,
  catalog: [],
  fonts: ['Inter', 'JetBrains Mono', 'Roboto', 'Roboto Mono'],
  assets: [],
  location: null,
  // How the theme's video background reaches the live screen.
  liveVideo: null,
  // The brightness set on each screen in this session (percent, by key):
  // what the slider shows and what the boot media is set with.
  brightness: {},
  // The screen restarting now, and the one that stopped responding (its
  // card offers the restart; D-2026-09-30-release-polish-13), by key.
  restarting: null,
  hung: null,
  // Whether ffmpeg can take posters and convert videos (`media_tools`).
  mediaTools: null,
  // A file being added from the Media panel or a drop.
  addingMedia: false,
  // The live screen's link failed and the backend connects it again
  // (`{attempt, attempts}`), else null.
  reconnecting: null,
  // What Auto turns the video background (`video_auto`), for the theme,
  // video and screen named by `key`; `value` is null until it is known.
  auto: { key: null, value: null },
};

function toast(message) {
  const box = $('toast');
  box.textContent = message;
  box.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => { box.hidden = true; }, 3500);
}

/**
 * Shows why a command failed; a port the system denied comes with the
 * command that fixes it (Linux), in a dialog.
 */
function fail(e) {
  if (e?.udevCommand) showAccessHelp(t, e, toast);
  else toast(errorText(t, e));
}

// ------------------------------------------------------------ store ----
const session = await bridge.session().catch(() => null);
// Without a session: a blank theme for the 8.8", horizontal like the backend's
// default for bar-shaped screens.
// New elements and copies are named in the UI's language.
const names = { widget: (widget) => t(`widget.${widget}`), copy: (name) => t('layers.copyOf', { name }) };
const store = createStore(session?.theme ?? { schema: 1, name: t('themes.untitled'), canvas: { width: 1920, height: 480 }, orientation: 'landscape', refreshSeconds: 1, background: { type: 'color', color: '#0c0e16ff' }, elements: [] }, { names });
state.location = session?.location ?? null;
// The fastest refresh a theme may ask for comes from the backend (the
// core's); without a backend nothing refreshes faster than once a second.
const minRefresh = () => session?.minRefreshSeconds ?? 1;

const canvasView = createCanvasView({
  store,
  scroll: $('stage-scroll'),
  box: $('canvas-box'),
  canvas: $('preview'),
  overlay: $('overlay'),
  onZoom: (z) => { $('zoom-label').textContent = `${Math.round(z * 100)}%`; },
  describe: (name) => t('stage.selected', { name }),
  onFrameRequest: () => setFramingMode(true),
});

const library = createLibrary({
  store,
  canvas: canvasView,
  stage: $('stage'),
  t,
  locale: () => locale,
  themeFilter: prefs?.themeFilter ?? null,
  actions: {
    openTheme: (location) => openTheme(location),
    newTheme: (axis) => newTheme(axis),
    refreshThemes: () => refreshThemes(),
    importTheme: () => importTheme(),
    addImage: () => addImage(),
    addVideo: () => addMedia(),
    setBrightness: (screen, percent) => bridge.setBrightness(screen, percent).then(() => { state.brightness[screen] = percent; }).catch((e) => fail(e)),
    release: (screen) => bridge.release(screen).then(() => setLive(false)).catch((e) => fail(e)),
    setAutostart: (on) => bridge.setAutostart(on).then(() => { state.autostart = on; }).catch((e) => fail(e)),
    autostart: () => state.autostart,
    leaveDesktopMode: (panel) => leaveDesktopMode(panel),
    showSensors: (keys) => bridge.showSensors(keys).catch((e) => fail(e)),
    restart: (screen) => restartScreen(screen),
    themeThumbnail: (location) => bridge.themeThumbnail(location),
    rememberThemeFilter: ({ scope, axis }) => bridge.setThemeFilter(scope, axis).catch(() => {}),
  },
});

const storage = createStoragePanel({
  root: $('storage-panel'),
  t,
  locale: () => locale,
  bridge,
  notify: toast,
  context: () => ({
    screen: state.screens.find((s) => s.key === state.screen) ?? null,
    live: state.live,
    liveVideo: state.liveVideo,
    brightness: state.brightness,
  }),
  restart: (screen) => restartScreen(screen),
});
// The Storage tab is the storage manager: shown, it takes the whole width of
// the window (D-2026-09-30-storage-manager-4); the editor comes back with
// any other tab.
const workspace = document.querySelector('.workspace');
const storageWide = () => workspace.classList.contains('storage-wide');
function syncStorageWide() {
  const wide = !$('panel-screen').hidden && !$('screen-storage').hidden;
  if (wide === storageWide()) return;
  workspace.classList.toggle('storage-wide', wide);
  if (wide) {
    setFramingMode(false, { restoreFocus: false });
    storage.show();
  } else {
    storage.hide();
    canvasView.fit();
  }
}
const panelWatch = new MutationObserver(syncStorageWide);
for (const id of ['panel-screen', 'screen-storage']) panelWatch.observe($(id), { attributes: true, attributeFilter: ['hidden'] });
wireSubtabs(document.querySelector('#panel-screen .subtabs'), syncStorageWide);

// The preview moves (animated GIFs, a video background) only while the
// window is shown and motion is not reduced.
const reducedMotion = window.matchMedia?.('(prefers-reduced-motion: reduce)');
const motionAllowed = () => !document.hidden && !reducedMotion?.matches;

const inspector = createInspector({
  root: $('inspector'),
  store,
  t,
  sensors: { catalog: () => labelledCatalog(), fonts: () => state.fonts },
  minRefresh,
  video: {
    context: () => ({
      screen: currentScreen(), live: state.live, liveVideo: state.liveVideo, tools: state.mediaTools, locale,
      auto: state.auto.value, framing: canvasView.framing(), motion: !reducedMotion?.matches,
    }),
    useVideo: () => addMedia(null, { asBackground: true }),
    useImage: () => useImage(),
    openStorage: () => openStorage(),
    setFraming: (on) => setFramingMode(on),
    openGuide: () => bridge.openGuide('ffmpeg', locale).catch((e) => fail(e)),
  },
});

// ---------------------------------------------------------- framing ----
// "Frame on canvas" (D-2026-10-01-video-background-framing-6): the editing
// area frames the video background; the bar over it says how, and a live
// region reads the zoom and position out once each change settles.
function framingReadout() {
  return t('framing.readout', framingPercents(framingOf(store.getState().theme.background)));
}

function updateFramingReadout() {
  const on = canvasView.framing();
  const text = on ? framingReadout() : '';
  $('framing-numbers').textContent = text;
  if (!store.isGesturing() && $('framing-live').textContent !== text) $('framing-live').textContent = text;
}

/**
 * Framing mode on or off: on only for a video background, with no element
 * selected (none can be meanwhile); off gives the focus back to the
 * inspector's toggle unless something else took it.
 */
function setFramingMode(on, { restoreFocus = true } = {}) {
  const next = Boolean(on) && store.getState().theme.background.type === 'video' && !storageWide();
  if (next === canvasView.framing()) return;
  if (next && store.getState().selection.length) store.select([]);
  canvasView.setFraming(next ? { auto: () => state.auto.value, label: t('framing.surface'), describedBy: 'framing-keys', onLeave: () => setFramingMode(false) } : null);
  $('framing-hud').hidden = !next;
  $('stage').classList.toggle('framing', next);
  updateFramingReadout();
  inspector.contextChanged();
  if (next) canvasView.focusFraming();
  else if (restoreFocus) $('framing-canvas')?.focus();
}

$('framing-done').addEventListener('click', () => setFramingMode(false));
// A selection, another background or another theme ends framing mode.
store.subscribe((s, reason) => {
  if (!canvasView.framing()) return;
  if (reason === 'load' || s.selection.length || s.theme.background.type !== 'video') setFramingMode(false, { restoreFocus: false });
});

/** What identifies Auto's answer: the theme's video, its turn and size, and the live screen. */
function autoKey(theme) {
  const bg = theme.background;
  if (bg.type !== 'video') return null;
  return JSON.stringify([bg.asset, theme.orientation, theme.canvas.width, theme.canvas.height, state.live ? state.screen : null]);
}

/** Asks the backend what Auto is when the video, the theme's turn or the live screen changed. */
async function refreshAuto() {
  const theme = store.getState().theme;
  const key = autoKey(theme);
  if (key === state.auto.key) return;
  state.auto = { key, value: null };
  if (key === null) return;
  const value = await bridge.videoAuto(theme).catch(() => null);
  if (state.auto.key !== key) return;
  state.auto = { key, value };
  inspector.contextChanged();
  canvasView.drawOverlay();
}

// ----------------------------------------------------------- render ----
// Animated GIFs and a video background move in the preview at their own
// pace (T-7.11, D-2026-10-01-video-background-framing-5), at most 15 frames
// a second; not while the window is hidden or motion is reduced.
const animation = createPreviewAnimation({
  request: () => renderNow(),
  enabled: motionAllowed,
});

async function drawPreview() {
  const started = performance.now();
  try {
    const frame = await bridge.render(store.getState().theme, { motion: motionAllowed() });
    canvasView.drawFrame(frame);
    $('status-render').textContent = t('status.render', { ms: Math.round(frame.millis) });
    animation.shown({ nextMs: frame.nextMs ?? null, elapsed: performance.now() - started });
  } catch (e) {
    animation.stop();
    $('status-render').textContent = t('status.renderError', { message: errorText(t, e) });
  }
}

// One render at a time, at most 30 a second while dragging, the last exact.
const previews = createRenderScheduler({ render: drawPreview, gesturing: () => store.isGesturing() });
const renderNow = () => previews.request();
document.addEventListener('visibilitychange', () => {
  if (!document.hidden) renderNow();
});
reducedMotion?.addEventListener?.('change', () => {
  inspector.contextChanged();
  renderNow();
});

let liveTimer = null;
function pushLive() {
  if (!state.live) return;
  clearTimeout(liveTimer);
  liveTimer = setTimeout(() => bridge.pushTheme(store.getState().theme).catch((e) => fail(e)), 150);
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

// The app asks before closing the window over unsaved edits: it learns
// whether there are any whenever that changes.
let reportedUnsaved = null;

function reportUnsaved() {
  const unsaved = store.isDirty();
  if (unsaved === reportedUnsaved) return;
  reportedUnsaved = unsaved;
  bridge.setUnsaved(unsaved).catch(() => { reportedUnsaved = null; });
}

function refreshChrome(reason) {
  const { theme } = store.getState();
  reportUnsaved();
  $('undo').disabled = !store.canUndo();
  $('redo').disabled = !store.canRedo();
  if (document.activeElement !== $('theme-name')) $('theme-name').value = theme.name;
  $('save').classList.toggle('dirty', store.isDirty());
  $('status-main').textContent = store.isDirty() ? t('status.unsaved') : t('status.saved');
  canvasView.setCanvasSize(theme.canvas);
  refreshOrientation(theme);
  canvasView.drawOverlay();
  library.renderLayers();
  void refreshAuto();
  inspector.render(state.assets);
  updateFramingReadout();
  if (reason !== 'select') {
    renderNow();
    pushLive();
  }
}

store.subscribe((_, reason) => refreshChrome(reason));

// Clock and sensor elements change every refresh even without edits.
function scheduleTick() {
  const seconds = Math.max(minRefresh(), store.getState().theme.refreshSeconds || 1);
  setTimeout(() => {
    renderNow();
    scheduleTick();
  }, seconds * 1000);
}

// ---------------------------------------------------------- sensors ----
/** The catalog with each sensor's name in the UI's language. */
function labelledCatalog() {
  return state.catalog.map((s) => ({ ...s, label: sensorLabel(t, s) }));
}

async function loadCatalog() {
  try {
    state.catalog = await bridge.catalog();
    library.setCatalog(labelledCatalog());
  } catch (e) {
    fail(e);
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
const currentScreen = () => state.screens.find((s) => s.key === state.screen) ?? null;

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
  if (state.screenError) device = t('status.devicesError', { message: errorText(t, state.screenError) });
  else if (state.restarting) device = t('restart.running');
  else if (state.reconnecting) device = t('restart.reconnecting', state.reconnecting);
  else if (current && state.live && state.liveVideo?.state === 'missing') device = t('status.liveVideoMissing');
  else if (current) device = state.live ? t('status.live') : t(`screen.state.${current.state}`);
  $('status-device').textContent = device;
  library.renderScreen(state.screens, state.screen, state.live, state.brightness, state.desktopMode, { restarting: state.restarting, hung: state.hung });
  storage.update();
  inspector.contextChanged();
  void refreshAuto();
}

async function refreshScreens() {
  try {
    const found = await bridge.listDevices();
    state.screens = found.screens;
    state.desktopMode = found.desktopMode ?? [];
    state.screenError = null;
  } catch (e) {
    state.screens = [];
    state.desktopMode = [];
    state.screenError = e;
  }
  if (!state.screens.some((s) => s.key === state.screen)) state.screen = state.screens[0]?.key ?? null;
  renderScreenSelect();
  // ffmpeg may have been installed or located since the video was added.
  const bg = store.getState().theme.background;
  if (bg.type === 'video' && !bg.poster) refreshTools();
}

// The backend owns live mode: it restores it at start, connects a screen
// whose link failed again (T-7.11) and stops it when the screen does not come
// back; the switch and the status follow what each sample reports.
function syncLive(s) {
  const live = Boolean(s.live);
  const video = s.video ?? null;
  const reconnecting = live ? (s.reconnecting ?? null) : null;
  if (reconnecting?.attempt !== state.reconnecting?.attempt) {
    if (state.reconnecting && !reconnecting && live) toast(t('restart.doneLive'));
    state.reconnecting = reconnecting;
    renderScreenSelect();
  }
  if ((video?.state ?? null) !== (state.liveVideo?.state ?? null)) {
    state.liveVideo = video;
    renderScreenSelect();
  }
  if (live === state.live && (!live || s.live === state.screen)) return;
  if (!live && state.live && s.liveError) {
    toast(t('toast.liveStopped', { message: errorText(t, s.liveError) }));
    // A screen that hung: its card offers the restart.
    if (s.liveError.code === 'hung') state.hung = state.screen;
  }
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
    fail(e);
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

// A panel in desktop mode goes back to USB monitor mode only after a dialog
// that names it and says the switch is not validated on hardware
// (D-2026-09-30-release-polish-8).
const confirmAction = createConfirm(t);

async function leaveDesktopMode(panel) {
  const ok = await confirmAction({
    title: t('desktop.confirmTitle', { address: panel.key }),
    body: [
      el('p', { text: t('desktop.confirmBody') }),
      el('p', { class: 'dialog-warning' }, [icon(ICONS.warning, 18), el('span', { text: t('desktop.confirmRisk') })]),
    ],
    action: t('desktop.confirmAction'),
    danger: true,
  });
  if (!ok) return;
  try {
    const done = await bridge.leaveDesktopMode(panel.key, true);
    toast(done?.model ? t('desktop.doneModel', { model: done.model }) : t('desktop.done'));
  } catch (e) {
    fail(e);
  }
  await refreshScreens();
}

// A screen that stopped responding restarts through its wake chip, without a
// replug, after a dialog that says what stops (D-2026-09-30-release-polish-13).
// It comes back under a new key; a screen that was live is live again.
async function restartScreen(key) {
  if (state.restarting) return;
  const screen = state.screens.find((s) => s.key === key);
  const name = screen?.models.length === 1 ? screen.models[0].name : key;
  const ok = await confirmAction({
    title: t('restart.confirmTitle', { name }),
    body: [el('p', { text: t('restart.confirmBody') })],
    action: t('restart.confirmAction'),
  });
  if (!ok) return;
  state.restarting = key;
  renderScreenSelect();
  try {
    const done = await bridge.restartScreen(key);
    state.hung = null;
    if (done?.key) state.screen = done.key;
    state.live = Boolean(done?.live);
    $('live').checked = state.live;
    storage.afterRestart();
    toast(state.live ? t('restart.doneLive') : t('restart.done'));
  } catch (e) {
    fail(e);
  }
  state.restarting = null;
  await refreshScreens();
}

// ----------------------------------------------------------- themes ----
async function refreshThemes() {
  try {
    library.renderThemes(await bridge.listThemes());
  } catch (e) {
    fail(e);
  }
}

async function refreshAssets() {
  try {
    state.assets = await bridge.assets();
    library.renderMedia(state.assets);
    inspector.render(state.assets);
  } catch (e) {
    fail(e);
  }
}

/** Whether ffmpeg can take posters and convert videos, for the inspector. */
async function refreshTools() {
  state.mediaTools = await bridge.mediaTools().catch(() => null);
  inspector.contextChanged();
}

/**
 * Whether the edited theme may be replaced or closed: nothing is unsaved, or
 * the user saved it or chose to discard the edits.
 */
async function settleUnsaved() {
  if (!store.isDirty()) return true;
  const answer = await askChoice(t, {
    title: t('unsaved.title'),
    body: t('unsaved.body', { name: store.getState().theme.name }),
    actions: [
      { id: 'discard', label: t('unsaved.discard'), kind: 'danger' },
      { id: 'save', label: t('unsaved.save'), kind: 'primary' },
    ],
    initial: 'save',
  });
  if (answer === 'save') return save();
  return answer === 'discard';
}

async function openTheme(location) {
  if (!(await settleUnsaved())) return;
  try {
    const theme = await bridge.openTheme(location);
    state.location = location;
    library.showImportReport(null);
    store.load(theme);
    await refreshAssets();
    canvasView.fit();
    toast(t('toast.opened', { name: theme.name }));
  } catch (e) {
    fail(e);
  }
}

// A new theme keeps the 180° turn of the edited one: it follows how the
// screen is mounted.
async function newTheme(axis) {
  if (!(await settleUnsaved())) return;
  try {
    const orientation = orientationOf(axis, isTurned(store.getState().theme.orientation));
    store.load(await bridge.newTheme(state.screen, t('themes.untitled'), orientation));
    state.location = null;
    library.showImportReport(null);
    await refreshAssets();
    canvasView.fit();
  } catch (e) {
    fail(e);
  }
}

async function importTheme() {
  if (!(await settleUnsaved())) return;
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
    fail(e);
  }
}

async function addImage() {
  try {
    const added = await bridge.addImage();
    if (added) await refreshAssets();
  } catch (e) {
    fail(e);
  }
}

/** What adding `added` (an `add_media` answer) tells: its play time and size, or why it has no poster. */
function addedText(added) {
  const name = fileNameOf(added.ref);
  if (added.kind !== 'video') return t('toast.imageAdded', { name });
  if (!added.poster && added.posterError && added.posterError.code !== 'unsupported') {
    return t('toast.posterFailed', { name, message: errorText(t, added.posterError) });
  }
  const details = videoFacts(added, (n) => formatBytes(n, locale)).join(' · ');
  return t(added.poster || !added.posterError ? 'toast.videoAdded' : 'toast.videoAddedNoFfmpeg', { name, details });
}

/**
 * Adds a video, an animated GIF or a picture to the theme: the file dropped
 * (`source`), else one picked in the native dialog. With `asBackground` it
 * becomes the theme's background (one undo step); a poster may take a few
 * seconds, meanwhile the Media panel says so.
 */
async function addMedia(source = null, { asBackground = false } = {}) {
  if (state.addingMedia) return;
  state.addingMedia = true;
  library.setAdding(true);
  try {
    const added = await bridge.addMedia(source);
    if (!added) return;
    await refreshAssets();
    if (asBackground) store.dispatch('setTheme', { patch: { background: backgroundOf(added) } });
    if (added.kind === 'video' && !added.poster) await refreshTools();
    toast(addedText(added));
  } catch (e) {
    fail(e);
  } finally {
    state.addingMedia = false;
    library.setAdding(false);
  }
}

/** Picks a picture in the native dialog and makes it the background. */
async function useImage() {
  try {
    const added = await bridge.addImage();
    if (!added) return;
    await refreshAssets();
    store.dispatch('setTheme', { patch: { background: { type: 'image', asset: added.ref, fit: 'cover' } } });
  } catch (e) {
    fail(e);
  }
}

/** Shows the storage tab, where a video missing on the screen is sent. */
function openStorage() {
  library.selectTab('screen');
  $('subtab-storage').click();
  $('subtab-storage').focus();
}

// ------------------------------------------------------------ drops ----
/**
 * Where a file dropped at (x, y) goes: `canvas` (the editing area: it
 * becomes the background), `media` (the Media panel: it is added), or none.
 */
function dropTargetAt(x, y) {
  const hit = document.elementFromPoint(x, y);
  if (hit?.closest('#stage')) return 'canvas';
  if (hit?.closest('#panel-media') && !$('panel-media').hidden) return 'media';
  return null;
}

function markDropTarget(target) {
  $('stage').classList.toggle('drop-target', target === 'canvas');
  $('panel-media').classList.toggle('drop-target', target === 'media');
}

function dropFile(name, source, target) {
  markDropTarget(null);
  if (!target || !source) return;
  if (!droppable(name)) {
    toast(t('media.dropUnsupported'));
    return;
  }
  addMedia(source, { asBackground: target === 'canvas' });
}

// Files dropped from the system: Tauri reports their paths and the pointer
// (physical pixels).
bridge.onFileDrop?.((evt) => {
  const ratio = window.devicePixelRatio || 1;
  const target = evt.position ? dropTargetAt(evt.position.x / ratio, evt.position.y / ratio) : null;
  if (evt.type !== 'drop') {
    markDropTarget(evt.type === 'leave' ? null : target);
    return;
  }
  const path = evt.paths?.find(droppable) ?? evt.paths?.[0];
  if (path) dropFile(path, path, target);
  else markDropTarget(null);
});

// Files dropped in a browser (demo mode): the bridge turns them into sources.
for (const [id, target] of [['stage', 'canvas'], ['panel-media', 'media']]) {
  const zone = $(id);
  zone.addEventListener('dragover', (evt) => {
    if (!evt.dataTransfer?.types?.includes('Files')) return;
    evt.preventDefault();
    markDropTarget(target);
  });
  zone.addEventListener('dragleave', (evt) => {
    if (!zone.contains(evt.relatedTarget)) markDropTarget(null);
  });
  zone.addEventListener('drop', (evt) => {
    const file = evt.dataTransfer?.files?.[0];
    if (!file) return;
    evt.preventDefault();
    dropFile(file.name, bridge.fileSource?.(file), target);
  });
}

/** Saves the theme; `true` once it is saved. */
async function save(saveAs = false) {
  try {
    const saved = await bridge.saveTheme(store.getState().theme, saveAs);
    if (!saved) return false;
    state.location = saved.location;
    store.markSaved();
    toast(t('toast.saved'));
    refreshThemes();
    return true;
  } catch (e) {
    fail(e);
    return false;
  }
}

// The window's close button with unsaved edits (and no screen live).
bridge.onCloseRequested(async () => {
  if (await settleUnsaved()) await bridge.closeWindow().catch((e) => fail(e));
}).catch(() => {});

// The tray's Quit with unsaved edits: the window is shown, and the app ends
// once the edits are saved or discarded.
bridge.onQuitRequested(async () => {
  if (await settleUnsaved()) await bridge.quitApp().catch((e) => fail(e));
}).catch(() => {});

// ---------------------------------------------------------- chrome -----
// The editor's own controls bring it back from the storage manager.
for (const id of ['undo', 'redo', 'zoom-in', 'zoom-out', 'zoom-fit', 'orient-vertical', 'orient-horizontal', 'orient-turn']) {
  $(id).addEventListener('click', () => {
    if (storageWide()) $('subtab-device').click();
  });
}
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
  // Nothing acts behind a modal dialog, and Esc keeps closing it.
  if (document.querySelector('dialog[open]')) return;
  const action = shortcutFor(evt, document.activeElement);
  if (!action) return;
  // The storage manager hides the editor: only saving acts on the theme.
  if (storageWide() && action.type !== 'save' && action.type !== 'saveAs') return;
  // Framing the video, elements stay out of reach: only undo, redo and save act.
  if (canvasView.framing() && !['undo', 'redo', 'save', 'saveAs'].includes(action.type)) return;
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

// ----------------------------------------------------------- language --
/** Shows the whole UI in `next` (`pt-BR` or `en`). */
function setLocale(next) {
  locale = next;
  current = translator(next);
  document.documentElement.lang = next;
  applyTranslations(document, t);
  library.setCatalog(labelledCatalog());
  library.retranslate();
  storage.retranslate();
  if (canvasView.framing()) $('overlay').setAttribute('aria-label', t('framing.surface'));
  refreshChrome('select');
  renderScreenSelect();
  renderNow();
}

const preferences = createPreferences({ t, bridge, onLanguage: setLocale, notify: toast });
$('preferences').addEventListener('click', () => preferences.open());

// ------------------------------------------------------------ start ----
library.renderWidgets();
refreshChrome('load');
canvasView.fit();
await Promise.all([loadCatalog(), refreshScreens(), refreshThemes(), refreshAssets(), refreshTools()]);
bridge.getAutostart().then((on) => { state.autostart = on; renderScreenSelect(); }).catch(() => {});
bridge.fonts().then((f) => { if (f?.length) state.fonts = f; }).catch(() => {});
inspector.render(state.assets);
sampleLoop();
scheduleTick();
setInterval(refreshScreens, 5000);

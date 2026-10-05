// An in-memory backend for demo mode: a simulated screen, sensors that move,
// themes, media and an approximate renderer. Nothing here reaches hardware.
import { DEMO_BACK_FROM_DESKTOP, DEMO_LIBRARY, DEMO_LOCAL_FILES, DEMO_ORIGINALS, DEMO_ORIGINALS_FOLDER, DEMO_PANELS, DEMO_PICKED, DEMO_PICKED_VIDEO, DEMO_POSTER_URL, DEMO_STORAGE, DEMO_THEME_VIDEOS, DEMO_UDEV_COMMAND, SCENARIOS } from './demo-data.js';
import { demoFindings, demoPlanAcross, demoPlanRename, demoPlanRestore, demoRank, demoSameFile } from './demo-manager.js';
import { DEMO_THEME } from './demo-theme.js';
import { createDemoGifs } from './demo-gifs.js';
import { createDemoStandby } from './demo-standby.js';
import { DEMO_GIF_FRAME_MS, DEMO_VIDEO_LOOP_MS, renderApprox } from './demo-render.js';
import { isHorizontal } from './editor/geometry.js';
import { IMAGE_EXTENSIONS as PICTURES, droppable, extensionOf, fileNameOf } from './editor/background.js';
import { framingOf, isPlainFraming, pictureBox, resolvedRotation } from './editor/video-framing.js';
import { pickLocale } from './i18n/index.js';
import { liveScreenIn } from './live-screen.js';
import { AXES, SCOPES } from './theme-filter.js';

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

/** The screen stopped reading what was sent: its firmware hung (the `hung` scenario). */
export function demoHung() {
  const detail = 'it stopped reading what was sent (250 bytes still queued)';
  return Object.assign(new Error(`the screen stopped responding: ${detail}`), { code: 'hung', args: { detail } });
}

/**
 * How long until the demo's animated GIFs change at `ms` (null: the theme
 * shows none), like the backend's preview: every visible `*.gif` image.
 */
export function demoNextChange(theme, ms) {
  const gif = (theme?.elements ?? []).some((e) => e.visible !== false && e.kind?.type === 'image' && String(e.kind.asset).toLowerCase().endsWith('.gif'));
  return gif ? DEMO_GIF_FRAME_MS - (Math.floor(ms) % DEMO_GIF_FRAME_MS) : null;
}

/**
 * The demo models a theme fits (its canvas is the panel turned the theme's
 * way up, like the core's rule) and the diagonal of the screen it was made
 * for, when they all have the same.
 */
export function demoFits(theme) {
  const turned = isHorizontal(theme.orientation);
  const models = DEMO_PANELS.filter((p) => {
    const [width, height] = turned ? [p.height, p.width] : [p.width, p.height];
    return width === theme.canvas.width && height === theme.canvas.height;
  });
  const sizes = new Set(models.map((m) => m.diagonalHundredths));
  return { models: models.map((m) => m.id), diagonalHundredths: sizes.size === 1 ? models[0].diagonalHundredths : null };
}

/** `color` when it is a `#rgb[a]`/`#rrggbb[aa]` color, else `fallback`. */
const svgColor = (color, fallback) => (typeof color === 'string' && /^#[0-9a-f]{3,8}$/i.test(color) ? color : fallback);

/** One element of a demo thumbnail: shapes as they are, rings as rings, text and the rest as bars. */
function thumbnailPart(e) {
  const { x, y, width, height } = e.frame;
  const kind = e.kind ?? {};
  if (kind.type === 'shape') return `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${kind.radius ?? 0}" fill="${svgColor(kind.fill, '#1e293b')}"/>`;
  if (kind.type === 'ring') {
    const r = Math.max(1, Math.min(width, height) / 2 - (kind.thickness ?? 8) / 2);
    const arc = 2 * Math.PI * r;
    return `<circle cx="${x + width / 2}" cy="${y + height / 2}" r="${r}" fill="none" stroke="${svgColor(kind.track, '#ffffff26')}" stroke-width="${kind.thickness ?? 8}"/>`
      + `<circle cx="${x + width / 2}" cy="${y + height / 2}" r="${r}" fill="none" stroke="${svgColor(kind.fill, '#38bdf8')}" stroke-width="${kind.thickness ?? 8}" stroke-dasharray="${arc * 0.45} ${arc}"/>`;
  }
  const fill = kind.type === 'text' ? svgColor(kind.style?.paint, '#e2e8f0') : svgColor(kind.fill, '#94a3b8');
  return `<rect x="${x}" y="${y + height * 0.25}" width="${width * 0.8}" height="${height * 0.5}" rx="${height * 0.12}" fill="${fill}"/>`;
}

/** A demo theme's thumbnail: its background and elements as an SVG `data:` URL. */
export function demoThumbnail(theme) {
  const { width, height } = theme.canvas;
  const background = theme.background?.type === 'color' ? svgColor(theme.background.color, '#0c0e16') : '#10111a';
  const parts = (theme.elements ?? []).filter((e) => e.visible !== false && e.frame).map(thumbnailPart).join('');
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}"><rect width="${width}" height="${height}" fill="${background}"/>${parts}</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg).replace(/\(/g, '%28').replace(/\)/g, '%29')}`;
}

/** Samples a live screen that dropped (the `flaky` scenario) reports it away. */
export const DEMO_AWAY_SAMPLES = 2;

/** The fastest refresh a theme may ask for, seconds (the core's `MIN_REFRESH_SECONDS`). */
export const DEMO_MIN_REFRESH = 0.25;

/** Pause between two simulated progress reports, ms. */
export const DEMO_STEP_MS = 150;
const CONVERT_STEPS = 8;
const UPLOAD_STEPS = 16;
/** Largest file a rev C screen takes, bytes (25 MiB, D-2026-09-30-release-polish-12). */
export const DEMO_REV_C_CAP = 26_214_400;
/** Largest file a TUR_USB screen takes, bytes (the vendor's 120 MB). */
export const DEMO_USB_CAP = 120_000_000;
/** The default limit of the local copies of deleted files: 2 GiB (D-2026-09-30-storage-manager-6). */
export const DEMO_CACHE_LIMIT = 2 * 2 ** 30;
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

/** An asset name like the backend makes it: `Férias Praia.MP4` → `f-rias-praia.mp4`. */
export function demoAssetName(fileName) {
  const base = fileNameOf(fileName);
  const dot = base.lastIndexOf('.');
  const clean = (text) => text.toLowerCase().replace(/[^a-z0-9_]+/g, '-').replace(/^-+|-+$/g, '');
  const stem = clean(dot > 0 ? base.slice(0, dot) : base) || 'file';
  return dot > 0 ? `${stem}.${clean(base.slice(dot + 1))}` : stem;
}

/** Why a video the demo adds without ffmpeg has no poster (the backend's error). */
const NO_POSTER = Object.freeze({
  code: 'unsupported',
  args: { detail: 'Taking a poster from a video needs ffmpeg with libx264' },
  message: 'not supported: Taking a poster from a video needs ffmpeg with libx264',
});

/** Clockwise quarter turns from a theme orientation to the 8.8"'s panel. */
export function demoTurns(orientation) {
  return (2 + 4 - (QUARTERS[orientation] ?? 2)) % 4;
}

/** A theme video asset's name stem on a screen: `assets/Nebula Azul.mp4` → `nebula_azul`. */
function videoStem(asset) {
  const file = fileNameOf(asset);
  const stem = file.includes('.') ? file.slice(0, file.lastIndexOf('.')) : file;
  return demoSuggestName(`${stem}.mp4`, 'mp4').slice(0, -'.mp4'.length);
}

/**
 * The `_f` and 8 hex digits a re-framed video's name gets: FNV-1a over the
 * demo's canonical framing (Fill, 100 % and centered: none). The core keeps
 * its own canonical form (D-2026-10-01-video-background-framing-4).
 */
export function demoFramingSuffix(framing) {
  if (isPlainFraming(framing)) return '';
  const pad = framing.fit === 'contain' ? framing.padColor : '';
  const canonical = [framing.fit, Math.round(framing.zoom * 100), Math.round(framing.position.x * 1000), Math.round(framing.position.y * 1000), pad].join(';');
  let hash = 0x811c9dc5;
  for (const c of canonical) hash = Math.imul(hash ^ c.codePointAt(0), 0x01000193) >>> 0;
  return `_f${hash.toString(16).padStart(8, '0')}`;
}

/**
 * How the 8.8" gets a theme's video, given what Auto is (`video_auto`) and
 * the video's own size (`info`): the clockwise quarter turns in total (the
 * framing's rotation and the theme's to the panel), whether it is sent as it
 * is (no turn, Fill, 100 %, centered, already panel-native), and the name it
 * has there: the vendor's by the total turns (`dragon.mp4`, `nebula_90.mp4`),
 * plus `_f…` for another framing.
 */
export function demoThemeVideo(theme, auto = null, info = null) {
  const framing = framingOf(theme.background);
  const turns = (demoTurns(theme.orientation) + resolvedRotation(framing, auto) / 90) % 4;
  const asIs = turns === 0 && isPlainFraming(framing) && info?.width === NATIVE.width && info?.height === NATIVE.height;
  const name = `${videoStem(theme.background.asset)}${['', '_90', '_180', '_270'][turns]}${demoFramingSuffix(framing)}.mp4`;
  return { turns, asIs, name };
}

/** Where the 8.8" keeps a theme's video (`assets/nebula.mp4`, landscape: `nebula_90.mp4`); see `demoThemeVideo`. */
export function demoVideoName(theme, auto = null) {
  return demoThemeVideo(theme, auto).name;
}

/** Whether a screen file named `fileName` is one of `asset`'s: any turn, any framing. */
export function demoIsThemeVideo(asset, fileName) {
  const name = String(fileName).toLowerCase();
  const stem = videoStem(asset);
  if (!name.startsWith(stem) || !name.endsWith('.mp4')) return false;
  let rest = name.slice(stem.length, -'.mp4'.length);
  const turn = ['_90', '_180', '_270'].find((suffix) => rest.startsWith(suffix));
  if (turn) rest = rest.slice(turn.length);
  return rest === '' || /^_f[0-9a-f]{8}$/.test(rest);
}

/**
 * The panel (portrait form, like the catalog's) a theme's Auto is told
 * against: the live screen's model, else the catalog's panel the canvas is
 * turned from, else `null`.
 * @param {{canvas: {width: number, height: number}, orientation: string}} theme
 * @param {{width: number, height: number}|null} [livePanel]
 */
export function demoPanelFor(theme, livePanel = null) {
  if (livePanel) return { width: livePanel.width, height: livePanel.height };
  const { width, height } = theme.canvas;
  const [w, h] = isHorizontal(theme.orientation) ? [height, width] : [width, height];
  return DEMO_PANELS.some((p) => p.width === w && p.height === h) ? { width: w, height: h } : null;
}

/**
 * What Auto is for a theme's video of `size` on `panel`, like the backend's
 * `video_auto`: a panel-native video in a theme an odd number of quarter
 * turns from the panel is already turned for it and gets the turns that
 * cancel the theme's (a landscape theme on the 8.8": 270); else 0.
 */
export function demoVideoAuto(theme, size, panel) {
  if (theme?.background?.type !== 'video' || !size) return { rotation: 0, size: null };
  const turns = demoTurns(theme.orientation);
  const native = Boolean(panel) && size.width === panel.width && size.height === panel.height;
  return { rotation: native && turns % 2 === 1 ? (4 - turns) * 90 : 0, size: { width: size.width, height: size.height } };
}

/** Most pictures a second the preview's video decoder gives (the backend's PREVIEW_FPS). */
export const DEMO_VIDEO_FPS = 15;
/** A preview decoder no picture is asked of for this long ends, ms (D-2026-10-01-video-background-framing-5). */
export const DEMO_DECODER_IDLE_MS = 2000;
/** Where in its video the demo takes a poster, ms. */
export const DEMO_POSTER_MS = 2000;

/**
 * The preview's simulated video decoder: at most one, for one video. The
 * first picture asked for starts it, another video restarts it, and it ends
 * when no picture is asked for during `DEMO_DECODER_IDLE_MS` (a hidden
 * window, reduced motion). Pictures follow the clock, so a decoder that
 * starts again resumes where the video would be.
 * @param {{wait?: (fn: () => void, ms: number) => unknown, cancel?: (timer: unknown) => void, onState?: (state: 'running'|'stopped') => void}} [deps]
 */
export function createDemoDecoder({ wait = (fn, ms) => setTimeout(fn, ms), cancel = (timer) => clearTimeout(timer), onState = () => {} } = {}) {
  let playing = null;
  let idle = null;
  const epochs = new Map();
  function stop() {
    if (idle !== null) cancel(idle);
    idle = null;
    if (playing === null) return;
    playing = null;
    onState('stopped');
  }
  return {
    /** The picture of `asset` (a video `durationMs` long) due at `nowMs`: its time in the video and when the next is due. */
    picture(asset, durationMs, nowMs) {
      if (playing !== asset) {
        playing = asset;
        onState('running');
      }
      if (!epochs.has(asset)) epochs.set(asset, nowMs);
      if (idle !== null) cancel(idle);
      idle = wait(stop, DEMO_DECODER_IDLE_MS);
      const elapsed = nowMs - epochs.get(asset);
      const period = 1000 / DEMO_VIDEO_FPS;
      return { ms: elapsed % Math.max(1, durationMs), nextMs: Math.max(1, Math.ceil(period - (elapsed % period))) };
    },
    /** The video it decodes now, or `null`. */
    playing: () => playing,
    stop,
  };
}

/** The guide pages `open_guide` opens. */
export const DEMO_GUIDE_PAGES = Object.freeze(['ffmpeg', 'gifs-and-stickers']);

/**
 * Holds a job phase in the middle until it is let go: a test sees the job
 * in progress however slow its machine is. Each `letGo` lets one held phase
 * go on, now or when it comes; a cancel lets the waiting one go at once.
 * @param {boolean} holding false: nothing is ever held
 */
export function createDemoGate(holding) {
  let permits = 0;
  let waiting = null;
  const wake = () => {
    const go = waiting;
    waiting = null;
    go?.();
    return Boolean(go);
  };
  return {
    /** Waits here while holding, until let go. */
    async hold() {
      if (!holding) return;
      if (permits > 0) {
        permits -= 1;
        return;
      }
      await new Promise((resolve) => { waiting = resolve; });
    },
    /** Lets the held phase go on, or the next one to come. */
    letGo() {
      if (!wake()) permits += 1;
    },
    /** A cancel: the held phase goes on to see it. */
    cancel: wake,
  };
}

/** Steps of one file's upload in a manager job (move, copy, rename, restore). */
const MANAGER_STEPS = 8;

/** A stand-in for a content id (the core's SHA-256): the same source and size, the same id. */
export function demoContent(source, size) {
  let hash = 0x811c9dc5;
  for (const c of `${source}|${size}`) hash = Math.imul(hash ^ c.charCodeAt(0), 0x01000193) >>> 0;
  return hash.toString(16).padStart(8, '0').repeat(8);
}

/** A demo file's thumbnail: a gradient of its name's color, a play mark on a video. */
export function demoFileThumbnail(name, kind) {
  let hue = 0;
  for (const c of name.toLowerCase()) hue = (hue * 31 + c.charCodeAt(0)) % 360;
  const [w, h] = kind === 'video' ? [40, 160] : [160, 120];
  const mark = kind === 'video'
    ? `<path d="M14 68v24l16-12z" fill="#ffffffcc"/>`
    : `<circle cx="118" cy="34" r="14" fill="#fde68a"/><path d="M0 120 52 58l34 38 22-20 52 44z" fill="#0f172acc"/>`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">`
    + `<stop offset="0" stop-color="hsl(${hue},70%,30%)"/><stop offset="1" stop-color="hsl(${(hue + 70) % 360},80%,62%)"/></linearGradient></defs>`
    + `<rect width="${w}" height="${h}" fill="url(#g)"/>${mark}</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg).replace(/\(/g, '%28').replace(/\)/g, '%29')}`;
}

/**
 * A simulated screen storage: capacity, files, uploads with progress over
 * time and cancel, deletes, playback and the boot media, with the same
 * confirmations and refusals as the app; and the storage manager's catalog
 * of what Bezel sent, with local copies (D-2026-09-30-storage-manager-2,
 * -5): overview, cleanup findings, move/copy/rename/restore plans run one
 * file at a time, batch deletes, associating an original and the cache.
 * With `hold`, every phase of a job waits after its first step until
 * `letGo` (tests only).
 */
function createDemoStorage(chosen, { delay, now, live, isLive, theme, themes, screens, hold = false, hung = () => false, videoInfo = () => null, autoOf = () => null }) {
  const layout = chosen.storage ?? DEMO_STORAGE;
  const card = chosen.card !== false;
  const files = new Map(layout.files.filter(([p]) => card || !p.startsWith('sd/')));
  const locals = new Map(Object.entries(DEMO_LOCAL_FILES).map(([name, f]) => [`demo://${name}`, { name, ...f }]));
  const listeners = new Set();
  const pending = new Map();
  const plans = new Map();
  const tools = { ready: chosen.ffmpeg !== false, configured: null };
  let tickets = 0;
  let job = null;
  // What the last manager overview listed, like the studio's: thumbnails only for these files.
  let listedBy = { key: null, paths: new Set() };
  const gate = createDemoGate(hold);
  const state = { playback: null, boot: layout.boot ?? null, bootBrightness: null };
  // The catalog of the 8.8" model (the demo's only model with storage).
  const catalog = {
    limit: DEMO_CACHE_LIMIT,
    entries: (layout.catalog ?? []).map((e) => ({
      card: null, state: 'stored', localCopy: true, thumb: true, source: null, durationMs: null, resolution: null, ...e, content: demoContent(e.source ?? e.path, e.size),
    })),
  };

  // Errors like the app's: a code, its arguments and the English text.
  const errorOf = (code, message, args = {}) => Object.assign(new Error(message), { code, args });
  const refuse = (code, message, args = {}) => Promise.reject(errorOf(code, message, args));
  // A refusal like the preflight's (and like an upload whose conversion is still too large).
  const refused = (code, extra = {}) => ({ status: 'refused', code, message: code, mismatches: [], candidates: [], accepted: [], ...extra });
  const screenOf = (key) => screens().find((s) => s.key === key);
  const limited = (key) => screenOf(key)?.family === 'turing-usb';
  const capOf = (key) => (limited(key) ? DEMO_USB_CAP : DEMO_REV_C_CAP);
  const entry = (path, size = files.get(path) ?? null) => {
    const [medium, kind, name] = path.split('/');
    return { path, medium, kind, name, size };
  };
  const totalOf = (medium) => (medium === 'sd' ? layout.cardTotal : chosen.internalTotal ?? layout.internalTotal);
  function capacity(medium) {
    const sum = [...files].filter(([p]) => p.startsWith(`${medium}/`)).reduce((total, [, size]) => total + size, 0);
    const total = totalOf(medium);
    const used = sum + (layout.usedExtra?.[medium] ?? 0);
    return { total, used, free: total - used };
  }
  const nowSec = () => Math.floor(now());
  const cardNow = () => (card ? layout.cardTotal : null);

  // ------------------------------------------------------------ catalog --
  /** Whether `e` is on the media the screen shows now (its card only when inserted). */
  const shown = (e) => e.path.startsWith('internal/') || (card && e.card === cardNow());
  /** The entry at `path` on this screen now, not deleted. */
  const entryAt = (path) => catalog.entries.find((e) => e.state !== 'deleted' && e.path === path && shown(e)) ?? null;
  function record(next) {
    catalog.entries = catalog.entries.filter((e) => !(e.path === next.path && e.card === next.card));
    catalog.entries.push({ thumb: true, durationMs: null, resolution: null, source: null, ...next });
  }
  const setState = (path, value) => {
    const found = entryAt(path);
    if (found) found.state = value;
  };
  /** Copies whose every entry is deleted, oldest first, with their size. */
  function deletedCopies() {
    const seen = new Map();
    for (const e of catalog.entries.filter((x) => x.localCopy)) {
      const slot = seen.get(e.content) ?? { content: e.content, all: true, size: e.size, sentAt: 0 };
      slot.all &&= e.state === 'deleted';
      slot.sentAt = Math.max(slot.sentAt, e.sentAt);
      seen.set(e.content, slot);
    }
    return [...seen.values()].filter((c) => c.all).sort((a, b) => a.sentAt - b.sentAt);
  }
  const dropCopies = (contents) => catalog.entries.forEach((e) => {
    if (contents.has(e.content)) e.localCopy = false;
  });
  /** Drops copies of deleted files, oldest first, until they fit the limit. */
  function evict() {
    let total = deletedCopies().reduce((sum, c) => sum + c.size, 0);
    const gone = new Set();
    for (const c of deletedCopies()) {
      if (total <= catalog.limit) break;
      total -= c.size;
      gone.add(c.content);
    }
    dropCopies(gone);
  }
  function markDeleted(path) {
    setState(path, 'deleted');
    evict();
  }
  function cacheInfo() {
    const sizes = new Map(catalog.entries.filter((e) => e.localCopy).map((e) => [e.content, e.size]));
    const deleted = deletedCopies();
    const sum = (list) => list.reduce((total, size) => total + size, 0);
    return { copies: sizes.size, bytes: sum([...sizes.values()]), deletedCopies: deleted.length, deletedBytes: sum(deleted.map((c) => c.size)), limit: catalog.limit };
  }
  /** Listed entries become stored (pending ones stay), absent ones missing. */
  function reconcile() {
    for (const e of catalog.entries.filter((x) => x.state !== 'deleted' && shown(x))) {
      if (!files.has(e.path)) e.state = 'missing';
      else if (e.state !== 'pending') e.state = 'stored';
    }
  }
  /** The boot media Bezel set and every video a theme plays (D-2026-09-30-storage-manager-9). */
  function guarded() {
    const assets = themes().filter((t) => t?.background?.type === 'video').map((t) => t.background.asset);
    const isBoot = (path) => Boolean(state.boot) && demoSameFile(state.boot, path);
    const isThemeVideo = (path) => path.split('/')[1] === 'video' && assets.some((asset) => demoIsThemeVideo(asset, path.split('/')[2]));
    return { isBoot, isThemeVideo };
  }
  /** Sizes as listed: a TUR_USB screen tells none, the catalog's stand in (D-2026-09-30-storage-manager-11). */
  const listedSize = (key, path) => (limited(key) ? entryAt(path)?.size ?? null : files.get(path));
  const entryDto = (e) => (e ? { state: e.state, localCopy: e.localCopy, sentAt: e.sentAt, source: e.source, durationMs: e.durationMs, resolution: e.resolution } : null);

  function managedFiles(key) {
    reconcile();
    const listed = [...files.keys()].map((path) => ({ path, size: listedSize(key, path) }));
    const { isBoot, isThemeVideo } = guarded();
    const found = demoFindings(listed, entryAt, (p) => isBoot(p) || isThemeVideo(p));
    return listed.map(({ path, size }) => ({
      ...entry(path, size),
      entry: entryDto(entryAt(path)),
      finding: found.get(path) ?? null,
      protected: (isBoot(path) && 'boot') || (isThemeVideo(path) && 'themeVideo') || null,
    }));
  }
  /**
   * Cataloged files that can be sent again: missing here, or on another
   * card; then the ones deleted through Bezel whose copies it holds.
   */
  function restorable() {
    const away = (e) => e.state !== 'deleted' && ((shown(e) && e.state === 'missing') || (e.path.startsWith('sd/') && !shown(e)));
    const deleted = (e) => e.state === 'deleted' && e.localCopy;
    return [...catalog.entries.filter(away), ...catalog.entries.filter(deleted)].map((e) => ({
      ...entry(e.path, e.size), id: `${e.path}@${e.card ?? ''}`, sentAt: e.sentAt, localCopy: e.localCopy, otherCard: e.path.startsWith('sd/') && e.card !== cardNow(), state: e.state, content: e.content,
    }));
  }
  function planView(key) {
    const { isBoot, isThemeVideo } = guarded();
    const sized = new Map([...files.keys()].map((p) => [p, listedSize(key, p)]));
    const clash = (target) => {
      const path = [...files.keys()].find((p) => demoSameFile(p, target));
      return path ? entry(path, sized.get(path)) : null;
    };
    const copyOf = (path) => {
      const e = entryAt(path);
      const size = sized.get(path);
      return e && e.state !== 'pending' && e.localCopy && (size === null || size === e.size) ? e : null;
    };
    return { files: sized, card: cardNow(), deletes: !limited(key), clash, copyOf, isBoot, isThemeVideo };
  }
  /** A ready plan is kept under a ticket; what the UI gets has no content ids. */
  function keepPlan(key, plan) {
    if (plan.status !== 'ready') return plan;
    const ticket = (tickets += 1);
    plans.set(ticket, { key, plan });
    const steps = plan.steps.map(({ content, ...step }) => step);
    return { ...plan, ticket, steps, bytes: steps.reduce((sum, s) => sum + s.size, 0), free: capacity(plan.to).free };
  }
  /**
   * Why the manager cannot read or plan now (like the overview's refusals):
   * the error it answers with, or `null` when it can.
   */
  function whyBlocked(key, to = 'internal') {
    if (chosen.denied) return demoDenied(key);
    if (!screenOf(key)?.models.every((m) => m.capabilities.storage)) return errorOf('unsupported', 'not supported: no storage', { detail: 'no storage' });
    if (job) return errorOf('busy', 'a storage operation is using the screen');
    if (!['internal', 'sd'].includes(to)) return errorOf('unknownMedium', `unknown medium "${to}" (internal or sd)`, { medium: String(to) });
    return null;
  }
  /** `whyBlocked` as an answer: its rejection, or `null` when the manager can go on (`blocked(key) ?? answer`). */
  function blocked(key, to = 'internal') {
    const why = whyBlocked(key, to);
    return why ? Promise.reject(why) : null;
  }

  /**
   * How the live theme's video reaches the screen: its name there (Auto and
   * the framing included); a file sent as it is counts only with the
   * asset's exact bytes, a converted one when present
   * (D-2026-10-01-video-background-framing-4).
   */
  function videoOfTheme() {
    const current = theme();
    if (!live() || current.background?.type !== 'video') return null;
    const info = videoInfo(current.background.asset);
    const { name, asIs } = demoThemeVideo(current, autoOf(current), info);
    const holds = (p) => (asIs ? files.get(p) === info.bytes : files.get(p) > 0);
    const stored = ['internal', ...(card ? ['sd'] : [])].map((m) => `${m}/video/${name}`).find(holds);
    if (stored) return { state: 'onDevice', path: stored };
    return { state: 'missing', path: `${card ? 'sd' : 'internal'}/video/${name}` };
  }

  function readyAnswer(local, path, convert, cap) {
    const ticket = (tickets += 1);
    const replaces = files.get(path) > 0 ? entry(path) : null;
    pending.set(ticket, { path, bytes: local.size, convert, cap, convertedSize: local.convertedSize ?? null, storedShort: Boolean(local.storedShort), source: local.source ?? `/home/demo/${local.name}` });
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

  function check(local, medium, cap, name, turns = demoTurns(theme().orientation)) {
    const kind = demoKindOf(local.name);
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
    if (!needsConversion && local.size > cap) return refused('tooLarge', { bytes: local.size, limit: cap });
    const free = capacity(medium).free + (files.get(path) ?? 0);
    if (!needsConversion && local.size >= free) {
      const candidates = [...files.keys()].filter((p) => p.startsWith(`${medium}/`)).map((p) => entry(p)).sort((a, b) => b.size - a.size);
      return refused('noSpace', { bytes: local.size, limit: free, candidates });
    }
    const convert = needsConversion ? { ...NATIVE, quarterTurns: turns, cropped: local.width * NATIVE.height !== local.height * NATIVE.width } : null;
    return readyAnswer(local, path, convert, cap);
  }

  /** A progress report; a manager job's names the file it is at (`step`). */
  const emit = (phase, done, total, step = null) => listeners.forEach((cb) => cb({ phase, done: Math.round(done), total, ...(step ? { step } : {}) }));

  /** Reports `steps` steps of `phase`; `true` when cancelled meanwhile. */
  async function steps(phase, total, count, each = () => {}, step = null) {
    for (let i = 0; i <= count; i += 1) {
      each((total * i) / count);
      emit(phase, (total * i) / count, total, step);
      if (i === count) return false;
      if (i === 1) await gate.hold();
      await delay(DEMO_STEP_MS);
      if (job.cancelled) return true;
    }
    return false;
  }

  async function upload(p) {
    let size = p.bytes;
    if (p.convert) {
      if (await steps('convert', 20_000, CONVERT_STEPS)) return { status: 'cancelled', path: p.path, partial: null };
      size = p.convertedSize ?? Math.round(p.bytes * 0.6);
      // Still over the screen's limit once converted: refused before sending.
      if (size > p.cap) return refused('convertedTooLarge', { bytes: size, limit: p.cap });
    }
    // A hung screen stops reading in the middle of the upload.
    if (hung()) throw demoHung();
    // Recorded before the first byte, with the exact bytes sent (D-2026-09-30-storage-manager-5).
    record({ path: p.path, card: p.path.startsWith('sd/') ? cardNow() : null, size, content: demoContent(p.source, size), localCopy: true, sentAt: nowSec(), source: p.source, state: 'pending' });
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
    setState(p.path, 'stored');
    emit('verify', 1, 1);
    state.playback = null;
    return { status: 'done', file: entry(p.path, size), converted: Boolean(p.convert) };
  }

  /** The preflight of one step on its target: the per-file limit and the free space (D-2026-09-30-storage-manager-7). */
  function stepRefusal(key, step, to) {
    const cap = capOf(key);
    if (step.size > cap) return refused('tooLarge', { bytes: step.size, limit: cap });
    const free = capacity(to).free + (step.replaces ? files.get(step.replaces.path) ?? 0 : 0);
    if (step.size >= free) return refused('noSpace', { bytes: step.size, limit: free, candidates: [] });
    return null;
  }

  /** Why a step stopped, as the app reports it: the core's halt code and what it needs. */
  const halted = (halt, extra = {}) => ({ halt, error: null, refusal: null, conflict: null, ...extra });

  /**
   * One file of a plan: send its copy, check the stored size, and only then
   * delete the source (move, rename). `'done'`, `{cancelled, stage, partial}`
   * or `{failed}` (with its stage and halt code); the source stays unless done.
   */
  async function runStep(key, transfer, step, info) {
    const to = step.target.split('/')[0];
    const refusal = stepRefusal(key, step, to);
    if (refusal) return { failed: { stage: 'preflight', ...halted('refused', { refusal }) } };
    if (hung()) {
      const { code, args, message } = demoHung();
      return { failed: { stage: 'upload', ...halted('failed', { error: { code, args, message } }) } };
    }
    const from = transfer === 'restore' ? catalog.entries.find((e) => e.content === step.content) : entryAt(step.source);
    if (step.replaces) files.delete(step.replaces.path);
    record({ path: step.target, card: to === 'sd' ? cardNow() : null, size: step.size, content: step.content, localCopy: true, sentAt: nowSec(), source: from?.source ?? null, durationMs: from?.durationMs ?? null, resolution: from?.resolution ?? null, state: 'pending' });
    const cancelled = await steps('upload', step.size, MANAGER_STEPS, (done) => files.set(step.target, Math.round(done)), info);
    if (cancelled) {
      const partial = files.get(step.target) || null;
      if (!partial) files.delete(step.target);
      return { cancelled: true, stage: 'upload', partial };
    }
    emit('verify', 0, 1, info);
    await delay(DEMO_STEP_MS);
    setState(step.target, 'stored');
    emit('verify', 1, 1, info);
    if (transfer === 'move' || transfer === 'rename') {
      emit('delete', 0, 1, info);
      await delay(DEMO_STEP_MS);
      files.delete(step.source);
      catalog.entries = catalog.entries.filter((e) => e !== from);
      emit('delete', 1, 1, info);
    }
    return 'done';
  }

  async function runPlanSteps(key, plan) {
    const strip = ({ content, ...step }) => step;
    const report = { transfer: plan.transfer, done: [], failed: null, cancelled: null, notStarted: [] };
    const count = plan.steps.length;
    for (let i = 0; i < count; i += 1) {
      const step = plan.steps[i];
      const rest = () => plan.steps.slice(i + 1).map(strip);
      if (job.cancelled) {
        report.notStarted = plan.steps.slice(i).map(strip);
        break;
      }
      const outcome = await runStep(key, plan.transfer, step, { index: i, count, source: step.source, target: step.target });
      if (outcome === 'done') {
        report.done.push(strip(step));
        continue;
      }
      if (outcome.cancelled) report.cancelled = { step: strip(step), stage: outcome.stage, partial: outcome.partial };
      else report.failed = { step: strip(step), ...outcome.failed };
      report.notStarted = rest();
      break;
    }
    state.playback = null;
    return { status: 'ran', ...report };
  }

  /**
   * A restore must still fit before its first byte (core `fits`): every
   * file within the per-file limit, their total below the free space. The
   * refusal, or `null`.
   */
  function restoreRefusal(key, plan) {
    if (plan.transfer !== 'restore' || !plan.steps.length) return null;
    const cap = capOf(key);
    const over = plan.steps.find((step) => step.size > cap);
    const refusal = (code, args) => ({ status: 'refused', code, args, message: code });
    if (over) return refusal('unsendable', { path: over.target, refusal: { code: 'tooLarge', bytes: over.size, limit: cap } });
    const needed = plan.steps.reduce((sum, step) => sum + step.size, 0);
    const { free } = capacity(plan.to);
    return needed >= free ? refusal('noSpace', { needed, free }) : null;
  }

  /** Runs `work` as the one storage job. */
  async function asJob(work) {
    job = { cancelled: false };
    try {
      return await work();
    } finally {
      job = null;
    }
  }

  /** Deletes the confirmed files `{path, size}` one by one; one gone or of another size stops the batch undeleted. */
  async function deleteEach(chosen) {
    const paths = chosen.map((c) => c.path);
    const report = { deleted: [], failed: null, cancelled: false, notStarted: [], freed: 0 };
    for (let i = 0; i < paths.length; i += 1) {
      const path = paths[i];
      if (job.cancelled) {
        Object.assign(report, { cancelled: true, notStarted: paths.slice(i) });
        break;
      }
      emit('delete', i, paths.length, { index: i, count: paths.length, source: path, target: null });
      await delay(DEMO_STEP_MS);
      if (!files.has(path) || files.get(path) !== chosen[i].size) {
        report.failed = { path, ...halted('sourceChanged') };
        report.notStarted = paths.slice(i + 1);
        break;
      }
      report.freed += files.get(path);
      files.delete(path);
      markDeleted(path);
      report.deleted.push(path);
    }
    return report;
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
    /** Both media with the catalog beside them (the storage manager's view). */
    managerOverview: (key) => {
      const why = whyBlocked(key);
      if (why) return Promise.reject(why);
      const listed = managedFiles(key);
      listedBy = { key, paths: new Set(listed.map((f) => f.path)) };
      return Promise.resolve({
        internal: capacity('internal'),
        card: card ? capacity('sd') : null,
        files: listed,
        folderErrors: [],
        restorable: restorable().map(({ content, ...r }) => r),
        deletes: !limited(key),
        cap: capOf(key),
        cache: cacheInfo(),
      });
    },
    /**
     * A file's thumbnail from its local copy (kept when the copy is cleared), else `null`; like the
     * studio's, only for a file the last manager overview of `key` listed.
     */
    managerThumbnail: (key, path) => {
      const found = listedBy.key === key && listedBy.paths.has(path) ? entryAt(path) : null;
      return Promise.resolve(found?.thumb ? demoFileThumbnail(found.path.split('/')[2], found.path.split('/')[1]) : null);
    },
    planMove: (key, paths, to, overwrite = []) => blocked(key, to) ?? Promise.resolve(keepPlan(key, demoPlanAcross(planView(key), 'move', paths, to, overwrite))),
    planCopy: (key, paths, to, overwrite = []) => blocked(key, to) ?? Promise.resolve(keepPlan(key, demoPlanAcross(planView(key), 'copy', paths, to, overwrite))),
    planRename: (key, path, newName, overwrite = []) => blocked(key) ?? Promise.resolve(keepPlan(key, demoPlanRename(planView(key), path, newName, overwrite))),
    planRestore: (key, ids, to, overwrite = []) => {
      const why = whyBlocked(key, to);
      if (why) return Promise.reject(why);
      const chosenEntries = restorable().filter((r) => ids.includes(r.id));
      const room = { free: capacity(to).free, cap: capOf(key) };
      return Promise.resolve(keepPlan(key, demoPlanRestore(planView(key), chosenEntries, to, room, overwrite)));
    },
    /**
     * Runs a plan one file at a time once its dialog was confirmed (`confirmed`; without it nothing
     * runs): `{status: 'ran', ...report}` says what was done, failed and never started; a restore
     * that no longer fits is `{status: 'refused', code, args}` before anything is sent.
     */
    runPlan: (ticket, confirmed) => {
      const held = plans.get(ticket);
      if (!held) return refuse('stale', 'this operation is no longer prepared');
      plans.delete(ticket);
      if (job) return refuse('busy', 'a storage operation is using the screen');
      if (!confirmed) {
        const verb = { move: 'moving', copy: 'copying', rename: 'renaming', restore: 'restoring' }[held.plan.transfer];
        const count = held.plan.steps.length;
        const detail = `${verb} ${count} file${count === 1 ? '' : 's'}`;
        return refuse('notConfirmed', `${detail} needs confirmation`, { detail });
      }
      const refusal = restoreRefusal(held.key, held.plan);
      if (refusal) return Promise.resolve(refusal);
      return asJob(() => runPlanSteps(held.key, held.plan));
    },
    /** Deletes the confirmed files `{path, size}` one by one (the cleanup's list, or a selection). */
    deleteFiles: (key, chosen, confirmed) => {
      if (limited(key)) return refuse('unsupported', 'not supported: deleting files', { detail: 'deleting files' });
      if (!confirmed) return refuse('notConfirmed', `deleting ${chosen.length} files needs confirmation`, { detail: `deleting ${chosen.length} files` });
      if (job) return refuse('busy', 'a storage operation is using the screen');
      return asJob(() => deleteEach(chosen));
    },
    /** The originals the demo's picker returns: files, or their folder. */
    pickOriginals: (folder) => Promise.resolve(folder ? [DEMO_ORIGINALS_FOLDER] : Object.keys(DEMO_ORIGINALS)),
    associateCandidates: (key, path, sources) => {
      const why = whyBlocked(key);
      if (why) return Promise.reject(why);
      const size = listedSize(key, path);
      if (!files.has(path) || size === null) return refuse('invalidInput', `invalid input: ${path} is not stored on the screen`, { detail: `${path} is not stored on the screen` });
      const known = (source) => (DEMO_ORIGINALS[source] ? [source] : Object.keys(DEMO_ORIGINALS).filter((p) => p.startsWith(`${source}/`)));
      const candidates = [...new Set(sources.flatMap(known))].map((source) => ({ source, name: source.split('/').pop(), ...DEMO_ORIGINALS[source] }));
      const kind = path.split('/')[1];
      const sought = { path, size, resolution: kind === 'video' ? { ...NATIVE } : null, durationMs: null };
      return Promise.resolve({ candidates: demoRank(sought, candidates) });
    },
    /** Copies a confirmed original into the store: the file gets a thumbnail and becomes movable. */
    associateOriginal: (key, path, source, confirmed) => {
      if (!confirmed) return refuse('notConfirmed', `associating ${path} needs confirmation`, { detail: `associating ${path}` });
      const why = whyBlocked(key);
      if (why) return Promise.reject(why);
      const original = DEMO_ORIGINALS[source];
      const size = listedSize(key, path);
      if (!original || original.size !== size || original.kind !== path.split('/')[1]) {
        const detail = `${source} is not the original of ${path}: another size or kind`;
        return refuse('invalidInput', `invalid input: ${detail}`, { detail });
      }
      record({ path, card: path.startsWith('sd/') ? cardNow() : null, size, content: demoContent(source, size), localCopy: true, sentAt: nowSec(), source, durationMs: original.durationMs, resolution: original.resolution, state: 'stored' });
      return Promise.resolve(managedFiles(key).find((f) => f.path === path));
    },
    cacheInfo: () => Promise.resolve(cacheInfo()),
    /** "Clear cache": the copies of deleted files (`all`: every copy); entries and thumbnails stay. */
    clearCache: (scope, confirmed) => {
      if (!['deleted', 'all'].includes(scope)) return refuse('invalidInput', `invalid input: cache scope "${scope}"`, { detail: `cache scope "${scope}"` });
      if (!confirmed) return refuse('notConfirmed', 'clearing the local copies needs confirmation', { detail: 'clearing the local copies' });
      const before = cacheInfo();
      const gone = scope === 'all' ? new Set(catalog.entries.map((e) => e.content)) : new Set(deletedCopies().map((c) => c.content));
      dropCopies(gone);
      const after = cacheInfo();
      return Promise.resolve({ removed: before.copies - after.copies, bytes: before.bytes - after.bytes });
    },
    setCacheLimit: (bytes) => {
      if (!Number.isSafeInteger(bytes) || bytes <= 0) return refuse('invalidInput', `invalid input: cache limit ${bytes}`, { detail: `cache limit ${bytes}` });
      catalog.limit = bytes;
      evict();
      return Promise.resolve(cacheInfo());
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
      return Promise.resolve(check({ ...local, source }, medium, capOf(key)));
    },
    prepareThemeVideo: (key) => {
      const video = videoOfTheme();
      if (!isLive(key) || video?.state !== 'missing') return refuse('noVideo', 'the live screen is not missing the theme video');
      const [medium, , name] = video.path.split('/');
      const asset = theme().background.asset;
      const info = videoInfo(asset);
      // A framing that leaves a panel-native video as it is sends the file without converting it.
      const { turns, asIs } = demoThemeVideo(theme(), autoOf(theme()), info);
      const local = { name: fileNameOf(asset), source: asset, size: info?.bytes ?? 18_874_368, format: 'MP4', width: info?.width ?? 1920, height: info?.height ?? 480, native: asIs };
      return Promise.resolve(check(local, medium, capOf(key), name, turns));
    },
    runUpload: async (ticket, overwrite) => {
      const p = pending.get(ticket);
      if (!p) return refuse('stale', 'this upload is no longer prepared');
      pending.delete(ticket);
      if (files.get(p.path) > 0 && !overwrite) return refuse('notConfirmed', `replacing ${p.path} needs confirmation`);
      if (job) return refuse('busy', 'a storage operation is using the screen');
      return asJob(() => upload(p));
    },
    cancelJob: () => {
      if (job) {
        job.cancelled = true;
        gate.cancel();
      }
      return Promise.resolve(Boolean(job));
    },
    /** Lets a held job phase go on (`hooks.hold`). */
    letGo: () => gate.letGo(),
    deleteStored: (key, path, confirmed) => {
      if (limited(key)) return refuse('unsupported', 'not supported: deleting files', { detail: 'deleting files' });
      if (!confirmed) return refuse('notConfirmed', `deleting ${path} needs confirmation`);
      files.delete(path);
      markDeleted(path);
      return Promise.resolve();
    },
    playStored: (key, path) => {
      if (isLive(key)) return refuse('live', 'turn live mode off to play files');
      if (!(files.get(path) > 0)) return refuse('invalidInput', `invalid input: ${path} is not stored on the screen`, { detail: `${path} is not stored on the screen` });
      state.playback = path;
      return Promise.resolve();
    },
    stopPlayback: (key) => {
      if (isLive(key)) return refuse('live', 'turn live mode off to stop files');
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
    /** Whether ffmpeg is there (or was located): it decodes the preview's video. */
    toolsReady: () => tools.ready,
    /**
     * What "When the computer shuts down" reads and adds (`demo-standby.js`):
     * the files, whether a card is in, the boot media, and a photo sent to
     * the card album, cataloged with its local copy like every upload.
     */
    standbyStorage: {
      files: () => new Map(files),
      card: () => card,
      boot: () => state.boot,
      // A photo's upload waits here like a job's phase (`hooks.hold`, tests only).
      hold: () => gate.hold(),
      storePhoto: (path, size, source) => {
        files.set(path, size);
        record({ path, card: cardNow(), size, content: demoContent(source, size), localCopy: true, sentAt: nowSec(), source, resolution: { ...NATIVE }, state: 'stored' });
      },
    },
    /** What the simulated screen plays, shows at power-up and starts with, and the catalog. */
    storageState: () => ({ ...state, files: new Map(files), catalog: catalog.entries.map((e) => ({ ...e })), limit: catalog.limit }),
  };
}

/**
 * @param {string} scenario key of SCENARIOS
 * @param {{now?: () => number, delay?: (ms: number) => Promise<void>, wait?: (fn: () => void, ms: number) => unknown, cancel?: (timer: unknown) => void}} [clock]
 *   seconds now, a pause, and the timer the preview's video decoder idles out with
 * @param {{onWindow?: (state: 'open'|'hidden'|'closed'|'quit') => void, onSensorsShown?: (keys: string[]) => void, onDecoder?: (state: 'running'|'stopped') => void, onGuide?: (page: string, language: string) => void, onStandby?: (writes: object[]) => void, languages?: readonly string[], hold?: boolean}} [hooks]
 *   what the window does, the sensors the list shows, the preview's video
 *   decoder, the guide pages opened, every plan B written to a screen, the
 *   system's languages, and whether job phases wait in the middle until
 *   `letGo` (tests)
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
  // The library: `Demo` first (the session's), the other screens' themes,
  // then what the user saves; each with the revision of its "files".
  const saved = [
    { location: 'demo://Demo', theme: structuredClone(DEMO_THEME), bundled: true, revision: 1 },
    ...DEMO_LIBRARY.map((entry) => ({ location: `demo://${entry.theme.name}`, theme: structuredClone(entry.theme), bundled: entry.bundled, drawable: entry.drawable !== false, revision: 1 })),
  ];
  // Which themes the Themes tab lists, as remembered.
  let themeFilter = { scope: null, axis: 'all' };
  const images = [];
  // Videos and animated GIFs added for a background (with their posters,
  // ref → data URL, and their own size, play time and bytes), and the sizes
  // of files dropped on the window, by demo source.
  const videos = new Map();
  const posters = new Map();
  const videoFiles = new Map(Object.entries(DEMO_THEME_VIDEOS));
  const dropped = new Map();
  // The scenario theme's video is in its assets, with its poster.
  const ownVideo = DEMO_THEME_VIDEOS[theme.background?.asset];
  if (ownVideo?.poster) {
    const ref = theme.background.asset;
    posters.set(ownVideo.poster, ownVideo.posterUrl);
    videos.set(ref, { ref, kind: 'video', animated: false, dataUrl: null, poster: ownVideo.poster, bytes: ownVideo.bytes, durationMs: ownVideo.durationMs });
  }
  /** Screen key → last orientation shown on it or chosen for it. */
  const remembered = new Map();
  const screenOf = (key) => devices.screens.find((s) => s.key === key);
  const modelOf = (key) => screenOf(key)?.models[0];
  /** The listed screen live now, whichever of its ports names it, else `null`. */
  const liveScreen = () => liveScreenIn(devices.screens, live);
  /** Whether the screen listed as `key` is the live one, by any of its ports (like `Studio::is_live`). */
  const isLiveScreen = (key) => Boolean(live) && (live === key || liveScreen()?.key === key);
  /** The key live mode records for the screen listed as `key`: its MCU port in `mcuLive` (0.1.0-dev.287). */
  const liveKeyOf = (key) => {
    if (!chosen.mcuLive) return key;
    return screenOf(key)?.wake?.address ?? key;
  };
  /** Remembers the theme's orientation for the live screen, under its listed key. */
  const rememberLive = () => {
    if (live) remembered.set(liveScreen()?.key ?? live, theme.orientation);
  };
  /** What Auto is for a theme, told against the live screen's panel (`video_auto`). */
  const autoOf = (t) => {
    const info = videoFiles.get(t?.background?.asset) ?? null;
    if (!t?.canvas) return demoVideoAuto(t, info, null);
    return demoVideoAuto(t, info, demoPanelFor(t, liveScreen()?.models[0]));
  };
  // The `hung` scenario: the screen stops reading until it is restarted. The
  // `flaky` one: it drops once after going live and comes back by itself.
  const screenState = { hung: Boolean(chosen.hung), away: chosen.flaky ? DEMO_AWAY_SAMPLES : 0 };
  const storage = createDemoStorage(chosen, {
    delay,
    now,
    live: () => live,
    isLive: isLiveScreen,
    theme: () => theme,
    // Every theme whose video is protected: the edited one and the library's.
    themes: () => [theme, ...saved.map((s) => s.theme)],
    screens: () => devices.screens,
    hold: Boolean(hooks.hold),
    hung: () => screenState.hung,
    videoInfo: (ref) => videoFiles.get(ref) ?? null,
    autoOf,
  });
  const { videoOfTheme, toolsReady, standbyStorage, ...storageApi } = storage;
  // "When the computer shuts down": the photo is framed in the shape the
  // screen stands in, the orientation last used with it, else its model's.
  const standby = createDemoStandby({
    chosen,
    screens: () => devices.screens,
    storage: standbyStorage,
    orientationOf: (key) => demoOrientation(modelOf(key), remembered.get(key)),
    denied: demoDenied,
    onWrite: (writes) => hooks.onStandby?.(writes),
  });
  const taken = () => new Set([...images, ...posters.keys(), ...videos.keys()]);
  const decoder = createDemoDecoder({ wait: clock.wait, cancel: clock.cancel, onState: (state) => hooks.onDecoder?.(state) });

  /**
   * What the preview shows of a theme's video background: its picture
   * playing (ffmpeg there, motion allowed), else its poster, turned and
   * framed; `null` without a video background or anything to show.
   */
  function videoPicture(next, motion, nowMs) {
    const bg = next?.background;
    if (bg?.type !== 'video') return null;
    const info = videoFiles.get(bg.asset) ?? null;
    const framing = framingOf(bg);
    const rotation = resolvedRotation(framing, autoOf(next));
    const sideways = rotation % 180 === 90;
    const source = info ? { width: info.width, height: info.height } : { width: sideways ? next.canvas.height : next.canvas.width, height: sideways ? next.canvas.width : next.canvas.height };
    const shown = { source, preTurned: Boolean(info?.preTurned), rotation, framing, box: pictureBox(source, rotation, framing, next.canvas) };
    if (motion && toolsReady()) return { ...shown, ...decoder.picture(bg.asset, info?.durationMs ?? DEMO_VIDEO_LOOP_MS, nowMs) };
    return bg.poster ? { ...shown, ms: DEMO_POSTER_MS, nextMs: null } : null;
  }
  /** A free asset reference for `fileName`, like the backend's. */
  const freeRef = (fileName) => {
    const name = demoAssetName(fileName);
    const dot = name.lastIndexOf('.');
    const [stem, extension] = dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ''];
    const used = taken();
    for (let n = 1; ; n += 1) {
      const ref = `assets/${stem}${n === 1 ? '' : `-${n}`}${extension}`;
      if (!used.has(ref)) return ref;
    }
  };

  /**
   * Adds a file like the backend's `add_media`: a video or an animated GIF
   * becomes a video with a poster (none without ffmpeg), a picture an image.
   */
  async function addMedia(source) {
    const name = source.startsWith('demo://') ? source.slice('demo://'.length) : fileNameOf(source);
    const known = DEMO_LOCAL_FILES[name];
    const bytes = known?.size ?? dropped.get(source) ?? 0;
    const extension = extensionOf(name);
    if (!droppable(name)) {
      return Promise.reject(Object.assign(new Error(`${name} is not a video, an animated GIF or a picture Bezel can use`), { code: 'notMedia', args: { file: name } }));
    }
    const still = extension === 'gif' ? Boolean(known?.still) : PICTURES.includes(extension);
    const ref = freeRef(name);
    if (still) {
      images.push(ref);
      return { ref, kind: 'image', poster: null, bytes, durationMs: null, posterError: null };
    }
    const tools = await storageApi.mediaTools();
    const poster = tools.ready ? freeRef(`${ref.slice('assets/'.length, ref.lastIndexOf('.'))}-poster.png`) : null;
    if (poster) posters.set(poster, DEMO_POSTER_URL);
    const animated = extension === 'gif';
    const durationMs = known?.durationMs ?? (animated ? 2_400 : 12_400);
    videos.set(ref, { ref, kind: animated ? 'image' : 'video', animated, dataUrl: animated ? DEMO_POSTER_URL : null, poster, bytes, durationMs });
    // Its own size, as the backend reads it from the file (a dropped one: 1920x1080).
    videoFiles.set(ref, { width: known?.width ?? 1920, height: known?.height ?? 1080, durationMs, bytes });
    return { ref, kind: 'video', poster, bytes, durationMs, posterError: poster ? null : { ...NO_POSTER } };
  }
  /**
   * A collection item copied into the theme under its name, like
   * `use_collected`: an image, or a background that plays like an animated
   * GIF added with "Add video…".
   */
  async function useInTheme(item, target) {
    const name = `${item.name}.gif`;
    if (target === 'image') {
      const ref = freeRef(name);
      images.push(ref);
      return { ref, kind: 'image', poster: null, bytes: item.bytes, durationMs: null, posterError: null };
    }
    const source = `demo://${name}`;
    dropped.set(source, item.bytes);
    return addMedia(source);
  }
  /**
   * The user's saved themes that use one of `refs`, and whether the open
   * one holds one of them among its assets (like the backend, which looks
   * for the same bytes in the session's assets, used or not yet).
   */
  const themesUsing = (refs) => {
    const uses = (t) => refs.some((ref) => JSON.stringify(t).includes(JSON.stringify(ref)));
    const held = taken();
    return { themes: saved.filter((s) => !s.bundled && uses(s.theme)).map((s) => s.theme.name), openTheme: refs.some((ref) => held.has(ref)) };
  };
  const gifs = createDemoGifs(chosen.klipy ?? {}, {
    now,
    useInTheme,
    themesUsing,
    onQuery: (query) => hooks.onGifQuery?.(query),
    onPreview: (id) => hooks.onGifPreview?.(id),
    onCollect: (id) => hooks.onGifCollect?.(id),
    onLink: (link) => hooks.onLink?.(link),
  });
  // The window, like the app: the close button hides it while a screen is
  // live, asks the UI when edits are unsaved, and closes it otherwise.
  let unsaved = false;
  let windowState = 'open';
  // The language the user chose (`null`: the system's, from the browser).
  let language = null;
  const systemLanguage = pickLocale(hooks.languages ?? []);
  const sensorOptions = { pingHost: null, mangohudDir: null };
  const closeListeners = new Set();
  const quitListeners = new Set();
  const windowGoes = (state) => {
    windowState = state;
    hooks.onWindow?.(state);
  };

  return {
    ...storageApi,
    ...gifs,
    ...standby,
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
    searchWeatherCities: async (query) => [{ name: query.trim(), region: 'Lazio', country: 'Italia', latitude: 41.9, longitude: 12.5 }],
    sample: () => {
      const t = now();
      const readings = {};
      DEMO_SENSORS.forEach(([key, , , quantity, base, swing], i) => {
        const value = demoValue(base, swing, t, i);
        readings[key] = { value, display: demoFormat(value, quantity) };
      });
      readings['gpu.1.fan'] = { unavailable: 'no fan sensor', display: '—' };
      // A hung screen stops live mode, like a frame the screen stopped reading.
      let liveError = null;
      if (live && screenState.hung) {
        const { code, args, message } = demoHung();
        liveError = { code, args, message };
        live = null;
      }
      // A live screen that dropped is connected again by the backend.
      let reconnecting = null;
      if (live && screenState.away > 0) {
        screenState.away -= 1;
        reconnecting = { attempt: 1, attempts: 3 };
      }
      return Promise.resolve({ sampleMillis: 3, readings, live: live || null, liveError, video: videoOfTheme(), reconnecting });
    },
    session: () => Promise.resolve({ theme: structuredClone(theme), location: chosen.theme ? null : saved[0].location, minRefreshSeconds: DEMO_MIN_REFRESH }),
    /** The preview, like `render_preview`: a video background plays with `motion` (see the bridge). */
    render: (next, { motion = true } = {}) => {
      const started = performance.now();
      const t = now();
      const video = videoPicture(next, motion, t * 1000);
      const frame = renderApprox(next, t, video);
      const due = [demoNextChange(next, t * 1000), video?.nextMs ?? null].filter((ms) => ms !== null);
      return Promise.resolve({ ...frame, millis: performance.now() - started, nextMs: due.length ? Math.min(...due) : null });
    },
    /** What Auto is for the theme's video background, like `video_auto`. */
    videoAuto: (next) => Promise.resolve(autoOf(next)),
    /** Opens a guide page (the demo shows which on the page). */
    openGuide: (page, language) => {
      if (!DEMO_GUIDE_PAGES.includes(page) || !['pt-BR', 'en'].includes(language)) {
        const detail = `guide page "${page}" in "${language}"`;
        return Promise.reject(Object.assign(new Error(`invalid input: ${detail}`), { code: 'invalidInput', args: { detail } }));
      }
      hooks.onGuide?.(page, language);
      return Promise.resolve();
    },
    /** The video the preview decodes now (`null`: none), for tests. */
    decoding: () => decoder.playing(),
    pushTheme: (next) => {
      theme = structuredClone(next);
      rememberLive();
      return Promise.resolve();
    },
    setLive: (on, screen) => {
      if (on && chosen.denied) return Promise.reject(demoDenied(screen));
      live = on ? liveKeyOf(screen) : null;
      rememberLive();
      return Promise.resolve({ live });
    },
    setBrightness: (screen) => (chosen.denied ? Promise.reject(demoDenied(screen)) : Promise.resolve()),
    release: (screen) => (chosen.denied ? Promise.reject(demoDenied(screen)) : Promise.resolve()),
    /** Restarts a screen through its wake chip (about 10 s on the real one). */
    restartScreen: async (key) => {
      if (chosen.denied) throw demoDenied(key);
      const screen = screenOf(key);
      if (!screen) throw Object.assign(new Error(`screen not found: ${key}`), { code: 'screenNotFound', args: { screen: key } });
      if (!screen.restartable) {
        const detail = `restarting ${screen.models[0]?.name ?? key}: only Turing rev C screens restart, through their wake chip (MCU); unplug the screen and plug it back in`;
        throw Object.assign(new Error(`not supported: ${detail}`), { code: 'unsupported', args: { detail } });
      }
      // Live by any of its ports, it comes back live under the key it is listed by.
      const wasLive = isLiveScreen(key);
      live = null;
      await delay(DEMO_STEP_MS * 4);
      screenState.hung = false;
      if (wasLive) live = key;
      return { key, live: wasLive };
    },
    saveTheme: (next, saveAs) => {
      theme = structuredClone(next);
      const location = `demo://${next.name}`;
      const at = saved.findIndex((s) => s.location === location);
      if (saveAs || at < 0) saved.push({ location, theme: structuredClone(next), bundled: false, revision: 1 });
      else Object.assign(saved[at], { theme: structuredClone(next), drawable: true, revision: saved[at].revision + 1 });
      return Promise.resolve({ location });
    },
    listThemes: () => Promise.resolve(saved.map((s) => ({
      name: s.theme.name, location: s.location, canvas: { ...s.theme.canvas }, orientation: s.theme.orientation, bundled: s.bundled, ...demoFits(s.theme), revision: String(s.revision),
    }))),
    /** A library theme's thumbnail; `null` for one that cannot be drawn. */
    themeThumbnail: (location) => {
      const found = saved.find((s) => s.location === location);
      if (!found) return Promise.reject(Object.assign(new Error(`${location} is not in the theme library; import it instead`), { code: 'notInLibrary', args: { location } }));
      return Promise.resolve(found.drawable === false ? null : demoThumbnail(found.theme));
    },
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
    addIcon: (name, svg) => {
      const ref = `assets/${name.startsWith('mdi-') ? name : `tabler-${name}`}-${posters.size + 1}.svg`;
      posters.set(ref, `data:image/svg+xml,${encodeURIComponent(svg)}`);
      return Promise.resolve({ ref });
    },
    assets: () => Promise.resolve([
      ...images.map((ref) => ({ ref, kind: 'image' })),
      ...[...posters].map(([ref, dataUrl]) => ({ ref, kind: 'image', dataUrl, bytes: 184_320, iconSvg: dataUrl.startsWith('data:image/svg+xml,') ? decodeURIComponent(dataUrl.split(',', 2)[1]) : null })),
      ...[...videos.values()].map((v) => ({ ...v })),
    ]),
    /** A video, GIF or picture: dropped (`source`), else picked in the dialog. */
    addMedia: (source = null) => addMedia(source ?? DEMO_PICKED_VIDEO),
    /** A dropped file as a source the demo can add or send, its size kept. */
    fileSource: (file) => {
      const source = storageApi.fileSource(file);
      if (source) dropped.set(source, file.size);
      return source;
    },
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
    quitApp: () => {
      windowGoes('quit');
      return Promise.resolve();
    },
    onQuitRequested: (cb) => {
      quitListeners.add(cb);
      return Promise.resolve(() => quitListeners.delete(cb));
    },
    /** The tray's Quit: over unsaved edits the window shows and the UI asks. */
    requestQuit: () => {
      if (!unsaved) {
        windowGoes('quit');
        return;
      }
      windowGoes('open');
      for (const cb of quitListeners) cb();
    },
    windowState: () => windowState,
    preferences: () => Promise.resolve({
      language,
      systemLanguage,
      pingHost: sensorOptions.pingHost ?? DEMO_PING_HOST,
      defaultPingHost: DEMO_PING_HOST,
      mangohudDir: sensorOptions.mangohudDir,
      mangohud: true,
      themeFilter: { ...themeFilter },
    }),
    setThemeFilter: (scope, axis) => {
      const chosen = scope ?? null;
      let unknown = null;
      if (chosen !== null && !SCOPES.includes(chosen)) unknown = chosen;
      else if (!AXES.includes(axis)) unknown = String(axis);
      if (unknown !== null) {
        const detail = `theme filter "${unknown}"`;
        return Promise.reject(Object.assign(new Error(`invalid input: ${detail}`), { code: 'invalidInput', args: { detail } }));
      }
      themeFilter = { scope: chosen, axis };
      return Promise.resolve();
    },
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
    /** The sensors the list shows: the app measures them (`net.ping` only then). */
    showSensors: (keys) => {
      hooks.onSensorsShown?.([...keys]);
      return Promise.resolve();
    },
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

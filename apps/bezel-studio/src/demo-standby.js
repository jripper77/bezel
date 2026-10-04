// The demo's "When the computer shuts down" (D-2026-10-03-power-off-standby-
// 2, -4, -6): the same calls and answers as the studio's standby commands
// (`standby_overview`, `set_standby`, `pick_photo`, `album_preview`,
// `album_add`), over the demo's screens and storage, so the section runs in
// a browser for Playwright. Nothing here reaches a screen. The choice is
// kept per model, like the catalog the CLI shares (two unnamed 8.8" share
// it); each plan B written goes to a hook, which the bridge shows on the
// page, and nothing is written or recorded without the confirmation.
import { DEMO_PHOTOS, DEMO_PICKED_PHOTO } from './demo-data.js';
import { CHOICES, PHOTO_FITS, albumName, albumPath, screenShape, sleepMinutesOf } from './standby.js';

/** The OPTIONS start modes (protocol §6.2): the built-in default, the images, the videos. */
export const DEMO_START_MODES = Object.freeze({ default: 0, image: 1, video: 2 });
/** The bytes per pixel a stored album PNG comes to, roughly. */
const PNG_BYTES_PER_PIXEL = 1.2;
/** How much smaller than the screen the demo's preview is drawn. */
const PREVIEW_SCALE = 4;

/** The family whose screens keep a choice (D-2026-10-03-power-off-standby-2 (1)). */
const REV_C = 'turing-rev-c';

/** An error like the app's: a code, its arguments and the English text. */
const refusal = (code, message, args = {}) => Object.assign(new Error(message), { code, args });

/** Reads a picked photo of the demo and the fit, or why not. */
function photoOf(source, fit) {
  const photo = DEMO_PHOTOS[source];
  if (!photo) return { error: refusal('fileError', `${source}: no such file`, { file: source, reason: 'no such file' }) };
  if (!PHOTO_FITS.includes(fit)) return { error: refusal('invalidInput', `invalid input: fit "${fit}"`, { detail: `fit "${fit}"` }) };
  return { photo };
}

/** The start mode of the boot media at `boot` (`internal/image/logo.png`), or the default without one. */
export function demoBootStartMode(boot) {
  if (!boot) return DEMO_START_MODES.default;
  return boot.split('/')[1] === 'image' ? DEMO_START_MODES.image : DEMO_START_MODES.video;
}

/**
 * The plan B a request writes (OPTIONS, D-2026-10-03-power-off-standby-2
 * (3)): `off` the minutes with the boot media's start mode, `video` the
 * videos' mode, `album` the images', `keep` the boot media's; the sleep
 * timer only with `off`.
 */
export function demoPlanB(request, boot) {
  const startMode = { video: DEMO_START_MODES.video, album: DEMO_START_MODES.image }[request.choice] ?? demoBootStartMode(boot);
  return { startMode, sleepMinutes: request.choice === 'off' ? request.sleepMinutes : 0 };
}

/**
 * Where a photo of `photo`'s size lands in a box of `shape`'s, centred:
 * `cover` fills the box (the photo's edges cut), `contain` shows it whole.
 */
export function demoPhotoBox(photo, shape, fit) {
  const scales = [shape.width / photo.width, shape.height / photo.height];
  const scale = fit === 'contain' ? Math.min(...scales) : Math.max(...scales);
  const width = photo.width * scale;
  const height = photo.height * scale;
  return { x: (shape.width - width) / 2, y: (shape.height - height) / 2, width, height };
}

/** The demo's preview of a photo framed in `shape`: a beach, black around it with Fit, as an SVG `data:` URL. */
export function demoAlbumPicture(photo, shape, fit) {
  const w = shape.width / PREVIEW_SCALE;
  const h = shape.height / PREVIEW_SCALE;
  const box = demoPhotoBox(photo, { width: w, height: h }, fit);
  const at = (fx, fy) => `${box.x + box.width * fx} ${box.y + box.height * fy}`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}"><rect width="${w}" height="${h}" fill="#000"/>`
    + '<defs><linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#38bdf8"/><stop offset="1" stop-color="#e0f2fe"/></linearGradient></defs>'
    + `<rect x="${box.x}" y="${box.y}" width="${box.width}" height="${box.height}" fill="url(#sky)"/>`
    + `<circle cx="${box.x + box.width * 0.7}" cy="${box.y + box.height * 0.22}" r="${Math.min(box.width, box.height) * 0.08}" fill="#fde047"/>`
    + `<path d="M${at(0, 0.55)} L${at(1, 0.5)} L${at(1, 0.72)} L${at(0, 0.75)}Z" fill="#0e7490"/>`
    + `<path d="M${at(0, 0.75)} L${at(1, 0.72)} L${at(1, 1)} L${at(0, 1)}Z" fill="#fcd34d"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg).replaceAll('(', '%28').replaceAll(')', '%29')}`;
}

/**
 * @param {object} deps
 * @param {{standby?: {choice: string, sleepMinutes: number|null, file: string|null}, denied?: boolean}} deps.chosen the scenario
 * @param {() => object[]} deps.screens the screens listed now
 * @param {{files: () => Map<string, number>, card: () => boolean, boot: () => string|null, storePhoto: (path: string, size: number, source: string) => void, hold?: () => Promise<void>}} deps.storage
 *   the screen's files, whether it has a card, its boot media, a photo stored on the card, and
 *   where a photo's upload waits until let go (tests)
 * @param {(key: string) => string} deps.orientationOf how the screen stands (the remembered orientation, else its model's)
 * @param {(screen: string) => Error} deps.denied the error of a port the system denies
 * @param {(writes: object[]) => void} [deps.onWrite] every plan B written so far, after each
 */
export function createDemoStandby({ chosen, screens, storage, orientationOf, denied, onWrite = () => {} }) {
  // The choice of each model (the catalog's `ScreenRecord.standby`).
  const records = new Map();
  const initial = chosen.standby ?? null;
  const writes = [];

  const screenOf = (key) => screens().find((s) => s.key === key) ?? null;
  const modelOf = (screen) => screen.models[0];
  const recordOf = (screen) => records.get(modelOf(screen).id) ?? (screen.family === REV_C ? initial : null) ?? { choice: 'keep', sleepMinutes: null, file: null };
  const connected = (screen) => screen.family === REV_C && screen.state === 'awake';
  const videosOf = (screen) => {
    if (!connected(screen)) return [];
    return [...storage.files()].filter(([path]) => path.split('/')[1] === 'video').map(([path, size]) => {
      const [medium, kind, name] = path.split('/');
      return { path, medium, kind, name, size };
    });
  };

  /** Why `choice` is not offered on `screen`, or `null`. */
  function reasonOf(screen, choice, videos) {
    if (screen.family !== REV_C) return 'unsupported';
    if (!connected(screen)) return 'notConnected';
    if (choice === 'album' && !storage.card()) return 'noCard';
    if (choice === 'video' && videos.length === 0) return 'noVideo';
    return null;
  }

  function overview(screen) {
    const record = recordOf(screen);
    const videos = videosOf(screen);
    const options = CHOICES.map((choice) => {
      const reason = reasonOf(screen, choice, videos);
      return { choice, enabled: reason === null, reason };
    });
    return { ...record, options, videos, card: connected(screen) && storage.card(), orientation: orientationOf(screen.key) };
  }

  /** The screen of `key` when it can take a change, else why not (a rejected promise). */
  function changeable(key) {
    const screen = screenOf(key);
    if (!screen) return { error: refusal('screenNotFound', `screen not found: ${key}`, { screen: key }) };
    if (chosen.denied) return { error: denied(key) };
    if (screen.family !== REV_C) return { error: refusal('unsupported', 'not supported: only Turing rev C screens keep a choice for when the computer shuts down', { detail: 'only Turing rev C screens keep a choice for when the computer shuts down' }) };
    if (!connected(screen)) return { error: refusal('unsupported', 'not supported: the screen is asleep; wake it first', { detail: 'the screen is asleep; wake it first' }) };
    return { screen };
  }

  /** Why the request cannot be taken on `screen`, or `null`. */
  function invalid(screen, request) {
    const detail = (text) => refusal('invalidInput', `invalid input: ${text}`, { detail: text });
    if (!CHOICES.includes(request.choice)) return detail(`choice "${request.choice}"`);
    if (request.choice === 'off' && sleepMinutesOf(request.sleepMinutes) === null) return detail(`sleep timer of ${request.sleepMinutes} minutes (1 to 10)`);
    if (request.choice === 'video' && !videosOf(screen).some((v) => v.path === request.file)) return detail(`${request.file} is not stored on the screen`);
    if (request.choice === 'album' && !storage.card()) return refusal('unsupported', 'not supported: the album needs an SD card in the screen', { detail: 'the album needs an SD card in the screen' });
    return null;
  }

  return {
    /** The choice of the screen `key` and what it offers (`standby_overview`). */
    standbyOverview: (key) => {
      const screen = screenOf(key);
      if (!screen) return Promise.reject(refusal('screenNotFound', `screen not found: ${key}`, { screen: key }));
      if (chosen.denied) return Promise.reject(denied(key));
      return Promise.resolve(overview(screen));
    },
    /**
     * Records the choice and writes its plan B, only with `confirmed`
     * (`set_standby`); `keep` while it is the choice writes nothing.
     */
    setStandby: (key, request, confirmed) => {
      const { screen, error } = changeable(key);
      if (error) return Promise.reject(error);
      const wanted = { choice: request?.choice, sleepMinutes: request?.sleepMinutes ?? null, file: request?.file ?? null };
      const wrong = invalid(screen, wanted);
      if (wrong) return Promise.reject(wrong);
      if (!confirmed) return Promise.reject(refusal('notConfirmed', `choosing "${wanted.choice}" for when the computer shuts down needs confirmation`, { detail: `choosing "${wanted.choice}"` }));
      if (wanted.choice === 'keep' && recordOf(screen).choice === 'keep') return Promise.resolve(overview(screen));
      const record = { choice: wanted.choice, sleepMinutes: wanted.choice === 'off' ? wanted.sleepMinutes : null, file: wanted.choice === 'video' ? wanted.file : null };
      records.set(modelOf(screen).id, record);
      writes.push({ screen: key, ...record, ...demoPlanB(record, storage.boot()) });
      onWrite(writes.map((w) => ({ ...w })));
      return Promise.resolve(overview(screen));
    },
    /** The photo the demo's picker returns. */
    pickPhoto: () => Promise.resolve(DEMO_PICKED_PHOTO),
    /** The photo framed in the shape the screen stands in (`album_preview`). */
    albumPreview: (key, source, fit) => {
      const screen = screenOf(key);
      if (!screen) return Promise.reject(refusal('screenNotFound', `screen not found: ${key}`, { screen: key }));
      const { photo, error } = photoOf(source, fit);
      if (error) return Promise.reject(error);
      return Promise.resolve(demoAlbumPicture(photo, screenShape(modelOf(screen), orientationOf(key)), fit));
    },
    /**
     * Sends the framed photo to the card album as a PNG of the panel's size
     * (`album_add`), only with `confirmed`; a photo of that name is replaced
     * only with `replace` too (`notConfirmed` otherwise, like the app's
     * check of the card's listing); without a card nothing is sent.
     */
    albumAdd: (key, source, fit, name, confirmed, replace = false) => {
      const { screen, error } = changeable(key);
      if (error) return Promise.reject(error);
      if (!storage.card()) return Promise.reject(refusal('unsupported', 'not supported: the album needs an SD card in the screen', { detail: 'the album needs an SD card in the screen' }));
      const read = photoOf(source, fit);
      if (read.error) return Promise.reject(read.error);
      const typed = albumName(String(name ?? ''));
      if (typed.problem) return Promise.reject(refusal('invalidInput', `invalid input: the name "${name}"`, { detail: `the name "${name}"` }));
      if (!confirmed) return Promise.reject(refusal('notConfirmed', `adding ${typed.name} to the album needs confirmation`, { detail: `adding ${typed.name} to the album` }));
      const path = albumPath(typed.name);
      if (storage.files().has(path) && !replace) return Promise.reject(refusal('notConfirmed', `replacing ${path} needs confirmation`, { detail: `replacing ${path}` }));
      const { width, height } = modelOf(screen);
      const bytes = Math.round(width * height * PNG_BYTES_PER_PIXEL);
      return Promise.resolve(storage.hold?.()).then(() => {
        storage.storePhoto(path, bytes, source);
        return { path, bytes };
      });
    },
    /** Every plan B written so far (tests). */
    standbyWrites: () => writes.map((w) => ({ ...w })),
  };
}

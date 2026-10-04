// "When the computer shuts down" (D-2026-10-03-power-off-standby-2, -4, -6):
// what each choice offers for the screen chosen at the top, what activating
// it does, the request `set_standby` gets, what the confirmation says will be
// written to the screen (its plan B, an OPTIONS packet) and happen at
// shutdown, the screen's videos and the card album's photos, the shape a
// photo is framed in, and the name it gets on the card. Nothing here
// touches the DOM: `ui/standby.js` draws it; texts are translation keys and
// params, translated there.
import { isHorizontal } from './editor/geometry.js';
import { MEDIA, baseName, renamePreview } from './storage-manager.js';

/** The four choices, in the order the radio group lists them; `keep` is the default. */
export const CHOICES = Object.freeze(['keep', 'off', 'video', 'album']);
/** Why a choice is not offered (`StandbyDto.options[].reason`). */
export const REASONS = Object.freeze(['notConnected', 'unsupported', 'noCard', 'noVideo']);
/** The sleep timer of `off`, minutes: what the screen takes and what is suggested. */
export const SLEEP = Object.freeze({ min: 1, max: 10, suggested: 5 });
/** How a photo is framed: Fill (the default) or Fit. */
export const PHOTO_FITS = Object.freeze(['cover', 'contain']);
/** The album: the card's image folder. */
export const ALBUM = Object.freeze({ medium: 'sd', kind: 'image' });
/** The format the album's photos are stored in. */
export const ALBUM_EXTENSION = 'png';
/**
 * A name `renamePreview` checks a typed name against: any `.png` (its
 * extension), never equal to a name it accepts (a name never starts with a dot).
 */
const ANY_PNG = `.${ALBUM_EXTENSION}`;

/** What a screen whose answer has no option of a choice offers of it. */
const REFUSED = Object.freeze({ enabled: false, reason: 'unsupported' });

/**
 * The option of `choice` in an overview (`StandbyDto`): `{choice, enabled, reason}`.
 * @param {{options?: {choice: string, enabled: boolean, reason: string|null}[]}|null} overview
 * @param {string} choice
 */
export function optionOf(overview, choice) {
  const found = overview?.options?.find((o) => o.choice === choice);
  if (!found) return { choice, ...REFUSED };
  return { choice, enabled: Boolean(found.enabled), reason: found.enabled ? null : found.reason ?? 'unsupported' };
}

/**
 * Whether the screen keeps a choice at all: a family without it (anything
 * but Turing rev C) answers every option `unsupported`, and the choice does
 * not show (D-2026-10-03-power-off-standby-2 (1)).
 */
export function offered(overview) {
  return CHOICES.some((choice) => optionOf(overview, choice).reason !== 'unsupported');
}

/**
 * What activating the option of `choice` does: `refused` (not offered),
 * `nothing` (`keep` when it is the choice: nothing is sent), `manage` (the
 * album when it is the choice: its photos, nothing written) or `ask` (the
 * confirmation that says what is written; `off` and `video` ask again to
 * change the minutes or the video).
 */
export function activation(overview, choice) {
  if (!optionOf(overview, choice).enabled) return 'refused';
  if (overview.choice !== choice) return 'ask';
  if (choice === 'keep') return 'nothing';
  return choice === 'album' ? 'manage' : 'ask';
}

/** `minutes` as a whole number of the sleep timer's range, else `null`. */
export function sleepMinutesOf(minutes) {
  const n = Number(minutes);
  return Number.isInteger(n) && n >= SLEEP.min && n <= SLEEP.max ? n : null;
}

/** The minutes the sleep timer offers, shortest first. */
export function sleepChoices() {
  return Array.from({ length: SLEEP.max - SLEEP.min + 1 }, (_, i) => SLEEP.min + i);
}

/** `count` minutes, as a translation key and params. */
export function minutesText(count) {
  return count === 1 ? { key: 'standby.minuteOne', params: {} } : { key: 'standby.minutes', params: { count } };
}

/**
 * What the confirmation of `choice` starts with: the minutes of `off` (the
 * screen's when it is the choice, else the suggestion) and the video of
 * `video` (the chosen one when still stored, else the first listed).
 */
export function dialogDefaults(overview, choice) {
  const current = overview?.choice === choice;
  const sleepMinutes = (current && sleepMinutesOf(overview.sleepMinutes)) || SLEEP.suggested;
  const videos = overview?.videos ?? [];
  const kept = current && videos.some((v) => v.path === overview.file) ? overview.file : null;
  return { sleepMinutes, file: kept ?? videos[0]?.path ?? null };
}

/**
 * The request `set_standby` gets for `choice` (`{choice, sleepMinutes,
 * file}`): the minutes only with `off`, the video only with `video`.
 */
export function requestOf(choice, { sleepMinutes = null, file = null } = {}) {
  return {
    choice,
    sleepMinutes: choice === 'off' ? sleepMinutesOf(sleepMinutes) : null,
    file: choice === 'video' ? file : null,
  };
}

/** Whether `request` can be sent: a choice, the minutes of `off`, the video of `video`. */
export function complete(request) {
  if (!CHOICES.includes(request.choice)) return false;
  if (request.choice === 'off') return request.sleepMinutes !== null;
  if (request.choice === 'video') return typeof request.file === 'string' && request.file.length > 0;
  return true;
}

/**
 * What confirming `request` does, as translation keys and params: what
 * happens when the computer shuts down (`atShutdown`), and each setting of
 * the plan B the screen gets now (`written`: how it starts at power-up, its
 * sleep timer, its brightness; D-2026-10-03-power-off-standby-2 (3)).
 */
export function confirmationOf(request) {
  const startsWith = { keep: 'standby.written.startBoot', off: 'standby.written.startBoot', video: 'standby.written.startVideo', album: 'standby.written.startAlbum' };
  const minutes = request.choice === 'off' ? minutesText(request.sleepMinutes) : null;
  const sleep = minutes ? { key: 'standby.written.sleep', params: { minutes } } : { key: 'standby.written.sleepOff', params: {} };
  const atShutdown = request.choice === 'video'
    ? { key: 'standby.atShutdown.video', params: { name: baseName(request.file ?? '') } }
    : { key: `standby.atShutdown.${request.choice}`, params: {} };
  return {
    atShutdown,
    written: [{ key: startsWith[request.choice], params: {} }, sleep, { key: 'standby.written.brightness', params: {} }],
  };
}

/**
 * What the current choice adds under its label: the sleep timer of `off`,
 * the video of `video`; `null` for any other option.
 */
export function currentDetail(overview, choice) {
  if (overview?.choice !== choice) return null;
  if (choice === 'off' && sleepMinutesOf(overview.sleepMinutes) !== null) return { key: 'standby.detail.off', params: { minutes: minutesText(overview.sleepMinutes) } };
  const medium = String(overview.file ?? '').split('/')[0];
  if (choice === 'video' && MEDIA.includes(medium)) return { key: `standby.detail.video.${medium}`, params: { name: baseName(overview.file) } };
  return null;
}

/**
 * The text of a message `{key, params}` in the language of `t`; a param
 * that is a message itself (`minutesText`) is translated first.
 * @param {(k: string, p?: object) => string} t
 * @param {{key: string, params?: Record<string, unknown>}} message
 */
export function translate(t, { key, params = {} }) {
  const isMessage = (value) => typeof value === 'object' && value !== null && typeof value.key === 'string';
  const filled = Object.fromEntries(Object.entries(params).map(([name, value]) => [name, isMessage(value) ? translate(t, value) : value]));
  return t(key, filled);
}

/** The screen's videos by medium, internal memory first; a medium without any is left out. */
export function videoGroups(videos) {
  return MEDIA.map((medium) => ({ medium, files: (videos ?? []).filter((v) => v.medium === medium) })).filter((g) => g.files.length > 0);
}

/**
 * The album's photos: the files of the card's image folder in the storage
 * manager's overview (`manager_overview`), by name. Its listing is the one
 * `manager_thumbnail` answers for, so Bezel's photos get their thumbnails.
 * @param {{files?: {medium: string, kind: string, name: string}[]}|null} overview
 */
export function albumPhotos(overview) {
  const photos = (overview?.files ?? []).filter((f) => f.medium === ALBUM.medium && f.kind === ALBUM.kind);
  return photos.sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * The shape the screen stands in, which a photo is framed in
 * (D-2026-10-03-power-off-standby-4 (3)): its model's panel turned the way
 * of `orientation` (`portrait`, `landscape` and their reverses).
 * @param {{width: number, height: number}} model
 * @param {string} orientation
 * @returns {{width: number, height: number, axis: 'horizontal'|'vertical'}}
 */
export function screenShape(model, orientation) {
  const long = Math.max(model.width, model.height);
  const short = Math.min(model.width, model.height);
  return isHorizontal(orientation) ? { width: long, height: short, axis: 'horizontal' } : { width: short, height: long, axis: 'vertical' };
}

/**
 * The name a photo gets in the album, from its file's: lower case, only
 * `[a-z0-9_-]` (a run of anything else is one `_`), `.png` (like the core's
 * `FileName::suggest`); `photo.png` when nothing is left.
 */
export function suggestPhotoName(source) {
  const file = baseName(source);
  const dot = file.lastIndexOf('.');
  const stem = (dot > 0 ? file.slice(0, dot) : file).replace(/[A-Z]+/g, (c) => c.toLowerCase());
  let out = '';
  for (const c of stem) {
    const kept = /[a-z0-9_-]/.test(c) ? c : '_';
    // A run of `_` is one `_`, and none leads the name.
    if (kept !== '_' || (out !== '' && !out.endsWith('_'))) out += kept;
  }
  // Runs are single, so at most one `_` trails: drop it.
  const name = out.endsWith('_') ? out.slice(0, -1) : out;
  return `${name || 'photo'}.${ALBUM_EXTENSION}`;
}

/**
 * A name typed for the album, as the screen takes it (lower case, only
 * `[a-z0-9_.-]`, no leading dot, `.png`), and what is wrong with it: a plan
 * refusal (`invalidName`, `extensionChanged`; `planRefusalText` says it), or `null`.
 */
export function albumName(raw) {
  return renamePreview(raw, ANY_PNG);
}

/** The photo of the album the name `name` would replace (the same name but for letter case), or `null`. */
export function albumClash(photos, name) {
  const lower = String(name).toLowerCase();
  return photos.find((p) => p.name.toLowerCase() === lower) ?? null;
}

/** The album's path of a photo named `name`. */
export const albumPath = (name) => `${ALBUM.medium}/${ALBUM.kind}/${name}`;

/**
 * Runs `step` on each of `items` (any iterable) in turn: the next starts
 * once the one before settled, and the promise settles after the last; a
 * step that fails stops the rest.
 */
export function inTurn(items, step) {
  return Array.from(items).reduce((before, item) => before.then(() => step(item)), Promise.resolve());
}

/**
 * Which answer about the screen shown is the newest, so that an older one is
 * never drawn over it: each reading of the screen shown takes a ticket, and
 * so does a write while its screen is the one shown (it outdates a reading
 * in flight). A write for another screen (the one a dialog was opened for,
 * since replaced at the top) takes none: the reading of the screen shown now
 * is still drawn, and that write's answer is not drawn over it.
 *
 * When a write's answer comes back to its screen shown, a reading of it
 * that started while the write ran (the screen chosen again at the top) may
 * have read the catalog before the write saved it: the screen is read again
 * then, so that it never keeps showing the choice from before the write
 * (review W3 of iteration 3).
 */
export function createAnswers() {
  let latest = 0;
  const newest = (ticket) => ticket !== null && ticket === latest;
  /** A new ticket, newer than every one before. */
  const next = () => {
    latest += 1;
    return latest;
  };
  return {
    /** A reading of the screen shown starts: its ticket. */
    reading: next,
    /** A write for `key` starts while `shown` is shown: its ticket, `null` for another screen. */
    writing: (key, shown) => (key === shown ? next() : null),
    /** Whether the answer of `ticket` is still the newest. */
    newest,
    /**
     * What the answer of the write for `key` (its `ticket`) does, arriving
     * while `shown` is shown: `draw` (no reading of its screen started
     * since), `read` (one did: read the screen again) or `none` (another
     * screen is shown; its own reading is drawn).
     */
    written: (ticket, key, shown) => {
      if (key !== shown) return 'none';
      return newest(ticket) ? 'draw' : 'read';
    },
  };
}

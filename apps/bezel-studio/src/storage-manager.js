// The storage manager's logic (D-2026-09-30-storage-manager-4, -7..-13): the
// two lists (internal memory and SD card) sorted and filtered, the
// multi-selection of a listbox and its keys, what each action may do with the
// selection (and why not), and the sentences of the backend's codes: cleanup
// findings, plan skips, warnings and refusals, run reports. Nothing here
// touches the DOM: `ui/manager.js` draws it, `ui/storage.js` runs the jobs.

/** The two media, internal first. */
export const MEDIA = Object.freeze(['internal', 'sd']);
/** The two folders of each medium. */
export const KINDS = Object.freeze(['image', 'video']);
/** How a list can be ordered (D-2026-09-30-storage-manager-13). */
export const SORTS = Object.freeze(['name', 'size', 'sent', 'kind']);
/** Which kinds a list shows. */
export const KIND_FILTERS = Object.freeze(['all', 'image', 'video']);
/** Which files a list shows by where they came from. */
export const ORIGIN_FILTERS = Object.freeze(['all', 'bezel', 'other', 'findings']);
/** Cleanup reason codes (core `cleanup::Code`), the pre-checked ones first. */
export const FINDING_CODES = Object.freeze(['hangPartial', 'pending', 'duplicate', 'sizeDiffers', 'variant', 'sameSize', 'unused']);
/** Why a plan leaves a file out (core `archive::Skip`). */
export const SKIP_CODES = Object.freeze(['conflict', 'noLocalCopy', 'deleteUnsupported', 'present']);
/** What a plan's confirmation warns about (core `archive::Warning`). */
export const WARNING_CODES = Object.freeze(['bootMedia', 'themeVideo']);
/** Why no plan could be made (core `archive::PlanRefusal`). */
export const PLAN_REFUSALS = Object.freeze(['noCard', 'notListed', 'sameMedium', 'invalidName', 'extensionChanged', 'sameName', 'unsendable', 'noSpace']);
/** What a plan does (core `archive::Transfer`). */
export const TRANSFERS = Object.freeze(['move', 'copy', 'rename', 'restore']);
/** Why a file stopped its batch (core `manager::Halt`). */
export const HALT_CODES = Object.freeze(['cancelled', 'sourceChanged', 'conflict', 'noLocalCopy', 'refused', 'failed']);
/** How far a stopped file had come (core `manager::Stage`), in order. */
export const STAGES = Object.freeze(['preflight', 'upload', 'verify', 'delete', 'catalog']);
/** Size limits offered for the local copies of deleted files (default 2 GiB, D-2026-09-30-storage-manager-6). */
export const CACHE_LIMITS = Object.freeze([512 * 2 ** 20, 2 ** 30, 2 * 2 ** 30, 5 * 2 ** 30, 10 * 2 ** 30]);
/** Longest upload name, bytes (core `FileName::MAX_BYTES`). */
export const MAX_NAME_BYTES = 208;

/** The other medium. */
export const otherMedium = (medium) => (medium === 'sd' ? 'internal' : 'sd');

/** The file name at the end of a local path, a demo source or a screen path. */
export function baseName(source) {
  return String(source).split(/[/\\]/).pop();
}

/** The medium of a screen path (`sd/video/a.mp4`: `sd`). */
export const mediumOf = (path) => String(path).split('/')[0];

// ------------------------------------------------------------- formats --

/**
 * Decimal sizes, like the screens' limits ("120 MB"): 184 kB, 24.1 MB.
 * @param {number|null|undefined} bytes
 * @param {string} locale
 */
export function formatBytes(bytes, locale = 'en') {
  if (bytes === null || bytes === undefined) return '—';
  const units = ['B', 'kB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let i = 0;
  while (value >= 1000 && i < units.length - 1) {
    value /= 1000;
    i += 1;
  }
  const digits = i === 0 || value >= 100 ? 0 : 1;
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: digits }).format(value)} ${units[i]}`;
}

/** Bytes in a MiB, the unit the screens' per-file limits are shown in. */
const MIB = 1024 * 1024;

/**
 * A size in MiB, like the core shows the per-file limits: whole when exact,
 * else one decimal, rounded `up` (a file over a limit never reads as equal
 * to it) or `down` (a limit never reads larger than it is).
 * @param {number|null|undefined} bytes
 * @param {string} locale
 * @param {'up'|'down'} rounding
 */
export function formatMiB(bytes, locale = 'en', rounding = 'down') {
  if (bytes === null || bytes === undefined) return '—';
  const scaled = (bytes * 10) / MIB;
  const tenths = rounding === 'up' ? Math.ceil(scaled) : Math.floor(scaled);
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(tenths / 10)} MiB`;
}

/** A size to the byte, as the association matches it: 7,444,889. */
export function formatExactBytes(bytes, locale = 'en') {
  return new Intl.NumberFormat(locale).format(bytes);
}

/** A cache limit in MiB or GiB, as the user picks it: 512 MiB, 2 GiB. */
export function formatLimit(bytes, locale = 'en') {
  const gib = bytes / 2 ** 30;
  if (gib >= 1) return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(gib)} GiB`;
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(bytes / MIB)} MiB`;
}

/** The day a file was sent (seconds since the epoch), in the UI's language. */
export function formatSent(seconds, locale = 'en') {
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(new Date(seconds * 1000));
}

// ------------------------------------------------------ what a screen does --

/**
 * What the storage tab offers for a screen. TUR_USB screens take uploads and
 * play files but neither delete nor set the boot media through Bezel
 * (D-2026-09-30-storage-video-7), so nothing that ends in a delete runs
 * there (D-2026-09-30-storage-manager-11).
 * @param {{family?: string, models?: {capabilities?: {storage?: boolean}}[]}|null} screen
 */
export function storageFeatures(screen) {
  const models = screen?.models ?? [];
  const storage = models.length > 0 && models.every((m) => Boolean(m.capabilities?.storage));
  const full = storage && screen.family !== 'turing-usb';
  return { storage, remove: full, boot: full };
}

// ------------------------------------------------- preflight refusals ---

/** One way a file differs from what the screen takes. */
export function mismatchText(t, m) {
  const params = { found: m.found ?? t('storage.unknownSize'), expected: m.expected ?? '' };
  return t(`storage.mismatch.${m.code}`, params);
}

/**
 * Why the preflight refused a file, in one or two sentences.
 * @param {(k: string, p?: object) => string} t
 * @param {string} locale
 * @param {object} r the refusal
 */
export function refusalText(t, locale, r) {
  const details = (r.mismatches ?? []).map((m) => mismatchText(t, m)).join('; ');
  switch (r.code) {
    case 'invalidName':
      return r.name ? t('storage.refused.invalidChar', { char: r.name }) : t('storage.refused.invalidName');
    case 'wrongExtension':
      return t('storage.refused.wrongExtension', { accepted: (r.accepted ?? []).map((e) => `.${e}`).join(', ') });
    case 'wrongProfile':
    case 'needsConverter':
      return t(`storage.refused.${r.code}`, { details });
    case 'tooLarge':
    case 'convertedTooLarge':
      return t(`storage.refused.${r.code}`, { size: formatMiB(r.bytes, locale, 'up'), limit: formatMiB(r.limit, locale, 'down') });
    case 'noSpace':
      return t('storage.refused.noSpace', { size: formatBytes(r.bytes, locale), free: formatBytes(r.limit, locale) });
    case 'wrongKind':
    case 'emptyFile':
    case 'noCard':
      return t(`storage.refused.${r.code}`);
    default:
      return r.message ?? String(r.code);
  }
}

// ------------------------------------------------------------- the lists --

/** Whether Bezel sent the file (its catalog entry is not deleted). */
export const isBezel = (file) => Boolean(file.entry && file.entry.state !== 'deleted');
/** Whether Bezel holds the file's exact bytes: it can be moved, renamed, restored. */
export const hasCopy = (file) => Boolean(isBezel(file) && file.entry.localCopy && file.entry.state !== 'pending');

const byName = (locale) => {
  const collator = new Intl.Collator(locale, { sensitivity: 'base', numeric: true });
  return (a, b) => collator.compare(a.name, b.name) || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
};

/**
 * The order of a list: by name; size (largest first, unknown last); date sent
 * (newest first, files Bezel did not send last); kind (images first). Ties go
 * by name.
 * @param {'name'|'size'|'sent'|'kind'} sort
 * @param {string} locale
 */
export function compareFiles(sort, locale = 'en') {
  const name = byName(locale);
  const keys = {
    size: (a, b) => (b.size ?? -1) - (a.size ?? -1),
    sent: (a, b) => (isBezel(b) ? b.entry.sentAt : -1) - (isBezel(a) ? a.entry.sentAt : -1),
    kind: (a, b) => KINDS.indexOf(a.kind) - KINDS.indexOf(b.kind),
  };
  const first = keys[sort];
  return first ? (a, b) => first(a, b) || name(a, b) : name;
}

/**
 * Whether `file` passes the filter: its name holds the text (letter case
 * aside), its kind, and its origin (sent by Bezel, not, or with a cleanup
 * finding).
 * @param {{text?: string, kind?: string, origin?: string}} filter
 */
export function matchesFilter(file, filter = {}) {
  const text = (filter.text ?? '').trim().toLowerCase();
  if (text && !file.name.toLowerCase().includes(text)) return false;
  if (filter.kind && filter.kind !== 'all' && file.kind !== filter.kind) return false;
  switch (filter.origin) {
    case 'bezel': return isBezel(file);
    case 'other': return !isBezel(file);
    case 'findings': return Boolean(file.finding);
    default: return true;
  }
}

/** The files of `medium` the list shows, in its order. */
export function visibleFiles(files, medium, filter, sort, locale = 'en') {
  return files.filter((f) => f.medium === medium && matchesFilter(f, filter)).sort(compareFiles(sort, locale));
}

// ------------------------------------------------------------ selection --

/** No file selected, none focused. */
export const emptySelection = () => ({ selected: [], anchor: null, focus: null });

function range(ids, from, to) {
  const a = ids.indexOf(from);
  const b = ids.indexOf(to);
  if (a < 0 || b < 0) return b < 0 ? [] : [to];
  return ids.slice(Math.min(a, b), Math.max(a, b) + 1);
}

function moved(state, action, ids) {
  const at = ids.indexOf(state.focus);
  let index;
  if (action.to === 'first') index = 0;
  else if (action.to === 'last') index = ids.length - 1;
  else if (at < 0) index = action.delta > 0 ? 0 : ids.length - 1;
  else index = Math.min(ids.length - 1, Math.max(0, at + action.delta));
  const focus = ids[index];
  if (!action.extend) return { ...state, focus };
  const anchor = ids.includes(state.anchor) ? state.anchor : (state.focus ?? focus);
  return { selected: range(ids, anchor, focus), anchor, focus };
}

function clicked(state, { id, ctrl, shift }, ids) {
  if (shift) {
    const anchor = ids.includes(state.anchor) ? state.anchor : id;
    return { selected: range(ids, anchor, id), anchor, focus: id };
  }
  if (ctrl) {
    const on = state.selected.includes(id);
    return { selected: on ? state.selected.filter((s) => s !== id) : [...state.selected, id], anchor: id, focus: id };
  }
  return { selected: [id], anchor: id, focus: id };
}

/**
 * The selection of a multi-select listbox after `action`; `ids` are the
 * options in the list's order. Arrows move the focus, Shift+arrows select
 * from the anchor to it, Space toggles the focused option, Ctrl+A selects
 * every option, Esc none; a click selects one, Ctrl+click toggles, Shift+click
 * selects a range. `prune` drops what the list no longer shows.
 * @param {{selected: string[], anchor: string|null, focus: string|null}} state
 * @param {{type: string, [k: string]: any}} action
 * @param {string[]} ids
 */
export function reduceSelection(state, action, ids) {
  switch (action.type) {
    case 'move':
      return ids.length ? moved(state, action, ids) : state;
    case 'toggle': {
      if (!ids.includes(state.focus)) return state;
      return clicked(state, { id: state.focus, ctrl: true }, ids);
    }
    case 'click':
      return clicked(state, action, ids);
    case 'all':
      return { ...state, selected: [...ids], focus: state.focus ?? ids[0] ?? null };
    case 'none':
      return { ...state, selected: [] };
    case 'only':
      return { selected: action.ids.filter((id) => ids.includes(id)), anchor: action.ids[0] ?? null, focus: action.ids[0] ?? null };
    case 'prune': {
      const keep = (id) => (ids.includes(id) ? id : null);
      return { selected: state.selected.filter((id) => ids.includes(id)), anchor: keep(state.anchor), focus: keep(state.focus) ?? ids[0] ?? null };
    }
    default:
      return state;
  }
}

/**
 * What a key does in a file list: a selection action, a command on the
 * selection (`delete`, `rename`), or nothing.
 * @param {{key: string, shiftKey?: boolean, ctrlKey?: boolean, metaKey?: boolean}} evt
 */
export function keyAction(evt) {
  const mod = evt.ctrlKey || evt.metaKey;
  const extend = Boolean(evt.shiftKey);
  switch (evt.key) {
    case 'ArrowDown': return { type: 'move', delta: 1, extend };
    case 'ArrowUp': return { type: 'move', delta: -1, extend };
    case 'Home': return { type: 'move', to: 'first', extend };
    case 'End': return { type: 'move', to: 'last', extend };
    case ' ': return mod ? null : { type: 'toggle' };
    case 'Escape': return { type: 'none' };
    case 'Delete': return { command: 'delete' };
    case 'F2': return { command: 'rename' };
    default: return mod && evt.key.toLowerCase() === 'a' ? { type: 'all' } : null;
  }
}

/** The selected files and their total size (unknown sizes count none). */
export function selectionInfo(files, selected) {
  const chosen = files.filter((f) => selected.includes(f.path));
  return { files: chosen, count: chosen.length, bytes: chosen.reduce((sum, f) => sum + (f.size ?? 0), 0) };
}

// --------------------------------------------------------------- actions --

const allow = { enabled: true, reason: null };
const deny = (reason) => ({ enabled: false, reason });

/**
 * What the side toolbar of `medium` offers for the selected `files`, each
 * with the reason code it is disabled for (`busy`, `none`, `single`, `noCard`,
 * `deleteUnsupported`, `live`, `hasCopy`, `unknownSize`).
 * @param {{features: {remove: boolean, boot: boolean}, files: object[], medium: string, card: boolean, live: boolean, busy: boolean}} ctx
 */
export function actionsFor({ features, files, medium, card, live, busy }) {
  const common = (min, max = Infinity) => {
    if (busy) return deny('busy');
    if (files.length < min) return deny('none');
    if (files.length > max) return deny('single');
    return null;
  };
  const other = otherMedium(medium) === 'sd' && !card ? deny('noCard') : null;
  const removes = features.remove ? null : deny('deleteUnsupported');
  const single = files[0];
  return {
    move: common(1) ?? other ?? removes ?? allow,
    copy: common(1) ?? other ?? allow,
    rename: common(1, 1) ?? removes ?? allow,
    play: common(1, 1) ?? (live ? deny('live') : allow),
    boot: common(1, 1) ?? (features.boot ? allow : deny('deleteUnsupported')),
    delete: common(1) ?? removes ?? allow,
    associate: common(1, 1) ?? (hasCopy(single) ? deny('hasCopy') : null) ?? (single.size === null ? deny('unknownSize') : allow),
  };
}

// ----------------------------------------------------------------- plans --

/** A file and where it is: "earth.mp4 (SD card)". */
export function placeText(t, path) {
  return t('storage.plan.place', { name: baseName(path), medium: t(`storage.medium.${mediumOf(path)}`) });
}

/** One step of a plan: "earth.mp4 (Internal memory) → earth.mp4 (SD card) · 2.5 MB". */
export function stepText(t, locale, step) {
  return t('storage.plan.step', { source: placeText(t, step.source), target: placeText(t, step.target), size: formatBytes(step.size, locale) });
}

/** Why a plan leaves a file out. */
export function skipText(t, locale, skipped) {
  const conflict = skipped.conflict;
  return t(`storage.skip.${skipped.code}`, {
    name: baseName(skipped.source),
    target: placeText(t, skipped.target),
    size: formatBytes(conflict?.size, locale),
  });
}

/** What a plan's confirmation warns about. */
export function warningText(t, warning) {
  return t(`storage.warning.${warning.code}`, { name: baseName(warning.path) });
}

/**
 * Why a move, copy, rename or restore could not be planned; nothing was sent.
 * @param {{code: string, args?: object, message?: string}} r
 */
export function planRefusalText(t, locale, r) {
  const args = r.args ?? {};
  switch (r.code) {
    case 'noCard':
    case 'sameName':
      return t(`storage.plan.refused.${r.code}`);
    case 'notListed':
    case 'sameMedium':
      return t(`storage.plan.refused.${r.code}`, { name: baseName(args.path ?? '') });
    case 'invalidName':
      return args.char ? t('storage.plan.refused.invalidChar', { char: args.char }) : t('storage.plan.refused.invalidName', { max: MAX_NAME_BYTES });
    case 'extensionChanged':
      return args.expected ? t('storage.plan.refused.extensionChanged', { ext: args.expected }) : t('storage.plan.refused.noExtension');
    case 'unsendable':
      return t('storage.plan.refused.unsendable', { name: baseName(args.path ?? ''), reason: refusalText(t, locale, args.refusal ?? {}) });
    case 'noSpace': {
      const short = Math.max(0, args.needed + 1 - args.free);
      return t('storage.plan.refused.noSpace', { needed: formatBytes(args.needed, locale), free: formatBytes(args.free, locale), short: formatBytes(short, locale) });
    }
    default:
      return r.message ?? String(r.code);
  }
}

/**
 * The name a rename sends, as the core makes it (`FileName::for_upload`:
 * lower case, only `[a-z0-9_.-]`, no leading dot, the file's extension), and
 * what is wrong with it, as a plan refusal (`null`: nothing).
 * @param {string} raw what the user typed
 * @param {string} sourceName the file's name now
 */
export function renamePreview(raw, sourceName) {
  const name = raw.trim().replace(/[A-Z]+/g, (c) => c.toLowerCase());
  const refusal = (code, args = {}) => ({ name, problem: { code, args } });
  const bad = [...name].find((c) => !/[a-z0-9_.-]/.test(c));
  if (bad) return refusal('invalidName', { char: bad });
  if (!name || name.startsWith('.') || new TextEncoder().encode(name).length > MAX_NAME_BYTES) return refusal('invalidName');
  const extension = (n) => (n.includes('.') ? n.slice(n.lastIndexOf('.') + 1).toLowerCase() : null);
  const expected = extension(sourceName);
  if (extension(name) !== expected) return refusal('extensionChanged', { expected });
  if (name === sourceName.toLowerCase()) return refusal('sameName');
  return { name, problem: null };
}

/** Bytes a plan sends, and how many files. */
export function planTotals(plan) {
  return { count: plan.steps.length, bytes: plan.steps.reduce((sum, s) => sum + s.size, 0) };
}

/**
 * Why a file stopped its batch, by the core's halt code: the backend error of
 * a failure, the target's refusal, or the halt's own sentence (the file gone
 * or changed since the list, its local copy gone, its target's name taken).
 * @param {{halt?: string, error?: object|null, refusal?: object|null, conflict?: {path: string}|null}} why
 * @param {(t: Function, e: unknown) => string} errorText the backend error's sentence
 */
export function haltText(t, locale, why, errorText) {
  switch (why.halt) {
    case 'refused':
      return refusalText(t, locale, why.refusal ?? {});
    case 'conflict':
      return t('storage.halt.conflict', { name: baseName(why.conflict?.path ?? '') });
    case 'cancelled':
    case 'sourceChanged':
    case 'noLocalCopy':
      return t(`storage.halt.${why.halt}`);
    default:
      return errorText(t, why.error);
  }
}

/**
 * What a run did, as short sentences: what was done, what failed and why,
 * what was cancelled, where a moved or renamed file stands by how far it
 * came (its original still there; also its checked copy; or only the copy),
 * and what never started.
 * @param {{transfer: string, done: object[], failed: object|null, cancelled: object|null, notStarted: object[]}} report
 * @param {(t: Function, e: unknown) => string} errorText the backend error's sentence
 */
export function reportLines(t, locale, report, errorText) {
  const lines = [t(`storage.report.done.${report.transfer}`, { count: report.done.length })];
  if (report.failed) {
    const reason = haltText(t, locale, report.failed, errorText);
    lines.push(t('storage.report.failed', { name: baseName(report.failed.step.source), reason }));
  }
  if (report.cancelled) lines.push(t('storage.report.cancelled', { name: baseName(report.cancelled.step.source) }));
  const stopped = report.failed ?? report.cancelled;
  const deletes = report.transfer === 'move' || report.transfer === 'rename';
  // A stage this UI does not know says nothing it cannot vouch for.
  if (stopped && deletes && STAGES.includes(stopped.stage)) {
    lines.push(t(`storage.report.stage.${stopped.stage}`, { place: placeText(t, stopped.step.source), target: placeText(t, stopped.step.target) }));
  }
  if (report.notStarted.length) lines.push(t('storage.report.notStarted', { count: report.notStarted.length }));
  return lines;
}

// --------------------------------------------------------------- cleanup --

/** The files with a cleanup finding, grouped by code, the exact signals first. */
export function cleanupGroups(files) {
  return FINDING_CODES.map((code) => ({ code, files: files.filter((f) => f.finding?.code === code) })).filter((g) => g.files.length);
}

/** What the assistant checks at first: only the exact signals (D-2026-09-30-storage-manager-9). */
export function precheckedPaths(files) {
  return files.filter((f) => f.finding?.prechecked).map((f) => f.path);
}

/** Why a file is suggested, with the file that stays or the size Bezel stored. */
export function findingText(t, locale, file) {
  const f = file.finding;
  return t(`storage.finding.${f.code}`, { kept: baseName(f.kept ?? ''), cataloged: formatBytes(f.cataloged, locale) });
}

// --------------------------------------------------------------- restore --

/** The cataloged files a restore of `medium` offers: its missing ones and, for the card, another card's. */
export function restorableFor(restorable, medium) {
  return (restorable ?? []).filter((r) => r.medium === medium);
}

/** The chosen entries' count and bytes, and whether they fit `free` (each upload needs less than the free space). */
export function restoreTotals(entries, chosen, free) {
  const picked = entries.filter((e) => chosen.includes(e.id));
  const bytes = picked.reduce((sum, e) => sum + e.size, 0);
  return { count: picked.length, bytes, fits: bytes === 0 || bytes < free };
}

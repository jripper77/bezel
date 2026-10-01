// What the core decides for the storage manager, for demo mode: the cleanup
// findings (`domain::cleanup::findings`), upload names, the move, copy,
// rename and restore plans (`domain::archive::plan_*`) and the ranking of a
// file's originals on the PC (`rank_candidates`), over plain objects, with
// the same codes and rules. The demo backend keeps the state and runs them.

/** Where a rev C upload hangs: what such an upload leaves (core `HANG_PARTIAL_BYTES`). */
export const DEMO_HANG_PARTIAL = 29_577_216;
/** Longest upload name, bytes (core `FileName::MAX_BYTES`). */
const MAX_NAME = 208;

const MEDIUM_ORDER = Object.freeze({ internal: 0, sd: 1 });
const KIND_ORDER = Object.freeze({ image: 0, video: 1 });

/** A screen path's medium, folder and name. */
export function demoSplit(path) {
  const [medium, kind, ...rest] = String(path).split('/');
  return { medium, kind, name: rest.join('/') };
}

const asciiLower = (text) => text.replace(/[A-Z]+/g, (c) => c.toLowerCase());
const uploadChar = (c) => /^[a-z0-9_.-]$/.test(c);

/** The text after a name's last dot, or `null` (core `FileName::extension`). */
export const demoExtension = (name) => (name.includes('.') ? name.slice(name.lastIndexOf('.') + 1) : null);

/**
 * The `x.ext` a vendor name artifact stands for, lower-cased: `x.ext.ext…`
 * or `x.ext<digits>.ext` (`NVI.mp427034822.mp4`: `nvi.mp4`); `null` for any
 * other name (`8.8APEX_2.mp4`).
 */
export function demoArtifactBase(name) {
  const lower = asciiLower(name);
  const dot = lower.lastIndexOf('.');
  if (dot < 0 || dot === lower.length - 1) return null;
  const stem = lower.slice(0, dot);
  const suffix = lower.slice(dot);
  let core = stem;
  for (let at = core.lastIndexOf(suffix); at > 0 && /^\d*$/.test(core.slice(at + suffix.length)); at = core.lastIndexOf(suffix)) {
    core = core.slice(0, at);
  }
  return core.length < stem.length ? `${core}${suffix}` : null;
}

/** The name an upload gets (core `FileName::for_upload`): `{name}`, or `{error: {char?}}`. */
export function demoForUpload(raw) {
  const name = asciiLower(raw);
  const bad = [...name].find((c) => !uploadChar(c));
  if (bad) return { error: { char: bad } };
  if (!name || name.startsWith('.') || new TextEncoder().encode(name).length > MAX_NAME) return { error: {} };
  return { name };
}

/** A valid upload name made from any name (core `FileName::suggest`). */
export function demoSuggest(hostName, extension) {
  const dot = hostName.lastIndexOf('.');
  const stem = dot > 0 ? hostName.slice(0, dot) : hostName;
  const ext = [...asciiLower(extension)].filter((c) => uploadChar(c) && c !== '.').join('');
  let out = '';
  for (const c of asciiLower(stem)) {
    const kept = uploadChar(c) && c !== '.' ? c : '_';
    if (!(kept === '_' && out.endsWith('_'))) out += kept;
  }
  const base = out.replace(/^_+|_+$/g, '').slice(0, MAX_NAME - ext.length - (ext ? 1 : 0)) || 'media';
  return ext ? `${base}.${ext}` : base;
}

/** The name a file gets when sent again (core `archive::upload_name`): `NVI.mp4` → `nvi.mp4`. */
export function demoUploadName(name) {
  return demoForUpload(name).name ?? demoSuggest(name, demoExtension(name) ?? '');
}

/** Two paths a screen may store as one file: the same folder, the same name apart from letter case. */
export function demoSameFile(a, b) {
  const x = demoSplit(a);
  const y = demoSplit(b);
  return x.medium === y.medium && x.kind === y.kind && asciiLower(x.name) === asciiLower(y.name);
}

/** Path order of the core (`RemotePath`'s `Ord`): medium, folder, then name. */
function comparePaths(a, b) {
  const x = demoSplit(a);
  const y = demoSplit(b);
  return MEDIUM_ORDER[x.medium] - MEDIUM_ORDER[y.medium] || KIND_ORDER[x.kind] - KIND_ORDER[y.kind] || (x.name < y.name ? -1 : x.name > y.name ? 1 : 0);
}

/** Shortest name first, then by path (core `cleanup::shorter`). */
function shorter(a, b) {
  return demoSplit(a.path).name.length - demoSplit(b.path).name.length || comparePaths(a.path, b.path);
}

const OWN = 'own';

/** What the catalog alone says of a listed file (core `from_catalog`). */
function fromCatalog(file, entry) {
  if (entry && entry.state !== 'pending' && (file.size === null || file.size === entry.size)) return OWN;
  if (file.size === DEMO_HANG_PARTIAL) return { code: 'hangPartial' };
  if (entry?.state === 'pending') return { code: 'pending' };
  if (entry) return { code: 'sizeDiffers', cataloged: entry.size };
  return null;
}

function settle(verdicts, i, reason) {
  if (verdicts[i] === null) verdicts[i] = reason;
}

/** Vendor name artifacts grouped with the name they stand for (core `name_groups`). */
function nameGroups(files, verdicts) {
  const groups = new Map();
  files.forEach((file, i) => {
    const { medium, kind, name } = demoSplit(file.path);
    const base = demoArtifactBase(name);
    const key = `${medium}/${kind}/${base ?? asciiLower(name)}`;
    const group = groups.get(key) ?? { artifact: false, members: [] };
    group.artifact ||= base !== null;
    group.members.push(i);
    groups.set(key, group);
  });
  for (const { artifact, members } of groups.values()) {
    if (!artifact || members.length < 2) continue;
    members.sort((a, b) => shorter(files[a], files[b]));
    const kept = members[0];
    const bySize = new Map();
    for (const i of members) {
      const { size } = files[i];
      if (size !== null && bySize.has(size)) {
        settle(verdicts, i, { code: 'duplicate', kept: files[bySize.get(size)].path });
        continue;
      }
      if (size !== null) bySize.set(size, i);
      if (i !== kept) settle(verdicts, i, { code: 'variant', kept: files[kept].path });
    }
  }
}

/** Files of exactly the size and kind of a shorter-named one (core `same_sizes`). */
function sameSizes(files, verdicts) {
  const classes = new Map();
  files.forEach((file, i) => {
    if (file.size === null) return;
    const key = `${demoSplit(file.path).kind}/${file.size}`;
    classes.set(key, [...(classes.get(key) ?? []), i]);
  });
  for (const members of classes.values()) {
    if (members.length < 2) continue;
    members.sort((a, b) => shorter(files[a], files[b]));
    for (const i of members.slice(1)) settle(verdicts, i, { code: 'sameSize', kept: files[members[0]].path });
  }
}

const PRECHECKED = new Set(['duplicate', 'hangPartial', 'pending']);

/**
 * The cleanup findings of a listing (core `cleanup::findings`), by path: at
 * most one per file, `{code, prechecked, kept, cataloged}`. Files Bezel sent
 * and verified get none; protected files (the boot media, theme videos)
 * never appear.
 * @param {{path: string, size: number|null}[]} files every listed file
 * @param {(path: string) => object|null} entryAt the file's catalog entry here, not deleted
 * @param {(path: string) => boolean} isProtected
 */
export function demoFindings(files, entryAt, isProtected) {
  const verdicts = files.map((f) => fromCatalog(f, entryAt(f.path)));
  nameGroups(files, verdicts);
  sameSizes(files, verdicts);
  const found = new Map();
  files.forEach((file, i) => {
    if (verdicts[i] === OWN || isProtected(file.path)) return;
    const reason = verdicts[i] ?? { code: 'unused' };
    found.set(file.path, { code: reason.code, prechecked: PRECHECKED.has(reason.code), kept: reason.kept ?? null, cataloged: reason.cataloged ?? null });
  });
  return found;
}

// ----------------------------------------------------------------- plans --

const refused = (code, args = {}) => ({ status: 'refused', code, args, message: code });

function newPlan(transfer, to) {
  return { status: 'ready', transfer, to, steps: [], skipped: [], warnings: [] };
}

const fileDto = (path, size) => ({ path, ...demoSplit(path), size });

/** A target another step of `plan` already sends. */
function planned(plan, target) {
  const step = plan.steps.find((s) => demoSameFile(s.target, target));
  return step ? fileDto(step.target, step.size) : null;
}

/**
 * Adds the listed `source` to `target` (core `TransferPlan::add`). `view`:
 * `files` (path → size), `clash(path)`, `copyOf(path)` (the entry whose copy
 * is the listed file), `deletes`, `isBoot(path)`, `isThemeVideo(path)`.
 */
function add(plan, view, source, target, overwrite) {
  if (!view.files.has(source)) return refused('notListed', { path: source });
  const skip = (code, conflict = null) => {
    plan.skipped.push({ source, target, code, conflict });
    return null;
  };
  if (plan.transfer !== 'copy' && !view.deletes) return skip('deleteUnsupported');
  const entry = view.copyOf(source);
  if (!entry) return skip('noLocalCopy');
  const twice = planned(plan, target);
  if (twice) return skip('conflict', twice);
  const replaces = view.clash(target);
  if (replaces && !overwrite.includes(target)) return skip('conflict', replaces);
  if (plan.transfer !== 'copy' && view.isBoot(source)) plan.warnings.push({ code: 'bootMedia', path: source });
  if (plan.transfer === 'rename' && view.isThemeVideo(source)) plan.warnings.push({ code: 'themeVideo', path: source });
  plan.steps.push({ source, target, size: entry.size, content: entry.content, replaces });
  return null;
}

/** Plans moving or copying the listed `paths` to `to` (core `plan_move`, `plan_copy`). */
export function demoPlanAcross(view, transfer, paths, to, overwrite = []) {
  if (to === 'sd' && view.card === null) return refused('noCard');
  const plan = newPlan(transfer, to);
  for (const source of paths) {
    const { medium, kind, name } = demoSplit(source);
    if (medium === to) return refused('sameMedium', { path: source });
    const refusal = add(plan, view, source, `${to}/${kind}/${demoUploadName(name)}`, overwrite);
    if (refusal) return refusal;
  }
  return plan;
}

/** Plans renaming the listed `source` to `newName` on its medium (core `plan_rename`). */
export function demoPlanRename(view, source, newName, overwrite = []) {
  const upload = demoForUpload(newName);
  if (upload.error) return refused('invalidName', upload.error);
  const { medium, kind, name } = demoSplit(source);
  const expected = demoExtension(name)?.toLowerCase() ?? null;
  if (demoExtension(upload.name) !== expected) return refused('extensionChanged', { expected });
  if (upload.name === asciiLower(name)) return refused('sameName');
  const plan = newPlan('rename', medium);
  return add(plan, view, source, `${medium}/${kind}/${upload.name}`, overwrite) ?? plan;
}

/**
 * Plans restoring cataloged `entries` onto `to`, oldest first, under their
 * names (core `plan_restore`): present files skipped, conflicts unless
 * overwritten; every file within `cap` and their total below `free`, else
 * refused before anything is sent.
 */
export function demoPlanRestore(view, entries, to, { free, cap }, overwrite = []) {
  if (to === 'sd' && view.card === null) return refused('noCard');
  const plan = newPlan('restore', to);
  for (const entry of [...entries].sort((a, b) => a.sentAt - b.sentAt)) {
    const { kind, name } = demoSplit(entry.path);
    const target = `${to}/${kind}/${name}`;
    const skip = (code, conflict = null) => plan.skipped.push({ source: entry.path, target, code, conflict });
    const there = view.clash(target);
    const twice = planned(plan, target);
    if (!entry.localCopy) skip('noLocalCopy');
    else if (twice) skip('conflict', twice);
    else if (there && there.size === entry.size) skip('present');
    else if (there && !overwrite.includes(target)) skip('conflict', there);
    else if (!entry.size || entry.size > cap) {
      const refusal = entry.size ? { code: 'tooLarge', bytes: entry.size, limit: cap } : { code: 'emptyFile' };
      return refused('unsendable', { path: target, refusal });
    } else plan.steps.push({ source: entry.path, target, size: entry.size, content: entry.content, replaces: there });
  }
  const needed = plan.steps.reduce((sum, s) => sum + s.size, 0);
  if (needed > 0 && needed >= free) return refused('noSpace', { needed, free });
  return plan;
}

// ------------------------------------------------------------- associate --

/** A name's comparable part (core `name_key`): `NVI.mp427034822.mp4` → `nvi`. */
function nameKey(name) {
  const base = demoArtifactBase(name) ?? asciiLower(name);
  const stem = base.includes('.') ? base.slice(0, base.lastIndexOf('.')) : base;
  return [...stem].filter((c) => /[a-z0-9]/i.test(c)).join('').toLowerCase();
}

/**
 * The originals that can be `sought`'s, likeliest first (core
 * `rank_candidates`): exactly its size and kind, then the same name, the
 * longest common start of the name, the resolution and the play time.
 * @param {{path: string, size: number, resolution: {width:number,height:number}|null, durationMs: number|null}} sought
 * @param {{source: string, size: number, kind: string, resolution: object|null, durationMs: number|null}[]} candidates
 */
export function demoRank(sought, candidates) {
  const { kind, name } = demoSplit(sought.path);
  const wanted = nameKey(name);
  const likeness = (c) => {
    const key = nameKey(c.source.split(/[/\\]/).pop());
    let prefix = 0;
    while (prefix < Math.min(key.length, wanted.length) && key[prefix] === wanted[prefix]) prefix += 1;
    const resolution = Boolean(sought.resolution && c.resolution && c.resolution.width === sought.resolution.width && c.resolution.height === sought.resolution.height);
    const duration = sought.durationMs !== null && c.durationMs !== null && Math.abs(sought.durationMs - c.durationMs) <= 1000;
    return [key === wanted ? 1 : 0, prefix, resolution ? 1 : 0, duration ? 1 : 0];
  };
  const ranked = candidates.filter((c) => c.size === sought.size && c.kind === kind).map((c) => ({ c, score: likeness(c) }));
  ranked.sort((a, b) => {
    const diff = b.score.findIndex((v, i) => v !== a.score[i]);
    if (diff >= 0) return b.score[diff] - a.score[diff];
    return a.c.source < b.c.source ? -1 : a.c.source > b.c.source ? 1 : 0;
  });
  return ranked.map(({ c, score }) => ({ ...c, sameName: score[0] === 1 }));
}

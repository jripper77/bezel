// The storage tab of the "Tela" panel (D-2026-09-30-storage-video-6): usage
// of the internal flash and the SD card, the files of each, sending a file
// (dropped on a medium or chosen) with a progress bar and Cancel, Play/Stop,
// and Delete and the boot media behind a dialog that names the file. Sending
// always shows a summary first (file, target, size, conversion, replaced
// file); nothing is deleted or sent on its own.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { makeDraggable } from './dragdrop.js';

export const MEDIA = Object.freeze(['internal', 'sd']);
export const KINDS = Object.freeze(['image', 'video']);

/** Error codes of the storage commands that have their own text. */
export const ERROR_CODES = Object.freeze(['busy', 'unsupported', 'notConfirmed', 'inUse', 'timeout', 'stale', 'live', 'noVideo']);

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

/**
 * What the storage tab offers for a screen. TUR_USB screens take uploads and
 * play files but neither delete nor set the boot media through Bezel
 * (D-2026-09-30-storage-video-7).
 * @param {{family?: string, models?: {capabilities?: {storage?: boolean}}[]}|null} screen
 */
export function storageFeatures(screen) {
  const models = screen?.models ?? [];
  const storage = models.length > 0 && models.every((m) => Boolean(m.capabilities?.storage));
  const full = storage && screen.family !== 'turing-usb';
  return { storage, remove: full, boot: full };
}

/** Share of a medium in use, 0..1. */
export function usedFraction(capacity) {
  if (!capacity?.total) return 0;
  return Math.min(1, capacity.used / capacity.total);
}

/**
 * A job's progress as text: the phase, the amount done and the fraction
 * (`null` when the total is unknown).
 * @param {(k: string, p?: object) => string} t
 * @param {string} locale
 * @param {{phase: string, done: number, total: number}} p
 */
export function progressParts(t, locale, p) {
  const fraction = p.total > 0 ? Math.min(1, p.done / p.total) : null;
  const percent = fraction === null ? null : Math.floor(fraction * 100);
  let amount = '';
  if (p.phase === 'upload' && p.total > 0) {
    amount = t('storage.amountBytes', { done: formatBytes(p.done, locale), total: formatBytes(p.total, locale), percent });
  } else if (p.phase === 'convert' && percent !== null) {
    amount = t('storage.amountPercent', { percent });
  }
  return { phase: t(`storage.phase.${p.phase}`), amount, fraction };
}

/**
 * What the screen keeps with its boot media (rev C OPTIONS): the brightness
 * it starts with, the level set in this session or its own default, and no
 * sleep timer (D-2026-09-30-storage-video-5).
 * @param {(k: string, p?: object) => string} t
 * @param {number|null|undefined} brightness percent set in this session
 */
export function bootKeepsText(t, brightness) {
  return Number.isInteger(brightness) ? t('storage.bootKeeps', { percent: brightness }) : t('storage.bootKeepsDefault');
}

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
      return t('storage.refused.tooLarge', { size: formatBytes(r.bytes, locale), limit: formatBytes(r.limit, locale) });
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

/** A storage command's error (`{code, message}` or an Error) as text. */
export function errorMessage(t, e) {
  if (ERROR_CODES.includes(e?.code)) return t(`storage.error.${e.code}`);
  return t('storage.error.failed', { message: e?.message ?? String(e) });
}

/** The file name at the end of a local path or demo source. */
export function baseName(source) {
  return String(source).split(/[/\\]/).pop();
}

/**
 * Wires a tab list inside a panel (click, arrow keys); `onSelect(name)` gets
 * the `data-subtab` of the chosen tab.
 * @param {HTMLElement} tablist
 * @param {(name: string) => void} onSelect
 */
export function wireSubtabs(tablist, onSelect) {
  const tabs = [...tablist.querySelectorAll('[role="tab"]')];
  const select = (tab) => {
    for (const other of tabs) {
      const on = other === tab;
      other.setAttribute('aria-selected', String(on));
      other.tabIndex = on ? 0 : -1;
      document.getElementById(other.getAttribute('aria-controls')).hidden = !on;
    }
    onSelect(tab.dataset.subtab);
  };
  tabs.forEach((tab, i) => {
    tab.addEventListener('click', () => select(tab));
    tab.addEventListener('keydown', (evt) => {
      const step = evt.key === 'ArrowRight' ? 1 : evt.key === 'ArrowLeft' ? -1 : 0;
      if (!step) return;
      const next = tabs[(i + step + tabs.length) % tabs.length];
      select(next);
      next.focus();
    });
  });
}

/**
 * An accessible confirmation dialog (a modal `<dialog>`: focus stays inside,
 * Esc cancels, focus returns to the button that opened it).
 * @returns {(opts: {title: string, body: Node[], action: string, danger?: boolean}) => Promise<boolean>}
 */
export function createConfirm(t) {
  return ({ title, body, action, danger = false }) => new Promise((resolve) => {
    const opener = document.activeElement;
    const cancel = el('button', { type: 'button', class: 'text-button', text: t('dialog.cancel') });
    const ok = el('button', { type: 'button', class: danger ? 'danger-button' : 'primary-button', text: action });
    const close = el('button', { type: 'button', class: 'icon-button dialog-close', title: t('dialog.close'), 'aria-label': t('dialog.close') }, [icon(ICONS.close, 16)]);
    const dialog = el('dialog', { class: 'confirm-dialog', 'aria-labelledby': 'confirm-title', 'aria-describedby': 'confirm-body' }, [
      el('div', { class: 'dialog-head' }, [el('h2', { id: 'confirm-title', text: title }), close]),
      el('div', { id: 'confirm-body', class: 'dialog-body' }, body),
      el('div', { class: 'dialog-actions' }, [cancel, ok]),
    ]);
    cancel.addEventListener('click', () => dialog.close('cancel'));
    close.addEventListener('click', () => dialog.close('cancel'));
    ok.addEventListener('click', () => dialog.close('ok'));
    dialog.addEventListener('close', () => {
      const yes = dialog.returnValue === 'ok';
      dialog.remove();
      if (opener?.isConnected) opener.focus();
      resolve(yes);
    }, { once: true });
    document.body.append(dialog);
    dialog.showModal();
    // A destructive answer is never the default one.
    (danger ? cancel : ok).focus();
  });
}

/**
 * @param {object} deps
 * @param {HTMLElement} deps.root where the tab is drawn
 * @param {(k: string, p?: object) => string} deps.t
 * @param {string} deps.locale
 * @param {object} deps.bridge
 * @param {(message: string) => void} deps.notify short confirmation (toast)
 * @param {() => {screen: object|null, live: boolean, liveVideo: {state: string, path?: string}|null}} deps.context
 */
export function createStoragePanel({ root, t, locale, bridge, notify, context }) {
  const confirm = createConfirm(t);
  const view = {
    key: null,
    shown: false,
    status: 'idle',
    data: null,
    error: null,
    tools: null,
    job: null,
    working: false,
    notice: null,
    signature: '',
  };
  // New notices are read out; each one is a region named by its heading.
  const notices = el('div', { class: 'storage-notices', 'aria-live': 'polite' });
  const jobBox = el('div', { class: 'storage-job', hidden: true });
  const body = el('div', { class: 'storage-body' });
  root.replaceChildren(notices, jobBox, body);

  const screen = () => context().screen;
  const busy = () => Boolean(view.job) || view.working;
  const bytes = (n) => formatBytes(n, locale);

  // ------------------------------------------------------------- loading --
  async function load() {
    const current = screen();
    if (!current || !storageFeatures(current).storage) {
      view.status = 'idle';
      renderAll();
      return;
    }
    const key = current.key;
    view.status = 'loading';
    renderBody();
    try {
      const [data, tools] = await Promise.all([bridge.storageOverview(key), bridge.mediaTools()]);
      if (key !== view.key) return;
      Object.assign(view, { data, tools, status: 'ready', error: null });
    } catch (e) {
      if (key !== view.key) return;
      Object.assign(view, { status: 'error', error: e });
    }
    renderAll();
  }

  /** Runs `work` with every action disabled, then reloads the lists. */
  async function act(work, success, { reload = true } = {}) {
    view.working = true;
    view.notice = null;
    renderAll();
    try {
      await work();
      if (success) notify(success);
    } catch (e) {
      view.notice = { kind: 'error', text: errorMessage(t, e) };
    }
    view.working = false;
    if (reload) await load();
    else renderAll();
  }

  // ------------------------------------------------------------- sending --
  async function choose(medium) {
    let source = null;
    try {
      source = await bridge.pickMedia();
    } catch (e) {
      view.notice = { kind: 'error', text: errorMessage(t, e) };
      renderNotices();
      return;
    }
    if (source) await send(source, medium);
  }

  async function send(source, medium) {
    if (busy() || !view.key) return;
    await prepare(() => bridge.prepareUpload(view.key, source, medium), baseName(source));
  }

  async function sendThemeVideo() {
    if (busy() || !view.key) return;
    await prepare(() => bridge.prepareThemeVideo(view.key), t('storage.themeVideo'));
  }

  async function prepare(ask, label) {
    view.working = true;
    view.notice = null;
    renderAll();
    let answer = null;
    try {
      answer = await ask();
    } catch (e) {
      view.notice = { kind: 'error', text: errorMessage(t, e) };
    }
    view.working = false;
    if (answer?.status === 'refused') view.notice = { kind: 'refused', name: label, refusal: answer };
    renderAll();
    if (answer?.status !== 'ready') return;
    if (await confirm(summaryDialog(answer))) await runJob(answer);
  }

  function summaryDialog(p) {
    const place = t(`storage.in.${p.target.medium}`);
    const folder = t(`storage.kind.${p.target.kind}`).toLowerCase();
    const size = [bytes(p.bytes), p.format, p.dimensions ? `${p.dimensions.width}×${p.dimensions.height}` : null].filter(Boolean).join(' · ');
    let conversion = t('storage.summary.asIs');
    if (p.convert) {
      const extras = [];
      if (p.convert.quarterTurns) extras.push(t('storage.summary.turned', { degrees: p.convert.quarterTurns * 90 }));
      if (p.convert.cropped) extras.push(t('storage.summary.cropped'));
      conversion = t('storage.summary.converted', { width: p.convert.width, height: p.convert.height }) + extras.map((x) => `, ${x}`).join('');
    }
    const row = (label, value) => [el('dt', { text: label }), el('dd', { text: value })];
    const content = [
      el('dl', { class: 'summary' }, [
        ...row(t('storage.summary.file'), `${p.source} (${size})`),
        ...row(t('storage.summary.target'), t('storage.summary.targetValue', { name: p.target.name, place, folder })),
        ...row(t('storage.summary.convert'), conversion),
      ]),
    ];
    if (p.replaces) {
      content.push(el('p', { class: 'dialog-warning' }, [
        icon(ICONS.warning, 18),
        el('span', { text: t('storage.summary.replaces', { name: p.replaces.name, size: bytes(p.replaces.size), place }) }),
      ]));
    }
    return {
      title: t('storage.confirmUploadTitle', { name: p.target.name }),
      body: content,
      action: p.replaces ? t('storage.confirmReplaceAction') : t('storage.confirmUploadAction'),
    };
  }

  async function runJob(prepared) {
    view.job = { name: prepared.target.name, phase: prepared.convert ? 'convert' : 'upload', done: 0, total: 0, cancelling: false };
    view.notice = null;
    renderAll();
    let result = null;
    try {
      result = await bridge.runUpload(prepared.ticket, Boolean(prepared.replaces));
    } catch (e) {
      view.notice = { kind: 'error', text: errorMessage(t, e) };
    }
    const { name } = view.job;
    view.job = null;
    if (result?.status === 'done') notify(t('storage.uploaded', { name }));
    if (result?.status === 'cancelled') view.notice = { kind: 'cancelled', name, path: result.path, partial: result.partial };
    await load();
  }

  function cancelJob() {
    if (!view.job || view.job.cancelling) return;
    view.job.cancelling = true;
    updateJob();
    Promise.resolve(bridge.cancelJob()).catch(() => {});
  }

  bridge.onJobProgress?.((p) => {
    if (!view.job) return;
    Object.assign(view.job, p);
    updateJob();
  });

  // ------------------------------------------------------ files and boot --
  const fileOf = (path) => ({ path, name: baseName(path), medium: path.split('/')[0] });

  async function askDelete(file) {
    const place = t(`storage.from.${file.medium}`);
    const ok = await confirm({
      title: t('storage.confirmDeleteTitle', { name: file.name }),
      body: [el('p', { text: t('storage.confirmDelete', { name: file.name, place }) })],
      action: t('storage.deleteAction'),
      danger: true,
    });
    if (ok) await act(() => bridge.deleteStored(view.key, file.path, true), t('storage.deleted', { name: file.name }));
  }

  // The brightness the dialog names is the one sent with the boot media.
  const bootBrightness = () => context().brightness?.[view.key] ?? null;

  async function askBoot(file) {
    const brightness = bootBrightness();
    const ok = await confirm({
      title: t('storage.confirmBootTitle', { name: file.name }),
      body: [el('p', { text: t('storage.confirmBoot', { name: file.name }) }), el('p', { text: bootKeepsText(t, brightness) })],
      action: t('storage.bootAction'),
    });
    if (ok) await act(() => bridge.setBootMedia(view.key, file.path, true, brightness), t('storage.bootSet', { name: file.name }), { reload: false });
  }

  async function askBootDefault() {
    const brightness = bootBrightness();
    const ok = await confirm({
      title: t('storage.confirmDefaultTitle'),
      body: [el('p', { text: t('storage.confirmDefault') }), el('p', { text: bootKeepsText(t, brightness) })],
      action: t('storage.defaultAction'),
    });
    if (ok) await act(() => bridge.setBootMedia(view.key, null, true, brightness), t('storage.bootReset'), { reload: false });
  }

  const play = (file) => act(() => bridge.playStored(view.key, file.path), t('storage.playing', { name: file.name }), { reload: false });
  const stop = () => act(() => bridge.stopPlayback(view.key), t('storage.stopped'), { reload: false });

  async function locate() {
    try {
      const tools = await bridge.locateFfmpeg();
      if (!tools) return;
      view.tools = tools;
      view.notice = tools.rejected ? { kind: 'error', text: t('storage.ffmpegRejected', { path: tools.rejected }) } : null;
      if (tools.ready) notify(t('storage.ffmpegReady', { version: tools.version ?? '' }));
    } catch (e) {
      view.notice = { kind: 'error', text: errorMessage(t, e) };
    }
    renderNotices();
  }

  // ------------------------------------------------------------ notices --
  function notice(kind, iconPaths, title, children, { dismiss = false } = {}) {
    const id = `storage-notice-${kind}`;
    const head = [icon(iconPaths, 18), el('h2', { id, text: title })];
    if (dismiss) {
      head.push(el('button', {
        type: 'button', class: 'icon-button', title: t('storage.dismiss'), 'aria-label': t('storage.dismiss'),
        onclick: () => { view.notice = null; renderNotices(); body.querySelector('button')?.focus(); },
      }, [icon(ICONS.close, 16)]));
    }
    return el('section', { class: `notice ${kind}`, 'aria-labelledby': id }, [el('div', { class: 'notice-head' }, head), ...children]);
  }

  function resultNotice() {
    const n = view.notice;
    if (!n) return null;
    if (n.kind === 'cancelled') {
      const children = [el('p', { text: n.partial ? t('storage.partial', { size: bytes(n.partial), name: n.name }) : t('storage.nothingLeft') })];
      if (n.partial && storageFeatures(screen()).remove) {
        children.push(el('div', { class: 'button-row' }, [
          el('button', { type: 'button', class: 'text-button', text: t('storage.deletePartial'), disabled: busy(), onclick: () => askDelete(fileOf(n.path)) }),
        ]));
      }
      return notice('cancelled', ICONS.info, t('storage.cancelled'), children, { dismiss: true });
    }
    if (n.kind === 'refused') {
      const children = [el('p', { text: refusalText(t, locale, n.refusal) })];
      const candidates = n.refusal.candidates ?? [];
      if (candidates.length && storageFeatures(screen()).remove) {
        children.push(el('p', { text: t('storage.candidates') }));
        children.push(el('ul', { class: 'stored-files compact' }, candidates.map((c) => el('li', { class: 'stored-file' }, [
          el('span', { class: 'file-name', title: c.name, text: c.name }),
          el('span', { class: 'file-size', text: bytes(c.size) }),
          el('button', { type: 'button', class: 'icon-button', title: t('storage.delete', { name: c.name }), 'aria-label': t('storage.delete', { name: c.name }), disabled: busy(), onclick: () => askDelete(c) }, [icon(ICONS.trash, 16)]),
        ]))));
      }
      if (n.refusal.code === 'needsConverter') children.push(ffmpegHelp());
      return notice('refused', ICONS.warning, t('storage.refusedTitle', { name: n.name }), children, { dismiss: true });
    }
    return notice('error', ICONS.warning, t('storage.errorTitle'), [el('p', { text: n.text })], { dismiss: true });
  }

  function ffmpegHelp() {
    const hints = view.tools?.installHints ?? [];
    return el('div', { class: 'ffmpeg-help' }, [
      hints.length ? el('ul', { class: 'hints' }, hints.map((h) => el('li', {}, [el('code', { text: h })]))) : null,
      el('div', { class: 'button-row' }, [
        el('button', { type: 'button', class: 'text-button', text: t('storage.ffmpegLocate'), disabled: busy(), onclick: locate }),
      ]),
    ]);
  }

  function renderNotices() {
    const { live, liveVideo } = context();
    const current = screen();
    const features = storageFeatures(current);
    const parts = [resultNotice()];
    if (features.storage && live && liveVideo?.state === 'missing') {
      parts.push(notice('video', ICONS.film, t('storage.videoMissingTitle'), [
        el('p', { text: t('storage.videoMissing') }),
        el('div', { class: 'button-row' }, [
          el('button', { type: 'button', class: 'primary-button', text: t('storage.sendVideo'), disabled: busy(), onclick: sendThemeVideo }),
        ]),
      ]));
    }
    if (features.storage && view.tools && !view.tools.ready && view.notice?.refusal?.code !== 'needsConverter') {
      parts.push(notice('ffmpeg', ICONS.info, t('storage.ffmpegMissingTitle'), [el('p', { text: t('storage.ffmpegMissing') }), ffmpegHelp()]));
    }
    notices.replaceChildren(...parts.filter(Boolean));
  }

  // ----------------------------------------------------------------- job --
  const jobTitle = el('strong', { class: 'job-title' });
  const jobPhase = el('span', { class: 'job-phase', 'aria-live': 'polite' });
  const jobAmount = el('span', { class: 'job-amount' });
  const jobBar = el('progress', { class: 'job-bar', max: 1, 'aria-label': t('storage.progressLabel') });
  const jobCancel = el('button', { type: 'button', class: 'text-button', text: t('storage.cancel'), onclick: cancelJob });
  jobBox.replaceChildren(
    el('div', { class: 'job-head' }, [icon(ICONS.upload, 18), jobTitle]),
    jobBar,
    el('div', { class: 'job-status' }, [jobPhase, jobAmount]),
    el('p', { class: 'hint', text: t('storage.busy') }),
    el('div', { class: 'button-row' }, [jobCancel]),
  );

  function updateJob() {
    const job = view.job;
    jobBox.hidden = !job;
    if (!job) return;
    const parts = progressParts(t, locale, job);
    jobTitle.textContent = t('storage.jobTitle', { name: job.name });
    jobPhase.textContent = job.cancelling ? t('storage.cancelling') : parts.phase;
    jobAmount.textContent = parts.amount;
    if (parts.fraction === null) jobBar.removeAttribute('value');
    else jobBar.value = parts.fraction;
    jobCancel.disabled = job.cancelling;
  }

  // ---------------------------------------------------------------- body --
  function bootSlot(features) {
    if (!features.boot) return null;
    const slot = el('section', { class: 'boot-slot', 'aria-labelledby': 'storage-boot-title' }, [
      el('h3', { id: 'storage-boot-title' }, [icon(ICONS.power, 16), t('storage.bootTitle')]),
      el('p', { class: 'hint', text: t('storage.bootHelp') }),
      el('div', { class: 'button-row' }, [
        el('button', { type: 'button', class: 'text-button', text: t('storage.defaultAction'), disabled: busy(), onclick: askBootDefault }),
      ]),
    ]);
    return slot;
  }

  function fileRow(file, features, slot) {
    const { live } = context();
    const name = el('span', { class: `file-name${slot && !busy() ? ' draggable' : ''}`, title: file.name, text: file.name });
    if (slot && !busy()) {
      const target = {
        containsClient: (x, y) => {
          const r = slot.getBoundingClientRect();
          return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
        },
        point: () => ({ x: 0, y: 0 }),
      };
      makeDraggable(name, { label: file.name, canvas: target, stage: slot, onDrop: () => askBoot(file), onClick: () => {} });
    }
    const button = (label, paths, onclick, disabled, reason) => el('button', {
      type: 'button', class: 'icon-button', title: disabled && reason ? reason : label, 'aria-label': label, disabled, onclick,
    }, [icon(paths, 16)]);
    return el('li', { class: 'stored-file' }, [
      icon(file.kind === 'video' ? ICONS.film : ICONS.image, 16),
      name,
      el('span', { class: 'file-size', text: bytes(file.size) }),
      button(t('storage.play', { name: file.name }), ICONS.play, () => play(file), busy() || live, live ? t('storage.liveReason') : null),
      features.boot ? button(t('storage.boot', { name: file.name }), ICONS.power, () => askBoot(file), busy()) : null,
      features.remove ? button(t('storage.delete', { name: file.name }), ICONS.trash, () => askDelete(file), busy()) : null,
    ]);
  }

  function dropZone(medium) {
    const zone = el('div', { class: 'drop-zone', dataset: { medium } }, [
      icon(ICONS.upload, 20),
      el('span', { text: t('storage.dropHere') }),
      el('button', { type: 'button', class: 'text-button', text: t('storage.choose'), disabled: busy(), onclick: () => choose(medium) }),
    ]);
    zone.addEventListener('dragover', (evt) => {
      evt.preventDefault();
      zone.classList.add('drag-over');
    });
    zone.addEventListener('dragleave', () => zone.classList.remove('drag-over'));
    zone.addEventListener('drop', (evt) => {
      evt.preventDefault();
      zone.classList.remove('drag-over');
      const source = bridge.fileSource?.(evt.dataTransfer?.files?.[0]);
      if (source) send(source, medium);
    });
    return zone;
  }

  function folder(f, features, slot) {
    const id = `storage-${f.medium}-${f.kind}`;
    let content;
    if (f.error) content = el('p', { class: 'empty-note', text: t('storage.folderError', { reason: errorMessage(t, f.error) }) });
    else if (!f.files.length) content = el('p', { class: 'empty-note', text: t(`storage.empty.${f.kind}`) });
    else content = el('ul', { class: 'stored-files', 'aria-labelledby': id }, f.files.map((file) => fileRow(file, features, slot)));
    return el('div', { class: 'folder' }, [
      el('h4', { id, text: `${t(`storage.kind.${f.kind}`)} (${f.files.length})` }),
      content,
    ]);
  }

  function mediumSection(medium, features, slot) {
    const capacity = medium === 'internal' ? view.data.internal : view.data.card;
    const id = `storage-${medium}-title`;
    const children = [el('h3', { id }, [icon(medium === 'sd' ? ICONS.card : ICONS.chip, 16), t(`storage.medium.${medium}`)])];
    if (!capacity) {
      children.push(el('p', { class: 'empty-note', text: t('storage.noCard') }));
    } else {
      const fraction = usedFraction(capacity);
      children.push(
        // A sliver stays visible for a little use.
        el('div', { class: `usage${fraction > 0.9 ? ' full' : ''}`, 'aria-hidden': 'true' }, [el('span', { style: { width: capacity.used ? `max(4px, ${(fraction * 100).toFixed(1)}%)` : '0' } })]),
        el('p', { class: 'usage-text', text: t('storage.usage', { used: bytes(capacity.used), total: bytes(capacity.total), free: bytes(capacity.free) }) }),
        dropZone(medium),
        ...view.data.folders.filter((f) => f.medium === medium).map((f) => folder(f, features, slot)),
      );
    }
    if (medium === 'sd') children.push(el('p', { class: 'hint', text: t('storage.cardHelp') }));
    return el('section', { class: 'medium', 'aria-labelledby': id }, children);
  }

  function renderBody() {
    const current = screen();
    const features = storageFeatures(current);
    body.setAttribute('aria-busy', String(view.status === 'loading'));
    if (!current) {
      body.replaceChildren(el('p', { class: 'empty-note', text: t('screen.none') }));
      return;
    }
    if (!features.storage) {
      body.replaceChildren(el('p', { class: 'empty-note', text: t('storage.unsupported') }));
      return;
    }
    if (view.status === 'loading' && !view.data) {
      body.replaceChildren(el('p', { class: 'loading-note', text: t('storage.loading') }));
      return;
    }
    if (view.status === 'error') {
      body.replaceChildren(
        el('p', { class: 'empty-note', role: 'alert', text: t('storage.loadError', { message: errorMessage(t, view.error) }) }),
        el('div', { class: 'button-row' }, [el('button', { type: 'button', class: 'text-button', text: t('storage.retry'), onclick: load })]),
      );
      return;
    }
    if (!view.data) {
      body.replaceChildren();
      return;
    }
    const { live } = context();
    const slot = bootSlot(features);
    const hints = [];
    if (live) hints.push(el('p', { class: 'hint', text: t('storage.liveHint') }));
    if (!features.remove) hints.push(el('p', { class: 'hint', text: t('storage.limitedHint') }));
    if (view.tools?.ready) hints.push(el('p', { class: 'hint' }, [t('storage.ffmpegReady', { version: view.tools.version ?? '' })]));
    body.replaceChildren(
      el('div', { class: 'button-row storage-toolbar' }, [
        el('button', { type: 'button', class: 'text-button', disabled: busy(), onclick: load }, [icon(ICONS.refresh, 16), el('span', { text: t('storage.refresh') })]),
        el('button', { type: 'button', class: 'text-button', disabled: busy() || live, title: live ? t('storage.liveReason') : null, onclick: stop }, [icon(ICONS.stop, 16), el('span', { text: t('storage.stop') })]),
      ]),
      ...hints,
      ...(slot ? [slot] : []),
      ...MEDIA.map((m) => mediumSection(m, features, slot)),
    );
  }

  function renderAll() {
    renderNotices();
    updateJob();
    renderBody();
  }

  // Files dropped from the system on a medium (the app's own drag-and-drop:
  // Tauri reports the paths and the pointer in physical pixels).
  bridge.onFileDrop?.((evt) => {
    for (const z of root.querySelectorAll('.drop-zone.drag-over')) z.classList.remove('drag-over');
    if (!view.shown || evt.type === 'leave' || !evt.position) return;
    const ratio = window.devicePixelRatio || 1;
    const zone = document.elementFromPoint(evt.position.x / ratio, evt.position.y / ratio)?.closest('.drop-zone');
    if (!zone || !root.contains(zone)) return;
    if (evt.type === 'drop' && evt.paths?.length) send(evt.paths[0], zone.dataset.medium);
    else zone.classList.add('drag-over');
  });

  return {
    /** The tab became visible: load what the screen stores. */
    show() {
      view.shown = true;
      if (view.status === 'idle' || view.status === 'error') load();
      else renderAll();
    },
    hide() {
      view.shown = false;
    },
    /** The screen, live mode or the theme's video changed. */
    update() {
      const current = screen();
      const key = current?.key ?? null;
      if (key !== view.key) {
        Object.assign(view, { key, data: null, error: null, notice: null, status: 'idle' });
        if (view.shown && !busy()) load();
        else renderAll();
        return;
      }
      const { live, liveVideo } = context();
      const signature = JSON.stringify([live, liveVideo?.state ?? null, current?.family ?? null]);
      if (signature === view.signature) return;
      view.signature = signature;
      renderNotices();
      if (!view.job) renderBody();
    },
    refresh: load,
  };
}

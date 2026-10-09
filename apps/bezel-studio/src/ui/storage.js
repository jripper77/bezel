// The storage tab of the "Tela" panel (D-2026-09-30-storage-video-6), grown
// into the full-width storage manager (D-2026-09-30-storage-manager-4, -13):
// usage and files of the internal flash and the SD card side by side, sending
// a file (dropped on a medium or chosen) with a progress bar and Cancel,
// Play/Stop, Delete and the boot media behind a dialog that names the file,
// and the manager's jobs (move, copy, rename, restore, cleanup) with their
// progress announced and their report. Sending always shows a summary first;
// nothing is deleted or sent on its own. The lists and the manager's dialogs
// are drawn by `manager.js`.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { errorText } from '../messages.js';
import { udevCommand } from './udev.js';
import { GLYPHS, createManagerView } from './manager.js';
import {
  KINDS, MEDIA, baseName, formatBytes, formatMiB, haltText, mismatchText, planRefusalText, refusalText, reportLines, storageFeatures,
} from '../storage-manager.js';

export { KINDS, MEDIA, baseName, formatBytes, formatMiB, mismatchText, refusalText, storageFeatures };

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

/**
 * The title of a running job: the upload, or the file a manager job is at
 * and its place in the job.
 * @param {(k: string, p?: object) => string} t
 * @param {{kind: string, transfer?: string, name: string, step?: {index: number, count: number}|null}} job
 */
export function jobTitle(t, job) {
  if (job.kind === 'upload') return t('storage.jobTitle', { name: job.name });
  const place = { name: job.name, index: (job.step?.index ?? 0) + 1, count: job.step?.count ?? 1 };
  return job.kind === 'plan' ? t(`storage.job.${job.transfer}`, place) : t('storage.job.delete', place);
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
export function createConfirm(t, { refocus = () => {} } = {}) {
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
      else refocus();
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
 * @param {() => string} deps.locale the UI's language now
 * @param {object} deps.bridge
 * @param {(message: string) => void} deps.notify short confirmation (toast)
 * @param {() => {screen: object|null, live: boolean, liveVideo: {state: string, path?: string}|null}} deps.context
 * @param {(key: string) => void} [deps.restart] restarts a screen that stopped responding
 */
export function createStoragePanel({ root, t, locale, bridge, notify, context, restart = () => {}, useBackground = () => {} }) {
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
    // The control that had the focus last, to give it back when the tab is drawn again.
    focusKey: null,
  };
  // New notices are read out; each one is a region named by its heading.
  const notices = el('div', { class: 'storage-notices', 'aria-live': 'polite' });
  const jobBox = el('div', { class: 'storage-job', hidden: true });
  // A manager job's progress, file by file, for screen readers.
  const announcer = el('p', { class: 'visually-hidden', role: 'status', 'aria-live': 'polite' });
  const body = el('div', { class: 'storage-body' });
  root.replaceChildren(notices, jobBox, announcer, body);
  root.addEventListener('focusin', (evt) => {
    const key = evt.target.closest?.('[data-focus]')?.dataset.focus;
    if (key) view.focusKey = key;
  });

  /** Gives the focus back to the control that had it, when it was lost with a redraw. */
  function refocus() {
    if (document.querySelector('dialog[open]')) return;
    const active = document.activeElement;
    if (active && active !== document.body && active.isConnected) return;
    const next = view.focusKey && root.querySelector(`[data-focus="${view.focusKey}"]`);
    if (next && !next.disabled) next.focus();
  }

  const confirm = createConfirm(t, { refocus });
  const screen = () => context().screen;
  const busy = () => Boolean(view.job) || view.working;
  const bytes = (n) => formatBytes(n, locale());
  const announce = (text) => {
    announcer.textContent = text;
  };

  /**
   * A command's error as a notice (with the udev command when it fixes it,
   * and the restart when the screen stopped responding).
   */
  const errorNotice = (e) => ({ kind: 'error', text: errorText(t, e), command: e?.udevCommand ?? null, hung: e?.code === 'hung' });

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
      const [data, tools] = await Promise.all([bridge.managerOverview(key), bridge.mediaTools()]);
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
      view.notice = errorNotice(e);
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
      view.notice = errorNotice(e);
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
      view.notice = errorNotice(e);
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
    view.job = { kind: 'upload', name: prepared.target.name, phase: prepared.convert ? 'convert' : 'upload', done: 0, total: 0, step: null, cancelling: false };
    view.notice = null;
    renderAll();
    let result = null;
    try {
      result = await bridge.runUpload(prepared.ticket, Boolean(prepared.replaces));
    } catch (e) {
      // A cancel mid-transfer can leave the firmware waiting for the rest of
      // the file: the link times out, the next operation reconnects, and a
      // partial file may remain (seen on the 8.8").
      view.notice = view.job?.cancelling && e?.code === 'timeout'
        ? { kind: 'error', text: t('storage.cancelledLost', { name: view.job.name }) }
        : { ...errorNotice(e), path: e?.code === 'sizeMismatch' ? e.args?.file : null };
    }
    const { name } = view.job;
    view.job = null;
    if (result?.status === 'done') notify(t('storage.uploaded', { name }));
    if (result?.status === 'cancelled') view.notice = { kind: 'cancelled', name, path: result.path, partial: result.partial };
    // A conversion the screen would not take is refused before a byte is sent.
    if (result?.status === 'refused') view.notice = { kind: 'refused', name, refusal: result };
    await load();
  }

  // ---------------------------------------------------- the manager's jobs --
  /** Starts a manager job: the job box shows the first file, the status region names it. */
  function startJob(job) {
    view.job = { phase: job.kind === 'plan' ? 'upload' : 'delete', done: 0, total: 0, cancelling: false, ...job };
    view.notice = null;
    renderAll();
    announce(jobTitle(t, view.job));
  }

  /**
   * Runs a confirmed plan one file at a time (D-2026-09-30-storage-manager-7,
   * -8): the report lists what was done, what failed and why, and what never
   * started; a cancelled upload's partial file is offered for a delete. A
   * restore that no longer fits is refused before anything is sent.
   */
  async function runPlan(plan) {
    const first = plan.steps[0];
    startJob({ kind: 'plan', transfer: plan.transfer, name: baseName(first.source), step: { index: 0, count: plan.steps.length, source: first.source, target: first.target } });
    try {
      // Only the confirmation's OK leads here.
      const run = await bridge.runPlan(plan.ticket, true);
      view.notice = run.status === 'refused'
        ? { kind: 'planRefused', text: planRefusalText(t, locale(), run) }
        : { kind: 'report', report: run };
    } catch (e) {
      view.notice = errorNotice(e);
    }
    view.job = null;
    await load();
  }

  /**
   * Deletes the confirmed files one by one (a selection, or the cleanup's
   * list), each with the size its dialog listed: one that changed since is
   * not deleted.
   */
  async function runDeletes(files) {
    const first = files[0].path;
    startJob({ kind: 'delete', name: baseName(first), step: { index: 0, count: files.length, source: first, target: null } });
    try {
      const confirmed = files.map(({ path, size }) => ({ path, size }));
      view.notice = { kind: 'deleteReport', report: await bridge.deleteFiles(view.key, confirmed, true) };
    } catch (e) {
      view.notice = errorNotice(e);
    }
    view.job = null;
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
    const next = p.step && p.step.index !== view.job.step?.index;
    Object.assign(view.job, p);
    if (p.step) view.job.name = baseName(p.step.source);
    updateJob();
    if (next) announce(jobTitle(t, view.job));
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
    if (ok) await act(() => bridge.setBootMedia(view.key, file.path, true, brightness), t('storage.bootSet', { name: file.name }));
  }

  async function askBootDefault() {
    const brightness = bootBrightness();
    const ok = await confirm({
      title: t('storage.confirmDefaultTitle'),
      body: [el('p', { text: t('storage.confirmDefault') }), el('p', { text: bootKeepsText(t, brightness) })],
      action: t('storage.defaultAction'),
    });
    if (ok) await act(() => bridge.setBootMedia(view.key, null, true, brightness), t('storage.bootReset'));
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
      view.notice = errorNotice(e);
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

  const buttonRow = (...buttons) => el('div', { class: 'button-row' }, buttons);
  const deletePartialButton = (path) => el('button', { type: 'button', class: 'text-button', text: t('storage.deletePartial'), disabled: busy(), onclick: () => askDelete(fileOf(path)) });
  const restartButton = () => el('button', { type: 'button', class: 'primary-button', text: t('screen.restart'), disabled: busy(), onclick: () => restart(view.key) });

  /** What a plan run did: done, failed, cancelled, not started (D-2026-09-30-storage-manager-7). */
  function reportNotice(report) {
    const stopped = Boolean(report.failed || report.cancelled);
    const children = reportLines(t, locale(), report, errorText).map((text) => el('p', { text }));
    const partial = report.cancelled?.partial ? report.cancelled.step.target : null;
    if (partial && storageFeatures(screen()).remove) {
      children.push(el('p', { text: t('storage.partial', { size: bytes(report.cancelled.partial), name: baseName(partial) }) }), buttonRow(deletePartialButton(partial)));
    }
    if (report.failed?.error?.code === 'hung' && screen()?.restartable) children.push(buttonRow(restartButton()));
    return notice(stopped ? 'error' : 'report', stopped ? ICONS.warning : ICONS.info, t(stopped ? 'storage.report.stopped' : 'storage.report.finished'), children, { dismiss: true });
  }

  function deleteReportNotice(report) {
    const stopped = Boolean(report.failed || report.cancelled);
    const children = [el('p', { text: t('storage.report.deleted', { count: report.deleted.length, size: bytes(report.freed) }) })];
    if (report.failed) children.push(el('p', { text: t('storage.report.deleteFailed', { name: baseName(report.failed.path), reason: haltText(t, locale(), report.failed, errorText) }) }));
    if (report.notStarted.length) children.push(el('p', { text: t('storage.report.notStarted', { count: report.notStarted.length }) }));
    return notice(stopped ? 'error' : 'report', stopped ? ICONS.warning : ICONS.info, t(stopped ? 'storage.report.stopped' : 'storage.report.finished'), children, { dismiss: true });
  }

  function resultNotice() {
    const n = view.notice;
    if (!n) return null;
    if (n.kind === 'report') return reportNotice(n.report);
    if (n.kind === 'deleteReport') return deleteReportNotice(n.report);
    if (n.kind === 'planRefused') return notice('refused', ICONS.warning, t('storage.plan.refusedTitle'), [el('p', { text: n.text })], { dismiss: true });
    if (n.kind === 'cancelled') {
      const children = [el('p', { text: n.partial ? t('storage.partial', { size: bytes(n.partial), name: n.name }) : t('storage.nothingLeft') })];
      if (n.partial && storageFeatures(screen()).remove) children.push(buttonRow(deletePartialButton(n.path)));
      return notice('cancelled', ICONS.info, t('storage.cancelled'), children, { dismiss: true });
    }
    if (n.kind === 'refused') {
      const children = [el('p', { text: refusalText(t, locale(), n.refusal) })];
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
    // A file stored with the wrong size is deleted on request, never on its own.
    const children = [el('p', { text: n.text })];
    if (n.command) children.push(el('p', { text: t('udev.explain') }), udevCommand(t, n.command, notify));
    if (n.hung && screen()?.restartable) children.push(buttonRow(restartButton()));
    if (n.path && storageFeatures(screen()).remove) {
      const file = fileOf(n.path);
      children.push(buttonRow(el('button', { type: 'button', class: 'text-button', text: t('storage.delete', { name: file.name }), disabled: busy(), onclick: () => askDelete(file) })));
    }
    return notice('error', ICONS.warning, t('storage.errorTitle'), children, { dismiss: true });
  }

  function ffmpegHelp() {
    const hints = view.tools?.installHints ?? [];
    return el('div', { class: 'ffmpeg-help' }, [
      hints.length ? el('ul', { class: 'hints' }, hints.map((h) => el('li', {}, [el('code', { text: h })]))) : null,
      buttonRow(el('button', { type: 'button', class: 'text-button', text: t('storage.ffmpegLocate'), disabled: busy(), onclick: locate })),
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
        buttonRow(el('button', { type: 'button', class: 'primary-button', text: t('storage.sendVideo'), disabled: busy(), onclick: sendThemeVideo })),
      ]));
    }
    if (features.storage && view.tools && !view.tools.ready && view.notice?.refusal?.code !== 'needsConverter') {
      parts.push(notice('ffmpeg', ICONS.info, t('storage.ffmpegMissingTitle'), [el('p', { text: t('storage.ffmpegMissing') }), ffmpegHelp()]));
    }
    notices.replaceChildren(...parts.filter(Boolean));
  }

  // ----------------------------------------------------------------- job --
  const jobTitleEl = el('strong', { class: 'job-title' });
  const jobPhase = el('span', { class: 'job-phase', 'aria-live': 'polite' });
  const jobAmount = el('span', { class: 'job-amount' });
  const jobBar = el('progress', { class: 'job-bar', max: 1, 'aria-label': t('storage.progressLabel') });
  const jobCancel = el('button', { type: 'button', class: 'text-button', onclick: cancelJob });
  const jobHint = el('p', { class: 'hint' });
  jobBox.replaceChildren(
    el('div', { class: 'job-head' }, [icon(ICONS.upload, 18), jobTitleEl]),
    jobBar,
    el('div', { class: 'job-status' }, [jobPhase, jobAmount]),
    jobHint,
    buttonRow(jobCancel),
  );

  function updateJob() {
    const job = view.job;
    jobBox.hidden = !job;
    if (!job) return;
    const parts = progressParts(t, locale(), job);
    jobTitleEl.textContent = jobTitle(t, job);
    jobPhase.textContent = job.cancelling ? t('storage.cancelling') : parts.phase;
    jobAmount.textContent = parts.amount;
    if (parts.fraction === null) jobBar.removeAttribute('value');
    else jobBar.value = parts.fraction;
    jobCancel.disabled = job.cancelling;
    jobCancel.textContent = job.kind === 'upload' ? t('storage.cancel') : t('storage.job.cancel');
    jobHint.textContent = { upload: t('storage.busy'), plan: t('storage.job.planHint'), delete: t('storage.job.deleteHint') }[job.kind];
  }

  // ------------------------------------------------------------- manager --
  function dropZone(medium) {
    const zone = el('div', { class: 'drop-zone', dataset: { medium } }, [
      el('span', { class: 'drop-icon', 'aria-hidden': 'true' }, [icon(ICONS.upload, 18)]),
      el('span', { text: t('storage.dropHere') }),
      el('button', { type: 'button', class: 'text-button send-button', disabled: busy(), dataset: { focus: `send-${medium}` }, onclick: () => choose(medium) }, [
        icon(ICONS.upload, 16), el('span', { text: t('storage.choose') }),
      ]),
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

  /**
   * A medium's use: used of total in mono, the bar (`usedFraction`) and what
   * is free, all in the screen's own numbers.
   */
  function usage(capacity) {
    const fraction = usedFraction(capacity);
    return el('div', { class: 'usage-card' }, [
      el('p', { class: 'usage-figure', 'aria-hidden': 'true' }, [bytes(capacity.used), el('span', { class: 'usage-total', text: ` / ${bytes(capacity.total)}` })]),
      // A sliver stays visible for a little use.
      el('div', { class: `usage${fraction > 0.9 ? ' full' : ''}`, 'aria-hidden': 'true' }, [el('span', { style: { width: capacity.used ? `max(4px, ${(fraction * 100).toFixed(1)}%)` : '0' } })]),
      el('p', { class: 'usage-free', 'aria-hidden': 'true', text: t('storage.usageFree', { free: bytes(capacity.free) }) }),
      // The whole sentence, for screen readers.
      el('p', { class: 'usage-text visually-hidden', text: t('storage.usage', { used: bytes(capacity.used), total: bytes(capacity.total), free: bytes(capacity.free) }) }),
    ]);
  }

  const manager = createManagerView({
    t,
    locale,
    bridge,
    host: {
      key: () => view.key,
      data: () => view.data,
      features: () => ({ ...storageFeatures(screen()), remove: storageFeatures(screen()).remove && view.data?.deletes !== false }),
      busy,
      live: () => context().live,
      notify,
      notice: (n) => {
        view.notice = n;
        renderNotices();
      },
      reload: load,
      runPlan,
      runDeletes,
      askBoot,
      askDelete,
      play,
      useBackground,
      dropZone,
      usage,
      confirm,
      refocus,
    },
  });

  // ---------------------------------------------------------------- body --
  function bootSlot(features) {
    if (!features.boot) return null;
    return el('section', { class: 'boot-slot', 'aria-labelledby': 'storage-boot-title' }, [
      el('span', { class: 'boot-icon', 'aria-hidden': 'true' }, [icon(ICONS.power, 18)]),
      el('div', { class: 'boot-text' }, [
        el('h3', { id: 'storage-boot-title', text: t('storage.bootTitle') }),
        el('p', { class: 'hint', text: t('storage.bootHelp') }),
      ]),
      buttonRow(el('button', { type: 'button', class: 'text-button', text: t('storage.defaultAction'), disabled: busy(), dataset: { focus: 'boot-default' }, onclick: askBootDefault })),
    ]);
  }

  function toolbar(features) {
    const { live } = context();
    const tool = (focus, paths, label, onclick, disabled, reason = null) => el('button', {
      type: 'button', class: 'text-button', disabled, title: disabled && reason ? reason : null, dataset: { focus }, onclick,
    }, [icon(paths, 16), el('span', { text: label })]);
    const cleanupReason = features.remove ? null : t('storage.reason.deleteUnsupported');
    return el('div', { class: 'button-row storage-toolbar' }, [
      tool('tool-refresh', ICONS.refresh, t('storage.refresh'), load, busy()),
      tool('tool-stop', ICONS.stop, t('storage.stop'), stop, busy() || live, live ? t('storage.liveReason') : null),
      // The assistants that look after the whole screen, apart at the end.
      el('span', { class: 'storage-assistants' }, [
        tool('tool-cleanup', GLYPHS.sweep, t('storage.cleanup.open'), () => manager.cleanup(), busy() || !features.remove, cleanupReason),
        tool('tool-cache', GLYPHS.box, t('storage.cache.open'), () => manager.cache(), busy()),
      ]),
    ]);
  }

  function renderBody() {
    const current = screen();
    const features = { ...storageFeatures(current), remove: storageFeatures(current).remove && view.data?.deletes !== false };
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
      const command = view.error?.udevCommand;
      body.replaceChildren(
        el('p', { class: 'empty-note', role: 'alert', text: t('storage.loadError', { message: errorText(t, view.error) }) }),
        ...(command ? [el('p', { class: 'hint', text: t('udev.explain') }), udevCommand(t, command, notify)] : []),
        buttonRow(el('button', { type: 'button', class: 'text-button', text: t('storage.retry'), onclick: load })),
      );
      return;
    }
    if (!view.data || manager.dragging()) {
      if (!view.data) body.replaceChildren();
      return;
    }
    const { live } = context();
    const hints = [];
    if (live) hints.push(el('p', { class: 'hint', text: t('storage.liveHint') }));
    if (!features.remove) hints.push(el('p', { class: 'hint', text: t('storage.limitedHint') }), el('p', { class: 'hint', text: t('storage.managerLimitedHint') }));
    if (view.tools?.ready) hints.push(el('p', { class: 'hint' }, [t('storage.ffmpegReady', { version: view.tools.version ?? '' })]));
    const slot = bootSlot(features);
    body.replaceChildren(
      el('div', { class: 'manager-top' }, [toolbar(features), manager.filterBar()]),
      ...hints,
      ...(slot ? [slot] : []),
      el('div', { class: 'manager-columns' }, MEDIA.map((m) => manager.column(m))),
    );
    refocus();
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
        manager.reset();
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
    /** The screen was restarted: an error from before it no longer stands. */
    afterRestart() {
      view.notice = null;
      if (view.shown && !busy()) load();
      else renderAll();
    },
    /** The UI's language changed: every text is drawn again. */
    retranslate() {
      jobBar.setAttribute('aria-label', t('storage.progressLabel'));
      renderAll();
    },
  };
}

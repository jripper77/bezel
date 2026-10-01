// The storage manager's lists and dialogs (D-2026-09-30-storage-manager-4,
// -13): internal memory and SD card side by side as multi-select listboxes
// with thumbnails, sorted and filtered; a side toolbar per list acting on its
// selection ("Move to the other side" does what dragging does); and the
// dialogs that confirm the exact list before anything changes: move, copy,
// rename, restore, the cleanup assistant, associating an original and the
// local copies. The jobs themselves run in `storage.js`.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { errorText } from '../messages.js';
import {
  CACHE_LIMITS, KIND_FILTERS, ORIGIN_FILTERS, SORTS, actionsFor, baseName, cleanupGroups, emptySelection, findingText, formatBytes,
  formatExactBytes, formatLimit, formatSent, hasCopy, isBezel, keyAction, otherMedium, placeText, planRefusalText, precheckedPaths,
  reduceSelection, renamePreview, restorableFor, restoreTotals, selectionInfo, skipText, stepText, visibleFiles, warningText,
} from '../storage-manager.js';

/** How far a press travels before it is a drag, px. */
const DRAG_THRESHOLD = 4;
/** The order of the side toolbar. */
const ACTIONS = Object.freeze(['move', 'copy', 'rename', 'play', 'boot', 'associate', 'delete']);
/** Icons of the manager's own actions (24x24, stroked). */
export const GLYPHS = Object.freeze({
  move: ['M4 12h14', 'M13 6l6 6-6 6'],
  rename: ['M4 20h4L19 9l-4-4L4 16z', 'M13 7l4 4'],
  link: ['M10 14a4 4 0 0 0 6 0l3-3a4 4 0 0 0-6-6l-1 1', 'M14 10a4 4 0 0 0-6 0l-3 3a4 4 0 0 0 6 6l1-1'],
  sweep: ['M14 4l-5 9', 'M5 13h8l1 7H4z', 'M8 17v3', 'M11 17v3'],
  box: ['M4 7h16v13H4z', 'M3 4h18v3H3z', 'M10 11h4'],
});
const ACTION_ICONS = Object.freeze({
  move: GLYPHS.move, copy: ICONS.copy, rename: GLYPHS.rename, play: ICONS.play, boot: ICONS.power, associate: GLYPHS.link, delete: ICONS.trash,
});

let dialogs = 0;

/**
 * @param {object} deps
 * @param {(k: string, p?: object) => string} deps.t
 * @param {() => string} deps.locale
 * @param {object} deps.bridge
 * @param {object} deps.host what the storage panel lends: `key()`, `data()`,
 *   `features()`, `busy()`, `live()`, `notify(text)`, `notice(n)`,
 *   `reload()`, `runPlan(plan)`, `runDeletes(files)`, `askBoot(file)`,
 *   `askDelete(file)`, `play(file)`, `dropZone(medium)`, `confirm(opts)`,
 *   `bootSlot()`
 */
export function createManagerView({ t, locale, bridge, host }) {
  const filter = { text: '', kind: 'all', origin: 'all' };
  let sort = 'name';
  const selection = { internal: emptySelection(), sd: emptySelection() };
  // Thumbnails by screen, path and time sent: a `data:` URL, `null` (none)
  // or `loading`.
  const thumbs = new Map();
  const queue = [];
  let fetching = false;
  /** The drawn lists, by medium: their listbox, options and toolbar. */
  const columns = {};
  let press = null;

  const bytes = (n) => formatBytes(n, locale());
  const files = () => host.data()?.files ?? [];
  const shownIn = (medium) => visibleFiles(files(), medium, filter, sort, locale());
  const selectedFiles = (medium) => selectionInfo(files(), selection[medium].selected).files;
  const fail = (e) => host.notice({ kind: 'error', text: errorText(t, e), hung: e?.code === 'hung' });

  // ---------------------------------------------------------- thumbnails --
  const thumbKey = (file) => `${host.key()}|${file.path}|${file.entry?.sentAt ?? ''}`;

  async function fetchThumbnails() {
    if (fetching) return;
    fetching = true;
    while (queue.length) {
      const [key, screen, path] = queue.shift();
      let url = null;
      try {
        url = await bridge.managerThumbnail(screen, path);
      } catch {
        url = null;
      }
      thumbs.set(key, url ?? null);
      for (const img of document.querySelectorAll('img[data-thumb]')) {
        if (img.dataset.thumb === key && url) {
          img.src = url;
          img.closest('.file-thumb')?.classList.add('has-picture');
        }
      }
    }
    fetching = false;
  }

  function thumbnail(file) {
    const fallback = icon(file.kind === 'video' ? ICONS.film : ICONS.image, 20);
    if (!isBezel(file)) return el('span', { class: 'file-thumb', 'aria-hidden': 'true' }, [fallback]);
    const key = thumbKey(file);
    if (!thumbs.has(key)) {
      thumbs.set(key, 'loading');
      queue.push([key, host.key(), file.path]);
      fetchThumbnails();
    }
    const url = thumbs.get(key);
    const picture = typeof url === 'string' && url !== 'loading';
    return el('span', { class: `file-thumb${picture ? ' has-picture' : ''}`, 'aria-hidden': 'true' }, [
      fallback,
      el('img', { alt: '', dataset: { thumb: key }, src: picture ? url : null, width: 48, height: 48 }),
    ]);
  }

  // ------------------------------------------------------------- options --
  function badges(file) {
    const tags = [];
    if (file.protected) tags.push([t(`storage.protected.${file.protected}`), 'protected']);
    if (file.entry?.state === 'pending') tags.push([t('storage.state.pending'), 'warn']);
    if (file.finding) tags.push([t(`storage.findingGroup.${file.finding.code}`), file.finding.prechecked ? 'warn' : 'finding']);
    if (!hasCopy(file)) tags.push([t('storage.noCopy'), 'muted']);
    return el('span', { class: 'file-tags' }, tags.map(([text, kind]) => el('span', { class: `tag ${kind}`, text })));
  }

  function meta(file) {
    const parts = [file.size === null ? t('storage.sizeUnknown') : bytes(file.size), t(`storage.kindOne.${file.kind}`)];
    parts.push(isBezel(file) ? t('storage.origin.sent', { date: formatSent(file.entry.sentAt, locale()) }) : t('storage.origin.other'));
    return parts.join(' · ');
  }

  function option(medium, file, index) {
    return el('li', {
      role: 'option',
      id: `mgr-${medium}-${index}`,
      class: 'file-option',
      'aria-selected': 'false',
      dataset: { path: file.path },
    }, [
      thumbnail(file),
      el('span', { class: 'file-main' }, [
        el('span', { class: 'file-name', text: file.name }),
        el('span', { class: 'file-meta', text: meta(file) }),
        badges(file),
      ]),
    ]);
  }

  // ----------------------------------------------------------- selection --
  /** Shows `medium`'s selection without drawing the list again (focus stays). */
  function showSelection(medium) {
    const column = columns[medium];
    if (!column?.listbox) return;
    const state = selection[medium];
    for (const opt of column.listbox.querySelectorAll('[role="option"]')) {
      const on = state.selected.includes(opt.dataset.path);
      opt.setAttribute('aria-selected', String(on));
      opt.classList.toggle('focused', opt.dataset.path === state.focus);
    }
    const focused = column.listbox.querySelector(`[data-path="${CSS.escape(state.focus ?? '')}"]`);
    if (focused) {
      column.listbox.setAttribute('aria-activedescendant', focused.id);
      focused.scrollIntoView?.({ block: 'nearest' });
    } else {
      column.listbox.removeAttribute('aria-activedescendant');
    }
    updateToolbar(medium);
  }

  function select(medium, action) {
    const ids = shownIn(medium).map((f) => f.path);
    selection[medium] = reduceSelection(selection[medium], action, ids);
    showSelection(medium);
  }

  // ------------------------------------------------------------- toolbar --
  function actionLabel(name, medium) {
    if (name === 'move' || name === 'copy') return t(`storage.action.${name}To.${otherMedium(medium)}`);
    return t(`storage.action.${name}`);
  }

  function allowed(medium) {
    const data = host.data();
    return actionsFor({
      features: host.features(),
      files: selectedFiles(medium),
      medium,
      card: Boolean(data?.card),
      live: host.live(),
      busy: host.busy(),
    });
  }

  function updateToolbar(medium) {
    const column = columns[medium];
    if (!column?.count) return;
    const can = allowed(medium);
    for (const [name, button] of Object.entries(column.buttons)) {
      const { enabled, reason } = can[name];
      button.disabled = !enabled;
      button.title = enabled ? '' : t(`storage.reason.${reason}`);
    }
    const info = selectionInfo(files(), selection[medium].selected);
    const shown = shownIn(medium).length;
    const total = files().filter((f) => f.medium === medium).length;
    column.count.textContent = [
      t('storage.list.shown', { shown, total }),
      info.count ? t('storage.list.selected', { count: info.count, size: bytes(info.bytes) }) : null,
    ].filter(Boolean).join(' · ');
  }

  function run(name, medium) {
    const chosen = selectedFiles(medium);
    switch (name) {
      case 'move':
      case 'copy':
        return transfer(name, medium, chosen.map((f) => f.path));
      case 'rename': return rename(chosen[0]);
      case 'play': return host.play(chosen[0]);
      case 'boot': return host.askBoot(chosen[0]);
      case 'associate': return associate(chosen[0]);
      case 'delete': return chosen.length === 1 ? host.askDelete(chosen[0]) : deleteMany(chosen);
      default: return null;
    }
  }

  function toolbar(medium) {
    const features = host.features();
    const buttons = {};
    for (const name of ACTIONS) {
      if (name === 'boot' && !features.boot) continue;
      buttons[name] = el('button', {
        type: 'button',
        class: 'text-button',
        dataset: { focus: `act-${name}-${medium}` },
        onclick: () => run(name, medium),
      }, [icon(ACTION_ICONS[name], 16), el('span', { text: actionLabel(name, medium) })]);
    }
    columns[medium].buttons = buttons;
    return el('div', { class: 'button-row manager-actions', role: 'group', 'aria-label': t('storage.list.actions', { medium: t(`storage.medium.${medium}`) }) }, Object.values(buttons));
  }

  // ------------------------------------------------------------ the list --
  function onKey(medium, evt) {
    const action = keyAction(evt);
    if (!action) return;
    evt.preventDefault();
    evt.stopPropagation();
    if (action.command) {
      const can = allowed(medium)[action.command];
      if (can.enabled) run(action.command, medium);
      return;
    }
    select(medium, action);
  }

  /** Where a drag over (x, y) would drop: the other list, or the boot slot. */
  function dropAt(medium, x, y) {
    const hit = document.elementFromPoint(x, y);
    const column = hit?.closest('.manager-column');
    if (column && column.dataset.medium !== medium) return { kind: 'column', node: column, medium: column.dataset.medium };
    const slot = hit?.closest('.boot-slot');
    if (slot && selectedFiles(medium).length === 1) return { kind: 'boot', node: slot };
    return null;
  }

  function endDrag() {
    press?.ghost?.remove();
    for (const node of document.querySelectorAll('.manager-column.drop-target, .boot-slot.drop-target')) node.classList.remove('drop-target');
    press = null;
  }

  function wirePointer(medium, listbox) {
    listbox.addEventListener('pointerdown', (evt) => {
      const opt = evt.target.closest('[role="option"]');
      if (!opt || evt.button !== 0) return;
      press = { medium, path: opt.dataset.path, x: evt.clientX, y: evt.clientY, ctrl: evt.ctrlKey || evt.metaKey, shift: evt.shiftKey, ghost: null };
      listbox.setPointerCapture?.(evt.pointerId);
    });
    listbox.addEventListener('pointermove', (evt) => {
      if (!press || press.medium !== medium) return;
      if (!press.ghost) {
        const far = Math.hypot(evt.clientX - press.x, evt.clientY - press.y) >= DRAG_THRESHOLD;
        if (!far || host.busy() || press.ctrl || press.shift) return;
        if (!selection[medium].selected.includes(press.path)) select(medium, { type: 'click', id: press.path });
        press.ghost = el('div', { class: 'drop-ghost', text: t('storage.drag.label', { count: selection[medium].selected.length }) });
        document.body.append(press.ghost);
      }
      press.ghost.style.left = `${evt.clientX}px`;
      press.ghost.style.top = `${evt.clientY}px`;
      const target = dropAt(medium, evt.clientX, evt.clientY);
      for (const node of document.querySelectorAll('.manager-column, .boot-slot')) node.classList.toggle('drop-target', node === target?.node);
    });
    listbox.addEventListener('pointerup', (evt) => {
      if (!press || press.medium !== medium) return;
      const { ghost, path, ctrl, shift } = press;
      const target = ghost ? dropAt(medium, evt.clientX, evt.clientY) : null;
      endDrag();
      if (!ghost) {
        select(medium, { type: 'click', id: path, ctrl, shift });
        return;
      }
      if (target?.kind === 'column') transfer('move', medium, selection[medium].selected);
      else if (target?.kind === 'boot') host.askBoot(selectedFiles(medium)[0]);
    });
    listbox.addEventListener('pointercancel', endDrag);
  }

  function listOf(medium) {
    const shown = shownIn(medium);
    const column = columns[medium];
    if (!shown.length) {
      column.listbox = null;
      const total = files().some((f) => f.medium === medium);
      return el('p', { class: 'empty-note', text: total ? t('storage.list.noMatch') : t('storage.list.empty') });
    }
    const listbox = el('ul', {
      role: 'listbox',
      class: 'file-list',
      tabindex: '0',
      'aria-multiselectable': 'true',
      'aria-label': t('storage.list.label', { medium: t(`storage.medium.${medium}`) }),
      dataset: { focus: `list-${medium}` },
    }, shown.map((file, i) => option(medium, file, i)));
    listbox.addEventListener('keydown', (evt) => onKey(medium, evt));
    listbox.addEventListener('focus', () => {
      if (!selection[medium].focus) select(medium, { type: 'move', to: 'first' });
    });
    wirePointer(medium, listbox);
    column.listbox = listbox;
    return listbox;
  }

  /** Draws `medium`'s list again (sort, filter or data changed); the list keeps the focus. */
  function refreshList(medium) {
    const column = columns[medium];
    if (!column?.slot) return;
    const ids = shownIn(medium).map((f) => f.path);
    selection[medium] = reduceSelection(selection[medium], { type: 'prune' }, ids);
    const hadFocus = column.listbox && column.listbox === document.activeElement;
    column.slot.replaceChildren(listOf(medium));
    if (hadFocus) column.listbox?.focus();
    showSelection(medium);
    updateToolbar(medium);
  }

  // ------------------------------------------------------------- columns --
  function usage(capacity) {
    const fraction = capacity.total ? Math.min(1, capacity.used / capacity.total) : 0;
    return [
      // A sliver stays visible for a little use.
      el('div', { class: `usage${fraction > 0.9 ? ' full' : ''}`, 'aria-hidden': 'true' }, [el('span', { style: { width: capacity.used ? `max(4px, ${(fraction * 100).toFixed(1)}%)` : '0' } })]),
      el('p', { class: 'usage-text', text: t('storage.usage', { used: bytes(capacity.used), total: bytes(capacity.total), free: bytes(capacity.free) }) }),
    ];
  }

  /**
   * One side of the manager: usage, the drop zone, restore, the toolbar and
   * the list, as a region named by its medium.
   */
  function column(medium) {
    const data = host.data();
    const capacity = medium === 'internal' ? data.internal : data.card;
    const id = `storage-${medium}-title`;
    columns[medium] = { buttons: {}, listbox: null, slot: null, count: null };
    const head = el('div', { class: 'column-head' }, [el('h3', { id }, [icon(medium === 'sd' ? ICONS.card : ICONS.chip, 16), t(`storage.medium.${medium}`)])]);
    const children = [head];
    if (!capacity) {
      children.push(el('p', { class: 'empty-note', text: t('storage.noCard') }));
    } else {
      const restorable = restorableFor(data.restorable, medium);
      if (restorable.length) {
        head.append(el('button', {
          type: 'button', class: 'text-button', disabled: host.busy(), dataset: { focus: `restore-${medium}` }, onclick: () => restore(medium),
        }, [icon(ICONS.refresh, 16), el('span', { text: t('storage.restore.open', { count: restorable.length }) })]));
      }
      const count = el('p', { class: 'list-count' });
      const slot = el('div', { class: 'list-slot' });
      Object.assign(columns[medium], { count, slot });
      const errors = (data.folderErrors ?? []).filter((f) => f.medium === medium)
        .map((f) => el('p', { class: 'empty-note', text: t('storage.folderError', { reason: errorText(t, f.error) }) }));
      children.push(...usage(capacity), host.dropZone(medium), toolbar(medium), count, ...errors, slot);
      slot.append(listOf(medium));
    }
    if (medium === 'sd') children.push(el('p', { class: 'hint', text: t('storage.cardHelp') }));
    const section = el('section', { class: 'medium manager-column', 'aria-labelledby': id, dataset: { medium } }, children);
    queueMicrotask(() => {
      showSelection(medium);
      updateToolbar(medium);
    });
    return section;
  }

  // ------------------------------------------------------------- filters --
  function filterBar() {
    const select = (name, values, current, onChange) => el('label', { class: 'field' }, [
      el('span', { text: t(`storage.filter.${name}`) }),
      el('select', { dataset: { focus: `filter-${name}` }, onchange: (evt) => onChange(evt.target.value) },
        values.map((v) => el('option', { value: v, text: t(`storage.filter.${name}.${v}`), selected: v === current }))),
    ]);
    const refresh = () => {
      refreshList('internal');
      refreshList('sd');
    };
    return el('div', { class: 'manager-filters', role: 'group', 'aria-label': t('storage.filter.label') }, [
      el('label', { class: 'field search' }, [
        el('span', { text: t('storage.filter.text') }),
        el('input', {
          type: 'search', value: filter.text, spellcheck: 'false', dataset: { focus: 'filter-text' },
          oninput: (evt) => {
            filter.text = evt.target.value;
            refresh();
          },
        }),
      ]),
      select('kind', KIND_FILTERS, filter.kind, (v) => { filter.kind = v; refresh(); }),
      select('origin', ORIGIN_FILTERS, filter.origin, (v) => { filter.origin = v; refresh(); }),
      select('sort', SORTS, sort, (v) => { sort = v; refresh(); }),
    ]);
  }

  // ------------------------------------------------------------- dialogs --
  /**
   * A modal dialog whose body can change while it is open; resolves with the
   * id of the button that closed it (`cancel` for Esc and the close button).
   */
  function modal(title, { wide = false } = {}) {
    const opener = document.activeElement;
    const id = `mgr-dialog-${(dialogs += 1)}`;
    // A long list scrolls: the body takes the focus for the keys that scroll it.
    const body = el('div', { id: `${id}-body`, class: 'dialog-body', tabindex: wide ? '0' : null });
    const actions = el('div', { class: 'dialog-actions' });
    const close = el('button', { type: 'button', class: 'icon-button dialog-close', title: t('dialog.close'), 'aria-label': t('dialog.close') }, [icon(ICONS.close, 16)]);
    const dialog = el('dialog', { class: `confirm-dialog${wide ? ' wide-dialog' : ''}`, 'aria-labelledby': `${id}-title` }, [
      el('div', { class: 'dialog-head' }, [el('h2', { id: `${id}-title`, text: title }), close]),
      body,
      actions,
    ]);
    let resolve;
    const result = new Promise((r) => { resolve = r; });
    close.addEventListener('click', () => dialog.close('cancel'));
    dialog.addEventListener('close', () => {
      dialog.remove();
      if (opener?.isConnected) opener.focus();
      else host.refocus();
      resolve(dialog.returnValue || 'cancel');
    }, { once: true });
    const button = (choice, label, kind = 'text') => {
      const b = el('button', { type: 'button', class: { primary: 'primary-button', danger: 'danger-button' }[kind] ?? 'text-button', text: label });
      b.addEventListener('click', () => dialog.close(choice));
      actions.append(b);
      return b;
    };
    document.body.append(dialog);
    dialog.showModal();
    return { dialog, body, result, button };
  }

  function warningRow(text) {
    return el('p', { class: 'dialog-warning' }, [icon(ICONS.warning, 18), el('span', { text })]);
  }

  /** What a plan does, as the confirmation lists it; conflicts can be overwritten. */
  function planBody(plan, overwrite, toggle) {
    const parts = [el('p', { text: t(`storage.plan.intro.${plan.transfer}`) })];
    if (plan.steps.length) {
      parts.push(
        el('ul', { class: 'plan-list', 'aria-label': t('storage.plan.stepsLabel') }, plan.steps.map((s) => el('li', {}, [
          el('span', { text: stepText(t, locale(), s) }),
          s.replaces ? el('small', { text: t('storage.plan.replaces', { name: s.replaces.name, size: bytes(s.replaces.size) }) }) : null,
        ]))),
        el('p', { class: 'plan-total', text: t('storage.plan.total', { count: plan.steps.length, size: bytes(plan.bytes), free: bytes(plan.free), place: t(`storage.in.${plan.to}`) }) }),
      );
    } else {
      parts.push(el('p', { class: 'empty-note', text: t('storage.plan.nothing') }));
    }
    if (plan.skipped.length) {
      parts.push(el('h3', { class: 'dialog-subtitle', text: t('storage.plan.skippedTitle') }));
      parts.push(el('ul', { class: 'plan-list skipped' }, plan.skipped.map((s) => {
        const row = [el('span', { text: skipText(t, locale(), s) })];
        if (s.code === 'conflict' && s.conflict && s.conflict.path !== s.source) {
          const box = el('input', { type: 'checkbox', checked: overwrite.has(s.target), onchange: (evt) => toggle(s.target, evt.target.checked) });
          row.push(el('label', { class: 'check' }, [box, el('span', { text: t('storage.plan.overwrite', { name: s.conflict.name }) })]));
        }
        return el('li', {}, row);
      })));
    }
    for (const w of plan.warnings) parts.push(warningRow(warningText(t, w)));
    return parts;
  }

  /**
   * The one confirmation of a plan: every file as source → target with its
   * size, what is left out and why, the warnings. Overwriting a conflict
   * plans again (`replan(overwrite)`); the plan confirmed is the one run.
   */
  async function confirmPlan(first, replan) {
    const name = baseName(first.steps[0]?.source ?? first.skipped[0]?.source ?? '');
    const m = modal(t(`storage.plan.title.${first.transfer}`, { to: t(`storage.to.${first.to}`), name }), { wide: true });
    m.button('cancel', t('dialog.cancel'));
    const ok = m.button('ok', t(`storage.plan.action.${first.transfer}`), 'primary');
    let plan = first;
    const overwrite = new Set();
    const draw = () => {
      m.body.replaceChildren(...planBody(plan, overwrite, toggle));
      ok.disabled = !plan.steps.length;
    };
    async function toggle(target, on) {
      if (on) overwrite.add(target);
      else overwrite.delete(target);
      ok.disabled = true;
      try {
        const next = await replan([...overwrite]);
        if (next.status === 'ready') plan = next;
        else m.body.append(warningRow(planRefusalText(t, locale(), next)));
      } catch (e) {
        m.body.append(warningRow(errorText(t, e)));
      }
      draw();
    }
    draw();
    (ok.disabled ? m.dialog.querySelector('.dialog-actions button') : ok).focus();
    if ((await m.result) === 'ok') host.runPlan(plan);
  }

  /** Plans with `ask`, then confirms; a refusal is explained in a notice. */
  async function planThen(ask, replan) {
    let plan = null;
    try {
      plan = await ask();
    } catch (e) {
      fail(e);
      return;
    }
    if (plan.status === 'refused') host.notice({ kind: 'planRefused', text: planRefusalText(t, locale(), plan) });
    else await confirmPlan(plan, replan);
  }

  function transfer(kind, medium, paths) {
    if (!paths.length || host.busy()) return null;
    const to = otherMedium(medium);
    const call = kind === 'move' ? 'planMove' : 'planCopy';
    return planThen(() => bridge[call](host.key(), paths, to, []), (overwrite) => bridge[call](host.key(), paths, to, overwrite));
  }

  async function rename(file) {
    const m = modal(t('storage.rename.title', { name: file.name }));
    const inputId = `${m.dialog.getAttribute('aria-labelledby')}-name`;
    const preview = el('p', { id: `${inputId}-preview`, class: 'hint', 'aria-live': 'polite' });
    const input = el('input', { id: inputId, type: 'text', spellcheck: 'false', value: file.name.toLowerCase(), 'aria-describedby': preview.id });
    m.body.append(
      el('p', { text: t('storage.rename.intro') }),
      el('div', { class: 'field' }, [el('label', { for: inputId, text: t('storage.rename.label') }), input]),
      preview,
    );
    m.button('cancel', t('dialog.cancel'));
    const ok = m.button('ok', t('storage.rename.next'), 'primary');
    const check = () => {
      const { name, problem } = renamePreview(input.value, file.name);
      preview.textContent = problem ? planRefusalText(t, locale(), problem) : t('storage.rename.preview', { name });
      preview.classList.toggle('field-error', Boolean(problem));
      input.setAttribute('aria-invalid', String(Boolean(problem)));
      ok.disabled = Boolean(problem);
    };
    input.addEventListener('input', check);
    input.addEventListener('keydown', (evt) => {
      if (evt.key === 'Enter' && !ok.disabled) {
        evt.preventDefault();
        ok.click();
      }
    });
    check();
    input.focus();
    input.select();
    if ((await m.result) !== 'ok') return;
    const name = input.value.trim();
    await planThen(() => bridge.planRename(host.key(), file.path, name, []), (overwrite) => bridge.planRename(host.key(), file.path, name, overwrite));
  }

  async function restore(medium) {
    const data = host.data();
    const entries = restorableFor(data.restorable, medium);
    const free = (medium === 'internal' ? data.internal : data.card)?.free ?? 0;
    const chosen = new Set(entries.filter((e) => e.localCopy).map((e) => e.id));
    const m = modal(t('storage.restore.title', { to: t(`storage.to.${medium}`) }), { wide: true });
    const total = el('p', { class: 'plan-total', 'aria-live': 'polite' });
    m.button('cancel', t('dialog.cancel'));
    const ok = m.button('ok', t('storage.restore.next'), 'primary');
    const update = () => {
      const sum = restoreTotals(entries, [...chosen], free);
      total.textContent = sum.fits ? t('storage.restore.total', { count: sum.count, size: bytes(sum.bytes), free: bytes(free) }) : t('storage.restore.noRoom', { size: bytes(sum.bytes), free: bytes(free) });
      total.classList.toggle('field-error', !sum.fits);
      ok.disabled = !sum.count || !sum.fits;
    };
    const row = (e) => {
      const state = e.otherCard ? t('storage.restore.otherCard') : t(`storage.state.${e.state}`);
      const details = [bytes(e.size), state, t('storage.origin.sent', { date: formatSent(e.sentAt, locale()) })];
      if (!e.localCopy) details.push(t('storage.noCopy'));
      const box = el('input', {
        type: 'checkbox', checked: chosen.has(e.id), disabled: !e.localCopy,
        onchange: (evt) => {
          if (evt.target.checked) chosen.add(e.id);
          else chosen.delete(e.id);
          update();
        },
      });
      return el('li', {}, [el('label', { class: 'check' }, [box, el('span', {}, [el('strong', { text: e.name }), el('small', { text: details.join(' · ') })])])]);
    };
    m.body.append(
      el('p', { text: t('storage.restore.intro') }),
      el('ul', { class: 'check-list', 'aria-label': t('storage.restore.listLabel') }, entries.map(row)),
      total,
    );
    update();
    (ok.disabled ? m.dialog.querySelector('.dialog-actions button') : ok).focus();
    if ((await m.result) !== 'ok') return;
    const ids = [...chosen];
    await planThen(() => bridge.planRestore(host.key(), ids, medium, []), (overwrite) => bridge.planRestore(host.key(), ids, medium, overwrite));
  }

  async function deleteMany(chosen) {
    const total = chosen.reduce((sum, f) => sum + (f.size ?? 0), 0);
    const ok = await host.confirm({
      title: t('storage.deleteMany.title', { count: chosen.length }),
      body: [
        el('ul', { class: 'plan-list' }, chosen.map((f) => el('li', { text: t('storage.deleteMany.item', { place: placeText(t, f.path), size: bytes(f.size) }) }))),
        el('p', { text: t('storage.deleteMany.body', { size: bytes(total) }) }),
      ],
      action: t('storage.deleteAction'),
      danger: true,
    });
    if (ok) host.runDeletes(chosen);
  }

  // ------------------------------------------------------------- cleanup --
  /**
   * The cleanup assistant (D-2026-09-30-storage-manager-9): the findings by
   * reason, only the exact signals checked; then one confirmation that lists
   * the exact files and the space freed.
   */
  async function cleanup() {
    const found = files().filter((f) => f.finding);
    const checked = new Set(precheckedPaths(found));
    const m = modal(t('storage.cleanup.title'), { wide: true });
    const total = el('p', { class: 'plan-total', 'aria-live': 'polite' });
    m.button('cancel', t('dialog.cancel'));
    const ok = found.length ? m.button('ok', t('storage.cleanup.next'), 'danger') : null;
    const update = () => {
      const picked = found.filter((f) => checked.has(f.path));
      total.textContent = t('storage.cleanup.total', { count: picked.length, size: bytes(picked.reduce((sum, f) => sum + (f.size ?? 0), 0)) });
      if (ok) ok.disabled = !picked.length;
    };
    const row = (file) => {
      const box = el('input', {
        type: 'checkbox', checked: checked.has(file.path), dataset: { path: file.path },
        onchange: (evt) => {
          if (evt.target.checked) checked.add(file.path);
          else checked.delete(file.path);
          update();
        },
      });
      const details = [t(`storage.medium.${file.medium}`), file.size === null ? t('storage.sizeUnknown') : bytes(file.size), findingText(t, locale(), file)];
      return el('li', {}, [el('label', { class: 'check' }, [box, el('span', {}, [el('strong', { text: file.name }), el('small', { text: details.join(' · ') })])])]);
    };
    m.body.append(el('p', { text: t('storage.cleanup.intro') }));
    if (!found.length) m.body.append(el('p', { class: 'empty-note', text: t('storage.cleanup.none') }));
    for (const group of cleanupGroups(found)) {
      const id = `${m.dialog.getAttribute('aria-labelledby')}-${group.code}`;
      m.body.append(el('section', { class: 'cleanup-group', 'aria-labelledby': id }, [
        el('h3', { id, class: 'dialog-subtitle', text: t(`storage.findingGroup.${group.code}`) }),
        el('p', { class: 'hint', text: t(`storage.findingHelp.${group.code}`) }),
        el('ul', { class: 'check-list' }, group.files.map(row)),
      ]));
    }
    m.body.append(total);
    update();
    m.dialog.querySelector('.dialog-actions button').focus();
    if ((await m.result) !== 'ok') return;
    const picked = found.filter((f) => checked.has(f.path));
    const freed = picked.reduce((sum, f) => sum + (f.size ?? 0), 0);
    const yes = await host.confirm({
      title: t('storage.cleanup.confirmTitle', { count: picked.length }),
      body: [
        el('ul', { class: 'plan-list', 'aria-label': t('storage.cleanup.confirmList') }, picked.map((f) => el('li', { text: t('storage.deleteMany.item', { place: placeText(t, f.path), size: bytes(f.size) }) }))),
        el('p', { text: t('storage.cleanup.confirmFreed', { size: bytes(freed) }) }),
      ],
      action: t('storage.cleanup.deleteAction'),
      danger: true,
    });
    if (yes) host.runDeletes(picked);
  }

  // ----------------------------------------------------------- associate --
  /**
   * Associates a screen file Bezel has no copy of with its original on the PC
   * (D-2026-09-30-storage-manager-10): files of exactly its size and kind,
   * likeliest first; the user confirms the pair, its bytes go to the store.
   */
  async function associate(file) {
    const m = modal(t('storage.associate.title', { name: file.name }), { wide: true });
    const found = el('div', { class: 'candidates', 'aria-live': 'polite' });
    let source = null;
    m.button('cancel', t('dialog.cancel'));
    const ok = m.button('ok', t('storage.associate.action'), 'primary');
    ok.disabled = true;
    const show = (candidates) => {
      source = candidates[0]?.source ?? null;
      ok.disabled = !source;
      if (!candidates.length) {
        found.replaceChildren(el('p', { class: 'empty-note', text: t('storage.associate.none', { size: formatExactBytes(file.size, locale()) }) }));
        return;
      }
      const group = `${m.dialog.getAttribute('aria-labelledby')}-pick`;
      found.replaceChildren(el('fieldset', { class: 'check-list' }, [
        el('legend', { text: t('storage.associate.candidates') }),
        ...candidates.map((c, i) => {
          const details = [bytes(c.size)];
          if (c.durationMs !== null) details.push(t('storage.associate.duration', { seconds: Math.round(c.durationMs / 1000) }));
          if (c.resolution) details.push(`${c.resolution.width}×${c.resolution.height}`);
          if (c.sameName) details.push(t('storage.associate.sameName'));
          return el('label', { class: 'check' }, [
            el('input', { type: 'radio', name: group, checked: i === 0, onchange: () => { source = c.source; } }),
            el('span', {}, [el('strong', { text: c.name }), el('small', { text: details.join(' · ') })]),
          ]);
        }),
      ]));
    };
    const pick = async (folder) => {
      try {
        const sources = await bridge.pickOriginals(folder);
        if (!sources?.length) return;
        show((await bridge.associateCandidates(host.key(), file.path, sources)).candidates);
      } catch (e) {
        found.replaceChildren(warningRow(errorText(t, e)));
      }
    };
    m.body.append(
      el('p', { text: t('storage.associate.intro', { size: formatExactBytes(file.size, locale()) }) }),
      el('div', { class: 'button-row' }, [
        el('button', { type: 'button', class: 'text-button', text: t('storage.associate.pickFiles'), onclick: () => pick(false) }),
        el('button', { type: 'button', class: 'text-button', text: t('storage.associate.pickFolder'), onclick: () => pick(true) }),
      ]),
      found,
    );
    m.body.querySelector('button').focus();
    if ((await m.result) !== 'ok' || !source) return;
    try {
      await bridge.associateOriginal(host.key(), file.path, source, true);
      host.notify(t('storage.associate.done', { name: file.name }));
    } catch (e) {
      fail(e);
    }
    await host.reload();
  }

  // --------------------------------------------------------------- cache --
  /** The local copies, their limit and "Clear cache" (D-2026-09-30-storage-manager-6). */
  async function cache() {
    let info;
    try {
      info = await bridge.cacheInfo();
    } catch (e) {
      fail(e);
      return;
    }
    const m = modal(t('storage.cache.title'), { wide: true });
    const facts = el('dl', { class: 'summary' });
    const all = el('input', { type: 'checkbox', id: `${m.dialog.getAttribute('aria-labelledby')}-all` });
    m.button('cancel', t('dialog.close'));
    const ok = m.button('ok', t('storage.cache.clear'), 'danger');
    const draw = () => {
      const row = (label, value) => [el('dt', { text: label }), el('dd', { text: value })];
      facts.replaceChildren(
        ...row(t('storage.cache.copies'), t('storage.cache.amount', { count: info.copies, size: bytes(info.bytes) })),
        ...row(t('storage.cache.deleted'), t('storage.cache.amount', { count: info.deletedCopies, size: bytes(info.deletedBytes) })),
      );
      ok.disabled = !(all.checked ? info.copies : info.deletedCopies);
    };
    const limit = el('select', {
      id: `${all.id}-limit`,
      onchange: async (evt) => {
        try {
          info = await bridge.setCacheLimit(Number(evt.target.value));
          host.notify(t('storage.cache.limitSet', { limit: formatLimit(info.limit, locale()) }));
        } catch (e) {
          host.notify(errorText(t, e));
        }
        draw();
      },
    }, [...new Set([...CACHE_LIMITS, info.limit])].sort((a, b) => a - b).map((v) => el('option', { value: String(v), text: formatLimit(v, locale()), selected: v === info.limit })));
    all.addEventListener('change', draw);
    m.body.append(
      el('p', { text: t('storage.cache.intro') }),
      facts,
      el('div', { class: 'field' }, [el('label', { for: limit.id, text: t('storage.cache.limit') }), limit]),
      el('p', { class: 'hint', text: t('storage.cache.limitHelp') }),
      el('label', { class: 'check' }, [all, el('span', { text: t('storage.cache.all') })]),
    );
    draw();
    m.dialog.querySelector('.dialog-actions button').focus();
    if ((await m.result) !== 'ok') return;
    const scope = all.checked ? 'all' : 'deleted';
    const count = scope === 'all' ? info.copies : info.deletedCopies;
    const size = scope === 'all' ? info.bytes : info.deletedBytes;
    const yes = await host.confirm({
      title: t('storage.cache.confirmTitle'),
      body: [el('p', { text: t('storage.cache.confirm', { count, size: bytes(size) }) }), el('p', { text: t('storage.cache.confirmKeeps') })],
      action: t('storage.cache.clearAction'),
      danger: true,
    });
    if (!yes) return;
    try {
      const done = await bridge.clearCache(scope, true);
      host.notify(t('storage.cache.cleared', { count: done.removed, size: bytes(done.bytes) }));
    } catch (e) {
      fail(e);
    }
    await host.reload();
  }

  return {
    column,
    filterBar,
    cleanup,
    cache,
    /** The screen changed: its selection and thumbnails go. */
    reset() {
      selection.internal = emptySelection();
      selection.sd = emptySelection();
      endDrag();
    },
    /** A drag in progress (the list is not drawn again under it). */
    dragging: () => Boolean(press?.ghost),
  };
}

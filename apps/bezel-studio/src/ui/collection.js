// The Media tab's Collection (D-2026-10-01-gif-sticker-search-5): the GIFs
// and stickers kept on this computer, with a filter (All | GIFs | Stickers),
// how many items there are and how large. Each item shows its preview (a
// still when motion is reduced), name, kind, size in pixels and bytes and
// source, and offers Add as image, Use as background, Rename (in place:
// Enter saves, Esc cancels, a name is never empty) and Delete… (after a
// confirmation naming the themes that use it). Its preview, or Add as image,
// drags onto the canvas: an image element where it is dropped. What happens
// is said in a status line, failures in an alert. The logic lives in
// `../collection.js`.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { makeDraggable } from './dragdrop.js';
import { askChoice } from './dialog.js';
import { errorMessage, errorText } from '../messages.js';
import {
  COLLECTION_FILTERS, cleanName, collectionCount, createCollectionList, deleteText, filterCollection, focusAfterRemoval, itemFacts, previewOf, renameKey,
} from '../collection.js';

/** An item's actions, in the order its buttons show. */
const ACTIONS = Object.freeze(['image', 'background', 'rename', 'delete']);

/**
 * @param {object} deps
 * @param {HTMLElement} deps.root where the collection is drawn (`#collection-panel`)
 * @param {(k: string, p?: object) => string} deps.t
 * @param {() => string} deps.locale
 * @param {object} deps.bridge
 * @param {{containsClient: Function, point: Function}} deps.canvas the canvas view, where previews drop
 * @param {HTMLElement} deps.stage
 * @param {HTMLElement} deps.searchButton "Search GIFs and stickers…", which an empty collection points to
 * @param {(item: object, target: 'image'|'background', at: {x: number, y: number}|null) => Promise<unknown>} deps.use
 *   copies an item into the theme: an image (where it was dropped, else in the middle) or the background
 */
export function createCollectionPanel({ root, t, locale, bridge, canvas, stage, searchButton, use }) {
  const motion = globalThis.matchMedia?.('(prefers-reduced-motion: reduce)') ?? null;
  const list = createCollectionList({ load: (still) => bridge.gifCollection(still), still: () => Boolean(motion?.matches) });
  const parts = {};
  // Each item shown, by id: its row, its action buttons and, while it is renamed, its field.
  const rows = new Map();
  // The items with an action on its way.
  const busy = new Set();
  let filter = 'all';
  // The item whose name is edited: `{id, value, error, saving}`.
  let editing = null;
  // Why the last action failed (a failed load is the list's own).
  let actionError = null;
  // Rows made so far, for unique element ids.
  let made = 0;
  // While the list is drawn again, a field taken away is not a field left.
  let drawing = false;

  /**
   * Says the text of the translation key `key` (with `params`) in the
   * status line, again when it is the same. It takes a key, never a text.
   */
  function announce(key, params) {
    const { status } = parts;
    const text = t(key, params);
    status.textContent = status.textContent === text ? `${text} ` : text;
  }

  /** Empties the status line. */
  function silence() {
    parts.status.textContent = '';
  }

  // ---------------------------------------------------------- problem --
  function paintProblem() {
    const loadError = list.status() === 'error' ? list.error() : null;
    if (!loadError && !actionError) {
      parts.problem.replaceChildren();
      return;
    }
    const lines = loadError ? [t('collection.loadFailed'), errorText(t, loadError)] : [errorText(t, actionError)];
    const retry = loadError && el('button', { type: 'button', class: 'text-button', text: t('collection.retry'), onclick: () => void refresh() });
    parts.problem.replaceChildren(el('div', { class: 'dialog-warning' }, [
      icon(ICONS.warning, 18),
      el('div', {}, [...lines.map((text) => el('p', { text })), retry]),
    ]));
  }

  function failed(error) {
    actionError = error;
    paintProblem();
  }

  function clearProblem() {
    actionError = null;
    paintProblem();
  }

  // ------------------------------------------------------------ focus --
  /** The item action (or rename field) that has the focus, to give it back once the list is drawn again. */
  function focusedAction() {
    const active = document.activeElement;
    if (!active || !parts.list.contains(active)) return null;
    for (const [id, row] of rows) {
      if (row.input === active) return { id, action: 'name' };
      const action = ACTIONS.find((a) => row.buttons[a] === active);
      if (action) return { id, action };
    }
    return null;
  }

  function focusAction(id, action) {
    const row = rows.get(id);
    if (action === 'name' && row?.input) {
      row.input.focus();
      row.input.setSelectionRange(row.input.value.length, row.input.value.length);
      return;
    }
    row?.buttons[action]?.focus();
  }

  // ---------------------------------------------------------- actions --
  /** Copies `item` into the theme as an image (at `at`, else in the middle) or the background. */
  async function act(item, target, at) {
    if (busy.has(item.id)) return;
    busy.add(item.id);
    clearProblem();
    announce('collection.using', { name: item.name });
    try {
      await use(item, target, at);
      announce(target === 'image' ? 'collection.addedImage' : 'collection.addedBackground', { name: item.name });
    } catch (e) {
      silence();
      failed(e);
    } finally {
      busy.delete(item.id);
    }
  }

  /** Asks first, naming the themes that use `item`; deletes it only once the user confirms. */
  async function confirmDelete(item) {
    const users = await bridge.collectedUsers(item.id);
    const answer = await askChoice(t, {
      title: t('collection.deleteTitle', { name: item.name }),
      body: deleteText(t, users, locale()),
      actions: [{ id: 'delete', label: t('collection.deleteAction'), kind: 'danger' }],
      initial: 'cancel',
    });
    if (answer !== 'delete') return;
    const next = focusAfterRemoval(filterCollection(list.items(), filter).map((known) => known.id), item.id);
    await bridge.deleteCollected(item.id, true);
    if (editing?.id === item.id) editing = null;
    list.removed(item.id);
    render();
    if (next) focusAction(next, 'delete');
    else searchButton.focus();
    announce('collection.deleted', { name: item.name });
  }

  async function remove(item) {
    if (busy.has(item.id)) return;
    busy.add(item.id);
    clearProblem();
    try {
      await confirmDelete(item);
    } catch (e) {
      failed(e);
      if (e?.code === 'notInCollection') void refresh();
    } finally {
      busy.delete(item.id);
    }
  }

  // ----------------------------------------------------------- rename --
  function startRename(item) {
    if (editing?.id === item.id) {
      focusAction(item.id, 'name');
      return;
    }
    clearProblem();
    editing = { id: item.id, value: item.name, error: null, saving: false };
    replaceRow(item);
    const { input } = rows.get(item.id) ?? {};
    input?.focus();
    input?.select();
  }

  /** Ends the rename of `item` and draws its row again; the focus can go back to its Rename. */
  function stopRename(item, refocus) {
    editing = null;
    replaceRow(item);
    if (refocus) focusAction(item.id, 'rename');
  }

  /**
   * The name is refused (empty, or the backend said why): the field says so,
   * with the text of the translation key `key` (and `params`), and keeps the
   * focus. It takes a key, never a text.
   */
  function refuseName(key, params) {
    const text = t(key, params);
    editing.error = text;
    const row = rows.get(editing.id);
    row.error.textContent = text;
    row.input.setAttribute('aria-invalid', 'true');
    row.input.focus();
  }

  async function saveName(item) {
    if (editing?.id !== item.id || editing.saving) return;
    const name = cleanName(editing.value);
    if (!name) {
      refuseName('collection.nameEmpty');
      return;
    }
    if (name === item.name) {
      stopRename(item, true);
      return;
    }
    editing.saving = true;
    try {
      const renamed = await bridge.renameCollected(item.id, name);
      list.renamed(renamed);
      stopRename(list.find(item.id) ?? { ...item, name: renamed.name }, true);
      announce('collection.renamed', { old: item.name, name: renamed.name });
    } catch (e) {
      if (editing?.id !== item.id) return;
      editing.saving = false;
      const why = errorMessage(t, e);
      refuseName(why.key, why.params);
    }
  }

  function renameKeydown(evt, item) {
    const action = renameKey(evt.key);
    if (!action) return;
    evt.preventDefault();
    evt.stopPropagation();
    if (action === 'save') void saveName(item);
    else stopRename(item, true);
  }

  /** Leaving the field saves a new name, and leaves the old one otherwise. */
  function renameBlur(item) {
    if (drawing || editing?.id !== item.id || editing.saving) return;
    const name = cleanName(editing.value);
    if (name && name !== item.name) void saveName(item);
    else stopRename(item, false);
  }

  function renameField(item, nameId) {
    const errorId = `${nameId}-error`;
    const hintId = `${nameId}-hint`;
    const input = el('input', {
      type: 'text', class: 'collection-name-input', value: editing.value, autocomplete: 'off', spellcheck: 'false',
      'aria-label': t('collection.nameField', { name: item.name }), 'aria-describedby': `${hintId} ${errorId}`, 'aria-invalid': String(Boolean(editing.error)),
    });
    input.addEventListener('input', () => {
      if (editing?.id === item.id) editing.value = input.value;
    });
    input.addEventListener('keydown', (evt) => renameKeydown(evt, item));
    input.addEventListener('blur', () => renameBlur(item));
    const error = el('p', { id: errorId, class: 'field-error', role: 'alert', text: editing.error ?? '' });
    const field = el('div', { class: 'collection-rename' }, [input, el('p', { id: hintId, class: 'hint', text: t('collection.renameHint') }), error]);
    return { input, error, field };
  }

  // ------------------------------------------------------------- rows --
  /** Makes `source` drag `item` onto the canvas, where it becomes an image. */
  function draggable(source, item, onClick) {
    makeDraggable(source, { label: item.name, canvas, stage, onDrop: (at) => void act(item, 'image', at), onClick });
  }

  function actionButtons(item, nameId) {
    const button = (action, text, onclick) => el('button', { type: 'button', class: `text-button collection-${action}`, dataset: { action }, 'aria-describedby': nameId, text, onclick });
    const image = button('image', t('collection.addImage'), null);
    // Like the widgets: a press adds it in the middle, a drag where it is dropped.
    draggable(image, item, () => void act(item, 'image', null));
    return {
      image,
      background: button('background', t('collection.useBackground'), () => void act(item, 'background', null)),
      rename: button('rename', t('collection.rename'), () => startRename(item)),
      delete: button('delete', t('collection.delete'), () => void remove(item)),
    };
  }

  function thumbOf(item, buttons) {
    const url = previewOf(item);
    const picture = url ? el('img', { alt: '', src: url, width: item.width, height: item.height, draggable: 'false' }) : icon(ICONS.image, 22);
    const thumb = el('span', { class: 'collection-thumb', 'aria-hidden': 'true', dataset: { kind: item.kind } }, [picture]);
    draggable(thumb, item, () => buttons.image.focus());
    return thumb;
  }

  function itemRow(item) {
    made += 1;
    const nameId = `collection-name-${made}`;
    const buttons = actionButtons(item, nameId);
    const renaming = editing?.id === item.id ? renameField(item, nameId) : null;
    const li = el('li', { class: 'collection-item', dataset: { kind: item.kind } }, [
      thumbOf(item, buttons),
      el('div', { class: 'collection-info' }, [
        el('strong', { id: nameId, class: 'collection-name', text: item.name, hidden: Boolean(renaming) }),
        renaming?.field ?? null,
        el('span', { class: 'collection-facts', text: itemFacts(t, item, locale()) }),
      ]),
      el('div', { class: 'collection-actions' }, ACTIONS.map((action) => buttons[action])),
    ]);
    rows.set(item.id, { li, buttons, input: renaming?.input ?? null, error: renaming?.error ?? null });
    return li;
  }

  /** Draws one item again (the others keep their buttons, so a click on them is not lost). */
  function replaceRow(item) {
    const old = rows.get(item.id)?.li;
    if (!old?.isConnected) {
      render();
      return;
    }
    drawing = true;
    try {
      old.replaceWith(itemRow(item));
    } finally {
      drawing = false;
    }
  }

  // ------------------------------------------------------------- list --
  function chooseFilter(next) {
    filter = next;
    render();
  }

  function emptyState(items) {
    if (!items.length) {
      return [icon(ICONS.image, 22), el('p', { text: t('collection.empty') }), el('p', { class: 'hint', text: t('collection.emptyHint') })];
    }
    return [
      el('p', { text: t(`collection.noneOf.${filter}`) }),
      el('button', { type: 'button', class: 'text-button', text: t('collection.showAll'), onclick: () => {
        chooseFilter('all');
        parts.filters.querySelector('[data-filter="all"]').focus();
      } }),
    ];
  }

  function paint(items, shown) {
    for (const button of parts.filters.querySelectorAll('[data-filter]')) button.setAttribute('aria-pressed', String(button.dataset.filter === filter));
    parts.count.textContent = collectionCount(t, items, shown, locale());
    parts.count.hidden = !items.length;
    parts.loading.hidden = list.status() !== 'loading' || items.length > 0;
    parts.drag.hidden = !shown.length;
    parts.list.hidden = !shown.length;
    const empty = list.status() === 'ready' && !shown.length;
    parts.empty.hidden = !empty;
    parts.empty.replaceChildren(...(empty ? emptyState(items) : []));
    paintProblem();
  }

  /** Draws the list as it is now; the focus stays on the action (or field) that had it. */
  function render() {
    const kept = focusedAction();
    const items = list.items();
    const shown = filterCollection(items, filter);
    if (editing && !shown.some((item) => item.id === editing.id)) editing = null;
    paint(items, shown);
    drawing = true;
    try {
      rows.clear();
      parts.list.replaceChildren(...shown.map(itemRow));
    } finally {
      drawing = false;
    }
    if (kept) focusAction(kept.id, kept.action);
  }

  /** Reads the collection again (previews still or moving as motion is now). */
  async function refresh() {
    const loaded = list.refresh();
    render();
    if (await loaded) render();
  }

  function build() {
    parts.filters = el('div', { class: 'segmented collection-filter', role: 'group', 'aria-label': t('collection.filter') }, COLLECTION_FILTERS.map((f) => el('button', {
      type: 'button', dataset: { filter: f }, 'aria-pressed': String(f === filter), text: t(`collection.filter.${f}`), onclick: () => chooseFilter(f),
    })));
    parts.count = el('p', { class: 'hint collection-count' });
    parts.loading = el('p', { class: 'hint', text: t('collection.loading'), hidden: true });
    parts.problem = el('div', { class: 'collection-problem', role: 'alert' });
    parts.status = el('p', { class: 'hint collection-status', role: 'status' });
    parts.drag = el('p', { class: 'hint', text: t('collection.dragHint') });
    parts.list = el('ul', { class: 'collection-list', 'aria-label': t('collection.list') });
    parts.empty = el('div', { class: 'collection-empty', hidden: true });
    root.replaceChildren(parts.filters, parts.count, parts.loading, parts.problem, parts.status, parts.drag, parts.list, parts.empty);
  }

  build();
  render();
  motion?.addEventListener?.('change', () => void refresh());

  return {
    /** The Collection subtab was shown: the list is read again. */
    show: () => refresh(),
    /** The collection changed elsewhere (an item was added from the search). */
    refresh,
    /** The UI's language changed. */
    retranslate() {
      const text = parts.status.textContent;
      build();
      parts.status.textContent = text;
      render();
    },
  };
}

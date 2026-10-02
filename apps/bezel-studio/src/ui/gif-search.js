// The "Search GIFs and stickers" dialog (D-2026-10-01-gif-sticker-search-3,
// -4), opened from the Media tab's Collection: the user's own KLIPY key at
// the top, with a "?" that discloses how to get one; GIFs | Stickers; the
// search field ("Search KLIPY", KLIPY's attribution); "Show explicit
// results" (off whenever the app starts, kept while it runs); a grid of
// results with "Load more", and "Powered by KLIPY". Nothing is asked of
// KLIPY when it opens. The logic lives in `../gif-search.js`.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { errorText } from '../messages.js';
import {
  GIF_KINDS, GUIDE_PAGE, PARTNER_PANEL, createGifResults, createSearchTrigger, gridMove, keyFailure, keyStatus, queryOf, resultsText, searchFailure,
} from '../gif-search.js';

const HELP_STEPS = Object.freeze(['gifs.helpStep1', 'gifs.helpStep2', 'gifs.helpStep3', 'gifs.helpStep4']);
/** What a result's button says while it is added, and once it is in the collection. */
const TILE_TEXT = Object.freeze({ idle: 'gifs.add', adding: 'gifs.adding', added: 'gifs.inCollection' });

/** Whether `url` is a picture the backend sent (`data:image/…`), the only kind shown. */
const isPicture = (url) => typeof url === 'string' && url.startsWith('data:image/');

/** How many results the grid shows in a row (those on the first row's line). */
function columnsOf(grid) {
  const cells = [...grid.children];
  if (!cells.length) return 1;
  const row = cells.findIndex((cell) => cell.offsetTop !== cells[0].offsetTop);
  return row === -1 ? cells.length : row;
}

/**
 * @param {object} deps
 * @param {(k: string, p?: object) => string} deps.t
 * @param {object} deps.bridge
 * @param {() => string} deps.locale the UI's language, for the guide
 * @param {(item: object) => void} [deps.onCollected] an item reached the collection (`CollectedDto`)
 */
export function createGifSearch({ t, bridge, locale, onCollected = () => {} }) {
  // Kept while the app runs, never saved: explicit results are off at every start.
  const session = { kind: 'gif', explicit: false };
  const motion = globalThis.matchMedia?.('(prefers-reduced-motion: reduce)') ?? null;
  /** The open dialog: its parts and what it shows; `null` when closed. */
  let ui = null;

  const announce = (text) => {
    const { live } = ui.parts;
    live.textContent = live.textContent === text ? `${text}\u00a0` : text;
  };
  const motionChanged = () => ui?.results.refreshPreviews();

  // ------------------------------------------------------------ links --
  function openPartnerPanel() {
    void bridge.openLink(PARTNER_PANEL).catch((e) => announce(errorText(t, e)));
  }

  function openGuide() {
    void bridge.openGuide(GUIDE_PAGE, locale()).catch((e) => announce(errorText(t, e)));
  }

  // ------------------------------------------------------------- help --
  const helpOpen = () => Boolean(ui) && !ui.parts.help.hidden;

  /** Shows or hides the key help; hiding it can give the focus back to its "?". */
  function setHelp(open, { refocus = false } = {}) {
    const { help, helpButton } = ui.parts;
    help.hidden = !open;
    helpButton.setAttribute('aria-expanded', String(open));
    if (open) help.focus();
    else if (refocus) helpButton.focus();
  }

  function helpPanel() {
    return el('div', { id: 'gif-key-help', class: 'key-help', role: 'group', 'aria-labelledby': 'gif-key-help-title', tabindex: '-1', hidden: true }, [
      el('h3', { id: 'gif-key-help-title', text: t('gifs.helpTitle') }),
      el('ol', {}, HELP_STEPS.map((key) => el('li', { text: t(key) }))),
      el('p', { class: 'hint', text: t('gifs.helpPrivacy') }),
      el('div', { class: 'button-row' }, [
        el('button', { type: 'button', class: 'text-button', text: t('gifs.partnerPanel'), onclick: openPartnerPanel }),
        el('button', { type: 'button', class: 'text-button', text: t('gifs.guide'), onclick: openGuide }),
      ]),
    ]);
  }

  /** Whether no control of the dialog has the focus (a click or a hidden control left it nowhere). */
  function focusLost() {
    const active = document.activeElement;
    return !active || active === ui.dialog || !ui.dialog.contains(active);
  }

  /** A click outside the open help closes it; the focus it held goes back to the "?". */
  function clickedAway(evt) {
    const { help, helpButton } = ui.parts;
    if (!helpOpen() || help.contains(evt.target) || helpButton.contains(evt.target)) return;
    setHelp(false, { refocus: focusLost() || help.contains(document.activeElement) });
  }

  /** Esc closes the open help, not the dialog. */
  function escapeHelp(evt) {
    if (evt.key !== 'Escape' || !helpOpen()) return;
    evt.preventDefault();
    evt.stopPropagation();
    ui.swallowCancel = true;
    setHelp(false, { refocus: true });
  }

  // -------------------------------------------------------------- key --
  function keyProblem(text) {
    const { keyError, keyInput } = ui.parts;
    keyError.textContent = text ?? '';
    keyInput.setAttribute('aria-invalid', String(Boolean(text)));
  }

  /** The key's line, Remove, and whether searching is possible (it needs a key). */
  function syncKey() {
    const { keyStatusLine, remove, query, queryHint, trending } = ui.parts;
    const ready = Boolean(ui.key?.configured);
    keyStatusLine.textContent = keyStatus(t, ui.key);
    remove.hidden = !ready;
    query.disabled = !ready;
    trending.disabled = !ready;
    queryHint.textContent = ready ? t('gifs.searchHint') : t('gifs.needKey');
  }

  async function saveKey(evt) {
    evt.preventDefault();
    const { keyInput } = ui.parts;
    const value = keyInput.value.trim();
    if (!value) {
      keyProblem(t('gifs.keyEmpty'));
      return;
    }
    if (ui.saving) return;
    ui.saving = true;
    try {
      ui.key = await bridge.saveKlipyKey(value);
      keyInput.value = '';
      keyProblem(null);
      syncKey();
      announce(t('gifs.keySavedNow'));
    } catch (e) {
      keyProblem(keyFailure(t, e));
    } finally {
      ui.saving = false;
    }
  }

  async function removeKey() {
    try {
      ui.key = await bridge.removeKlipyKey();
      ui.trigger.cancel();
      keyProblem(null);
      syncKey();
      announce(t('gifs.keyRemoved'));
      ui.parts.keyInput.focus();
    } catch (e) {
      keyProblem(errorText(t, e));
    }
  }

  function keyForm(parts) {
    parts.keyInput = el('input', { id: 'gif-key-input', type: 'password', autocomplete: 'off', spellcheck: 'false', 'aria-describedby': 'gif-key-status gif-key-error' });
    parts.remove = el('button', { type: 'button', class: 'text-button', text: t('gifs.keyRemove'), hidden: true, onclick: () => void removeKey() });
    parts.helpButton = el('button', {
      type: 'button', class: 'icon-button key-help-button', text: '?', title: t('gifs.help'), 'aria-label': t('gifs.help'),
      'aria-expanded': 'false', 'aria-controls': 'gif-key-help', onclick: () => setHelp(!helpOpen()),
    });
    parts.keyStatusLine = el('p', { id: 'gif-key-status', class: 'hint' });
    parts.keyError = el('p', { id: 'gif-key-error', class: 'field-error', role: 'alert' });
    parts.help = helpPanel();
    return el('form', { class: 'gif-key', onsubmit: (evt) => void saveKey(evt) }, [
      el('label', { for: 'gif-key-input', text: t('gifs.key') }),
      el('div', { class: 'gif-key-row' }, [parts.keyInput, el('button', { type: 'submit', class: 'text-button', text: t('gifs.keySave') }), parts.remove, parts.helpButton, parts.help]),
      parts.keyStatusLine,
      parts.keyError,
    ]);
  }

  // ----------------------------------------------------------- search --
  /** Page 1 of `text` with the kind and the filter chosen; trending when it is empty. */
  function runSearch(text) {
    if (!ui?.key?.configured) return;
    ui.asked = queryOf({ kind: session.kind, text, explicit: session.explicit });
    void ui.results.search(ui.asked);
  }

  /** The kind or the filter changed: what was searched, again from page 1. */
  function searchAgain() {
    if (!ui.asked) return;
    ui.trigger.cancel();
    runSearch(ui.asked.text);
  }

  function chooseKind(kind) {
    session.kind = kind;
    for (const button of ui.parts.kinds.querySelectorAll('[data-kind]')) button.setAttribute('aria-pressed', String(button.dataset.kind === kind));
    searchAgain();
  }

  function searchForm(parts) {
    parts.kinds = el('div', { class: 'segmented gif-kinds', role: 'group', 'aria-label': t('gifs.kinds') }, GIF_KINDS.map((kind) => el('button', {
      type: 'button', 'aria-pressed': String(kind === session.kind), dataset: { kind }, text: t(`gifs.kind.${kind}`), onclick: () => chooseKind(kind),
    })));
    parts.query = el('input', {
      id: 'gif-query', type: 'search', placeholder: t('gifs.search'), 'aria-label': t('gifs.search'), 'aria-describedby': 'gif-query-hint',
      autocomplete: 'off', spellcheck: 'false', enterkeyhint: 'search', oninput: () => ui.trigger.typed(parts.query.value),
    });
    parts.explicit = el('input', { id: 'gif-explicit', type: 'checkbox', role: 'switch', checked: session.explicit });
    parts.explicit.addEventListener('change', () => {
      session.explicit = parts.explicit.checked;
      searchAgain();
    });
    return el('form', { class: 'gif-controls', role: 'search', onsubmit: (evt) => {
      evt.preventDefault();
      ui.trigger.submit(parts.query.value);
    } }, [
      parts.kinds,
      el('label', { class: 'search gif-query' }, [parts.query]),
      el('label', { class: 'switch gif-explicit' }, [
        parts.explicit,
        el('span', { class: 'track', 'aria-hidden': 'true' }, [el('span', { class: 'thumb' })]),
        el('span', { text: t('gifs.explicit') }),
      ]),
    ]);
  }

  // ---------------------------------------------------------- results --
  function setActive(index, focus) {
    const tiles = ui.parts.grid.querySelectorAll('.gif-tile');
    ui.active = index;
    for (const [i, tile] of tiles.entries()) tile.tabIndex = i === index ? 0 : -1;
    if (focus) tiles[index]?.focus();
  }

  function gridKey(evt) {
    const next = gridMove(ui.active, evt.key, ui.results.view().items.length, columnsOf(ui.parts.grid));
    if (next === null) return;
    evt.preventDefault();
    setActive(next, true);
  }

  function markTile(tile, state) {
    tile.dataset.state = state;
    tile.setAttribute('aria-busy', String(state === 'adding'));
    tile.querySelector('.gif-action').textContent = t(TILE_TEXT[state]);
  }

  async function collect(item, tile) {
    const mine = ui;
    if (mine.adding.has(item.id)) return;
    mine.adding.add(item.id);
    markTile(tile, 'adding');
    try {
      const added = await bridge.collectGif(item.id);
      mine.collected.add(item.id);
      onCollected(added);
      if (ui !== mine) return;
      markTile(tile, 'added');
      announce(t('gifs.added', { name: added.name }));
    } catch (e) {
      if (ui !== mine) return;
      markTile(tile, mine.collected.has(item.id) ? 'added' : 'idle');
      showProblem(searchFailure(t, e), () => void collect(item, tile));
    } finally {
      mine.adding.delete(item.id);
    }
  }

  function tileOf(item) {
    const frame = el('span', { class: 'gif-frame', dataset: { state: 'loading' } }, [el('img', { alt: '', width: item.width, height: item.height, decoding: 'async' })]);
    ui.frames.set(item.id, frame);
    const state = ui.collected.has(item.id) ? 'added' : 'idle';
    const tile = el('button', { type: 'button', class: 'gif-tile', tabindex: '-1', dataset: { state } }, [
      frame,
      el('span', { class: 'gif-name', text: item.title }),
      el('span', { class: 'gif-action', text: t(TILE_TEXT[state]) }),
    ]);
    tile.addEventListener('click', () => void collect(item, tile));
    return el('li', {}, [tile]);
  }

  function paintPreview(id, url) {
    const frame = ui?.frames.get(id);
    if (!frame) return;
    if (isPicture(url)) frame.querySelector('img').src = url;
    frame.dataset.state = isPicture(url) ? 'ready' : 'none';
  }

  /** The first result, else the search field, gets the focus that the page just lost. */
  function keepFocus() {
    if (!focusLost()) return;
    const first = ui.parts.grid.querySelector('.gif-tile');
    (first ?? ui.parts.query).focus();
  }

  function showPage(view, more) {
    const { grid, start, startText, loadMore } = ui.parts;
    const shown = more ? view.items.slice(view.items.length - view.added) : view.items;
    if (!more) {
      ui.frames.clear();
      grid.replaceChildren();
    }
    const firstNew = grid.children.length;
    grid.append(...shown.map(tileOf));
    setActive(more ? Math.min(ui.active, view.items.length - 1) : 0, false);
    start.hidden = view.items.length > 0;
    if (!view.items.length) startText.textContent = resultsText(t, { count: 0, text: view.query.text });
    loadMore.hidden = !view.hasNext;
    announce(resultsText(t, { count: shown.length, text: view.query.text, more }));
    // "Load more" went away with the last page: its focus goes to the first new result.
    if (more && shown.length && focusLost()) grid.children[firstNew].querySelector('.gif-tile').focus();
    keepFocus();
  }

  /** A failure, with what fixes it: the Partner Panel, the key help or a retry. */
  function showProblem(failure, retry = null) {
    const { problem } = ui.parts;
    if (!failure) {
      problem.replaceChildren();
      return;
    }
    const actions = {
      partnerPanel: () => el('button', { type: 'button', class: 'text-button', text: t('gifs.partnerPanel'), onclick: openPartnerPanel }),
      retry: () => retry && el('button', { type: 'button', class: 'text-button', text: t('gifs.retry'), onclick: retry }),
      help: () => null,
    };
    problem.replaceChildren(el('div', { class: 'dialog-warning' }, [
      icon(ICONS.warning, 18),
      el('div', {}, [el('p', { text: failure.text }), failure.detail && el('p', { text: failure.detail }), actions[failure.action]()]),
    ]));
    if (failure.action === 'help') setHelp(true);
  }

  function resultsChanged(view, reason) {
    const { area, status } = ui.parts;
    const loading = reason === 'loading';
    area.setAttribute('aria-busy', String(loading));
    status.hidden = !loading;
    if (loading) showProblem(null);
    else if (reason === 'error') showProblem(searchFailure(t, view.error), () => void ui.results.retry());
    else showPage(view, reason === 'more');
  }

  function resultsArea(parts) {
    parts.status = el('p', { class: 'hint gif-status', text: t('gifs.searching'), hidden: true });
    parts.problem = el('div', { class: 'gif-problem', role: 'alert' });
    parts.trending = el('button', { type: 'button', class: 'text-button', text: t('gifs.trending'), 'aria-describedby': 'gif-query-hint', onclick: () => {
      parts.query.value = '';
      ui.trigger.submit('');
    } });
    parts.startText = el('p', { text: t('gifs.start') });
    parts.start = el('div', { class: 'gif-start' }, [parts.startText, parts.trending]);
    parts.grid = el('ul', { class: 'gif-grid', 'aria-label': t('gifs.results'), onkeydown: gridKey });
    parts.grid.addEventListener('focusin', (evt) => {
      const tiles = [...parts.grid.querySelectorAll('.gif-tile')];
      const index = tiles.indexOf(evt.target);
      if (index >= 0) setActive(index, false);
    });
    parts.loadMore = el('button', { type: 'button', class: 'text-button gif-more', text: t('gifs.loadMore'), hidden: true, onclick: () => void ui.results.more() });
    parts.area = el('div', { class: 'gif-results', 'aria-busy': 'false' }, [parts.status, parts.problem, parts.start, parts.grid, parts.loadMore]);
    return parts.area;
  }

  // ----------------------------------------------------------- dialog --
  function build() {
    const parts = {};
    const close = el('button', { type: 'button', class: 'icon-button dialog-close', title: t('dialog.close'), 'aria-label': t('dialog.close') }, [icon(ICONS.close, 16)]);
    parts.live = el('p', { class: 'visually-hidden', 'aria-live': 'polite' });
    parts.queryHint = el('p', { id: 'gif-query-hint', class: 'hint' });
    const dialog = el('dialog', { class: 'confirm-dialog gif-dialog', 'aria-labelledby': 'gif-title' }, [
      el('div', { class: 'dialog-head' }, [el('h2', { id: 'gif-title', text: t('gifs.title') }), close]),
      el('div', { class: 'gif-body' }, [
        keyForm(parts),
        searchForm(parts),
        parts.queryHint,
        resultsArea(parts),
        el('p', { class: 'gif-attribution', text: t('gifs.poweredBy') }),
        parts.live,
      ]),
    ]);
    close.addEventListener('click', () => dialog.close());
    return { dialog, parts };
  }

  function closed(opener) {
    ui.results.close();
    ui.trigger.cancel();
    motion?.removeEventListener?.('change', motionChanged);
    ui.dialog.remove();
    ui = null;
    if (opener?.isConnected) opener.focus();
  }

  function listen(dialog, opener) {
    dialog.addEventListener('click', clickedAway);
    dialog.addEventListener('keydown', escapeHelp);
    dialog.addEventListener('keyup', (evt) => {
      if (evt.key === 'Escape') ui.swallowCancel = false;
    });
    dialog.addEventListener('cancel', (evt) => {
      if (!ui.swallowCancel) return;
      evt.preventDefault();
      ui.swallowCancel = false;
    });
    dialog.addEventListener('close', () => closed(opener), { once: true });
    motion?.addEventListener?.('change', motionChanged);
  }

  /** Opens the dialog (asks only for the key's last 4 characters); focus returns to the opener. */
  async function open() {
    if (ui) return;
    const opener = document.activeElement;
    let keyError = null;
    const key = await bridge.klipyKey().catch((e) => {
      keyError = e;
      return null;
    });
    if (ui) return;
    const { dialog, parts } = build();
    ui = { dialog, parts, key, asked: null, active: 0, frames: new Map(), adding: new Set(), collected: new Set(), swallowCancel: false, saving: false };
    ui.results = createGifResults({
      search: (query) => bridge.searchGifs(query),
      preview: (id, still) => bridge.gifPreview(id, still),
      still: () => Boolean(motion?.matches),
      onChange: resultsChanged,
      onPreview: paintPreview,
    });
    ui.trigger = createSearchTrigger({ search: runSearch });
    syncKey();
    if (keyError) keyProblem(errorText(t, keyError));
    listen(dialog, opener);
    document.body.append(dialog);
    dialog.showModal();
    (key?.configured ? parts.query : parts.keyInput).focus();
  }

  return { open, isOpen: () => ui !== null };
}

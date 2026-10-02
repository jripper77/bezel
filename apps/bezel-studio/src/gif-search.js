// The GIF and sticker search's logic (D-2026-10-01-gif-sticker-search-3,
// -4): typing searches after a pause and Enter at once, the result grid's
// keys, the pages of a search (each result once, "Load more" appends), the
// previews asked a few at a time, and what each failure says and offers.
// Nothing here touches the DOM: `ui/gif-search.js` draws the dialog. Nothing
// here reaches KLIPY either: every call goes through the backend, which
// names a result by id and answers its preview as a `data:` URL.
import { errorText } from './messages.js';

/** What KLIPY searches, in the order the dialog offers them. */
export const GIF_KINDS = Object.freeze(['gif', 'sticker']);
/** The pause after the last key before typing searches, ms. */
export const SEARCH_DELAY_MS = 600;
/** The fewest characters typing searches with (Enter searches with any). */
export const MIN_SEARCH_CHARS = 2;
/** The most previews asked of the backend at once. */
export const PREVIEWS_AT_ONCE = 6;
/** The link of the key help and of a 429 (`open_link`'s allow-list). */
export const PARTNER_PANEL = 'klipyPartnerPanel';
/** The user guide's page (`open_guide`). */
export const GUIDE_PAGE = 'gifs-and-stickers';

/** What a failed search offers, by its code; anything else offers a retry. */
const FAILURE_ACTIONS = new Map([
  ['klipyRateLimited', 'partnerPanel'],
  ['klipyKeyRejected', 'help'],
  ['klipyNoKey', 'help'],
]);

/** The characters of `text` (code points, so an emoji is one). */
const lengthOf = (text) => [...text].length;

/**
 * Typing searches once it pauses for `delay` with `MIN_SEARCH_CHARS` or
 * more; Enter (`submit`) searches at once, even an empty text (trending).
 * Each new key or a submit drops the search still waiting.
 * @param {object} deps
 * @param {(text: string) => void} deps.search
 * @param {number} [deps.delay]
 * @param {(ms: number, fn: () => void) => unknown} [deps.wait]
 * @param {(handle: unknown) => void} [deps.cancel]
 */
export function createSearchTrigger({ search, delay = SEARCH_DELAY_MS, wait = (ms, fn) => setTimeout(fn, ms), cancel = (handle) => clearTimeout(handle) }) {
  let pending = null;
  const stop = () => {
    if (pending !== null) cancel(pending);
    pending = null;
  };
  return {
    /** The search field changed to `text`. */
    typed(text) {
      stop();
      const query = text.trim();
      if (lengthOf(query) < MIN_SEARCH_CHARS) return;
      pending = wait(delay, () => {
        pending = null;
        search(query);
      });
    },
    /** Enter in the search field: `text` now. */
    submit(text) {
      stop();
      search(text.trim());
    },
    /** Drops the search waiting, if any. */
    cancel: stop,
    /** Whether a search waits for the pause. */
    waiting: () => pending !== null,
  };
}

/** Where each key of the result grid goes from `i` of `n` in rows of `c`. */
const MOVES = new Map([
  ['ArrowRight', (i, n) => Math.min(i + 1, n - 1)],
  ['ArrowLeft', (i) => Math.max(i - 1, 0)],
  ['ArrowDown', (i, n, c) => (i + c < n ? i + c : i)],
  ['ArrowUp', (i, n, c) => (i - c >= 0 ? i - c : i)],
  ['Home', () => 0],
  ['End', (i, n) => n - 1],
]);

/**
 * The result the grid's `key` moves to from `index`: the arrows by one
 * result or one row (staying put at an edge), Home and End to the first and
 * the last; `null` for any other key or an empty grid.
 * @param {number} index
 * @param {string} key a `KeyboardEvent.key`
 * @param {number} count results shown
 * @param {number} columns results in a row
 * @returns {number|null}
 */
export function gridMove(index, key, count, columns) {
  const move = MOVES.get(key);
  if (!move || count <= 0) return null;
  const from = Math.min(Math.max(index, 0), count - 1);
  return move(from, count, Math.max(1, columns));
}

/**
 * `incoming` appended to `shown`, without the results already shown (a
 * later page of KLIPY may repeat some).
 * @template {{id: string}} T
 * @param {T[]} shown
 * @param {T[]} incoming
 * @returns {{items: T[], added: T[]}}
 */
export function mergeResults(shown, incoming) {
  const seen = new Set(shown.map((item) => item.id));
  const added = [];
  for (const item of incoming) {
    if (seen.has(item.id)) continue;
    seen.add(item.id);
    added.push(item);
  }
  return { items: [...shown, ...added], added };
}

/**
 * The query `search_gifs` takes for page 1 of `text`.
 * @param {{kind: string, text: string, explicit: boolean}} choice
 */
export function queryOf({ kind, text, explicit }) {
  return { kind: GIF_KINDS.includes(kind) ? kind : 'gif', text: String(text ?? '').trim(), page: 1, explicit: Boolean(explicit) };
}

/**
 * Asks previews `atOnce` at a time; `reset` forgets those waiting and drops
 * the answers still to come.
 * @param {object} deps
 * @param {(id: string) => Promise<string|null>} deps.fetch
 * @param {(id: string, url: string|null) => void} deps.onPreview
 * @param {number} [deps.atOnce]
 */
export function createPreviewQueue({ fetch, onPreview, atOnce = PREVIEWS_AT_ONCE }) {
  let waiting = [];
  let running = 0;
  let round = 0;

  function start(id) {
    const mine = round;
    running += 1;
    const done = (url) => {
      running -= 1;
      if (mine === round) onPreview(id, url);
      pump();
    };
    void Promise.resolve()
      .then(() => fetch(id))
      .then((url) => done(url ?? null), () => done(null));
  }

  function pump() {
    while (running < atOnce && waiting.length) start(waiting.shift());
  }

  return {
    /** Asks the previews of `ids`, after those already waiting. */
    want(ids) {
      waiting.push(...ids);
      pump();
    },
    reset() {
      round += 1;
      waiting = [];
    },
  };
}

/**
 * The pages of one search at a time: `search` asks page 1 (the results shown
 * stay until it answers, and after a failure), `more` appends the next page
 * without the results already shown, and only the new results' previews are
 * asked. A search started later wins: the answers of an earlier one are
 * dropped.
 * @param {object} deps
 * @param {(query: {kind: string, text: string, page: number, explicit: boolean}) => Promise<{page?: number, hasNext?: boolean, items?: {id: string}[]}>} deps.search
 * @param {(id: string, still: boolean) => Promise<string|null>} deps.preview
 * @param {() => boolean} [deps.still] whether previews are stills (motion reduced)
 * @param {(view: object, reason: 'loading'|'new'|'more'|'error') => void} [deps.onChange]
 * @param {(id: string, url: string|null) => void} [deps.onPreview]
 * @param {number} [deps.atOnce]
 */
export function createGifResults({ search, preview, still = () => false, onChange = () => {}, onPreview = () => {}, atOnce = PREVIEWS_AT_ONCE }) {
  const view = { query: null, items: [], hasNext: false, status: 'idle', error: null, failed: null, added: 0 };
  const previews = createPreviewQueue({ fetch: (id) => preview(id, still()), onPreview, atOnce });
  let round = 0;

  async function ask(query, mode) {
    if (mode === 'new') round += 1;
    const mine = round;
    Object.assign(view, { status: 'loading', error: null, failed: null });
    onChange(view, 'loading');
    try {
      const page = await search(query);
      if (mine !== round) return;
      if (mode === 'new') previews.reset();
      const { items, added } = mergeResults(mode === 'new' ? [] : view.items, page?.items ?? []);
      Object.assign(view, { query, items, hasNext: Boolean(page?.hasNext), status: 'ready', added: added.length });
      onChange(view, mode);
      previews.want(added.map((item) => item.id));
    } catch (error) {
      if (mine !== round) return;
      Object.assign(view, { status: 'error', error, failed: { query, mode } });
      onChange(view, 'error');
    }
  }

  return {
    view: () => view,
    /** Page 1 of `query`. */
    search: (query) => ask({ ...query, page: 1 }, 'new'),
    /** The next page of the search shown, once at a time. */
    more() {
      if (!view.query || !view.hasNext || view.status === 'loading') return Promise.resolve();
      return ask({ ...view.query, page: view.query.page + 1 }, 'more');
    },
    /** The call that failed, again. */
    retry() {
      if (!view.failed) return Promise.resolve();
      return ask(view.failed.query, view.failed.mode);
    },
    /** Every preview shown again (motion was reduced or allowed meanwhile). */
    refreshPreviews() {
      previews.reset();
      previews.want(view.items.map((item) => item.id));
    },
    /** The dialog closed: nothing answered from now on reaches it. */
    close() {
      round += 1;
      previews.reset();
    },
  };
}

/**
 * What a failed call says and offers: a 429 explains the 100 requests per
 * hour of a test key and how to ask for production, with the Partner Panel;
 * a refused or missing key opens the key help; anything else a retry.
 * @param {(k: string, p?: object) => string} t
 * @param {unknown} error
 * @returns {{text: string, detail: string|null, action: 'partnerPanel'|'help'|'retry'}}
 */
export function searchFailure(t, error) {
  const code = typeof error?.code === 'string' ? error.code : null;
  const action = FAILURE_ACTIONS.get(code) ?? 'retry';
  const detail = action === 'partnerPanel' ? t('gifs.rateLimitedHow') : null;
  return { text: errorText(t, error), detail, action };
}

/**
 * Why the key was not saved: the characters the backend takes, or the
 * backend's own reason.
 * @param {(k: string, p?: object) => string} t
 * @param {unknown} error
 */
export function keyFailure(t, error) {
  return error?.code === 'invalidInput' ? t('gifs.keyInvalid') : errorText(t, error);
}

/**
 * What the key field says of the saved key (`klipy_key`): its last 4
 * characters, never the key; a short key, whose ending the backend keeps
 * (`last4: null`), is just saved.
 * @param {(k: string, p?: object) => string} t
 * @param {{configured: boolean, last4: string|null}|null} key
 */
export function keyStatus(t, key) {
  if (!key?.configured) return t('gifs.keyNone');
  return key.last4 ? t('gifs.keySaved', { last4: key.last4 }) : t('gifs.keySavedNoEnding');
}

/**
 * What the live region says when results come: how many, for what (none
 * for trending), or how many more a page added.
 * @param {(k: string, p?: object) => string} t
 * @param {{count: number, text: string, more?: boolean}} result
 */
export function resultsText(t, { count, text, more = false }) {
  if (more) return count > 0 ? t('gifs.announceMore', { count }) : t('gifs.announceNoMore');
  if (count === 0) return text ? t('gifs.noResults', { text }) : t('gifs.noTrending');
  if (!text) return t('gifs.announceTrending', { count });
  return t(count === 1 ? 'gifs.announceOne' : 'gifs.announce', { count, text });
}

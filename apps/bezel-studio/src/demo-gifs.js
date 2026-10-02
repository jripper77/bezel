// The demo's KLIPY and collection (D-2026-10-01-gif-sticker-search-6): the
// same calls and answers as the backend's GIF commands, over recorded
// results, so the dialog runs in a browser for Playwright. Nothing here
// reaches KLIPY. The scenarios (`demo-data.js`): `gifs` (a key saved),
// `gifsNoKey` and `gifsRateLimited` (every search answers 429); a key saved
// as `DEMO_REFUSED_KEY` is refused at the first search, like a key KLIPY
// does not know. Each query, preview and download asked of KLIPY and each
// link opened go to hooks, which the bridge shows on the page.

/** Results in a page (the core's `PAGE_SIZE`). */
export const DEMO_GIF_PAGE_SIZE = 24;
/** Pages the recorded results have for any text. */
export const DEMO_GIF_PAGES = 3;
/** Results a page repeats from the one before, like KLIPY's shifting pages. */
export const DEMO_GIF_OVERLAP = 2;
/** A key the demo takes and KLIPY refuses at the first search. */
export const DEMO_REFUSED_KEY = 'refused-key';
/** The links `open_link` opens. */
export const DEMO_LINKS = Object.freeze(['klipyPartnerPanel']);

/** The characters a key may have, like the backend checks it. */
const KEY_PATTERN = /^[\w-]{1,128}$/;
const KINDS = Object.freeze(['gif', 'sticker']);
const TARGETS = Object.freeze(['image', 'background']);
const TITLES = Object.freeze({
  gif: ['Happy dance', 'Thumbs up', 'Mind blown', 'Cat typing', 'Slow clap', 'Facepalm', 'Party parrot', 'Excited dog', 'Wow', 'Nodding yes',
    'Shrug', 'High five', 'Coffee time', 'Sleepy cat', 'Victory', 'Laughing', 'Applause', 'Popcorn', 'Fire', 'Rainbow', 'Waving hello',
    'Good morning', 'Game over', 'Level up', 'Loading', 'Hearts', 'Thank you', 'Oops'],
  sticker: ['Star', 'Heart', 'Sparkles', 'Rocket', 'Cool sunglasses', 'Thumbs up', 'Party hat', 'Ghost', 'Pizza slice', 'Cactus', 'Moon',
    'Lightning', 'Crown', 'Rainbow', 'Cat face', 'Robot', 'Planet', 'Flower', 'Coffee cup', 'Gamepad', 'Headphones', 'Trophy', 'Balloon',
    'Fire', 'Snowflake', 'Sun', 'Cloud', 'Music note'],
});
/** The sizes recorded GIFs come in (stickers are square). */
const GIF_SIZES = Object.freeze([[480, 270], [480, 480], [360, 480]]);

/** An error of the backend: `{code, args, message}` on an `Error`. */
const failure = (code, args, message) => Object.assign(new Error(message), { code, args });
const invalid = (detail) => failure('invalidInput', { detail }, `invalid input: ${detail}`);

/** The id part a text gives its results: `Dancing Cat!` → `dancing-cat`; empty for trending. */
export function demoGifSlug(text) {
  return String(text).toLowerCase().split(/[^a-z0-9]+/).filter(Boolean).join('-');
}

/** The `n`th recorded result of a kind for a text (0-based across pages). */
export function demoGifItem(kind, text, n) {
  const titles = TITLES[kind];
  const round = Math.floor(n / titles.length);
  const title = round ? `${titles[n % titles.length]} ${round + 1}` : titles[n % titles.length];
  const [width, height] = kind === 'sticker' ? [512, 512] : GIF_SIZES[n % GIF_SIZES.length];
  return { id: `${kind}-${demoGifSlug(text) || 'trending'}-${n + 1}`, title, width, height };
}

/**
 * A page of recorded results, like `search_gifs` answers it: every page
 * after the first repeats the last results of the one before.
 * @param {{kind: string, text: string, page: number}} query
 */
export function demoGifPage({ kind, text, page }) {
  const first = (page - 1) * (DEMO_GIF_PAGE_SIZE - DEMO_GIF_OVERLAP);
  const items = Array.from({ length: DEMO_GIF_PAGE_SIZE }, (_, i) => demoGifItem(kind, text, first + i));
  return { kind, text, page, hasNext: page < DEMO_GIF_PAGES, items };
}

/** A stable number from `text` (FNV-1a), for colors and ids. */
function hashOf(text) {
  let hash = 0x811c9dc5;
  for (const c of String(text)) hash = Math.imul(hash ^ c.codePointAt(0), 0x01000193) >>> 0;
  return hash;
}

/** A fake SHA-256 of a result's content: 64 hex digits from its id. */
export function demoContentId(id) {
  return Array.from({ length: 8 }, (_, i) => hashOf(`${i}:${id}`).toString(16).padStart(8, '0')).join('');
}

/**
 * A result's preview as a `data:` URL: an SVG of its size, a sticker
 * without a background (it is transparent), a GIF's moving unless `still`
 * (the JPEG still of a reduced-motion preview).
 */
export function demoGifPreview({ id, width, height, kind = 'gif' }, still) {
  const hue = hashOf(id) % 360;
  const r = Math.round(Math.min(width, height) / 4);
  const motion = still ? '' : `<animate attributeName="r" values="${r};${Math.round(r * 1.4)};${r}" dur="1.2s" repeatCount="indefinite"/>`;
  const ground = kind === 'sticker' ? '' : `<rect width="${width}" height="${height}" fill="hsl(${hue} 55% 35%)"/>`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${ground}`
    + `<circle cx="${width / 2}" cy="${height / 2}" r="${r}" fill="hsl(${(hue + 150) % 360} 80% 60%)">${motion}</circle></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg).replaceAll('(', '%28').replaceAll(')', '%29')}`;
}

/** Characters of a key of which the backend shows the last 4 (a shorter key would show most of itself). */
const SHOWN_FROM_CHARS = 9;

/** What `klipy_key` answers for `key`: whether one is saved and, for a long enough key, its last 4 characters. */
const keyDto = (key) => ({ configured: key !== null, last4: key !== null && key.length >= SHOWN_FROM_CHARS ? key.slice(-4) : null });

/** Checks a query like the backend's command. */
function checkQuery({ kind, text, page, explicit }) {
  if (!KINDS.includes(kind)) throw invalid(`GIF kind "${kind}"`);
  if (!Number.isInteger(page) || page < 1) throw invalid(`page ${page}`);
  return { kind, text: String(text ?? '').trim(), page, explicit: Boolean(explicit) };
}

/**
 * The demo backend's GIF commands.
 * @param {{key?: string|null, rateLimited?: boolean}} setup the scenario's `klipy`
 * @param {object} deps
 * @param {() => number} deps.now seconds since the epoch
 * @param {(item: object, target: 'image'|'background') => Promise<object>} deps.useInTheme copies an item into the theme (`AddedMediaDto`)
 * @param {(refs: string[]) => {themes: string[], openTheme: boolean}} deps.themesUsing the themes that use one of `refs`
 * @param {(query: object) => void} [deps.onQuery] each query asked of KLIPY
 * @param {(id: string) => void} [deps.onPreview] each preview fetched from KLIPY (a result's id)
 * @param {(id: string) => void} [deps.onCollect] each result downloaded from KLIPY into the collection (its id)
 * @param {(link: string) => void} [deps.onLink] each link opened
 */
export function createDemoGifs(setup, { now, useInTheme, themesUsing, onQuery = () => {}, onPreview = () => {}, onCollect = () => {}, onLink = () => {} }) {
  let key = setup.key ?? null;
  // The results of the last search (its pages so far), by id.
  let lastSearch = { signature: null, items: new Map() };
  // The collection, newest first, and the theme assets each item became.
  const collection = [];
  const refs = new Map();

  const resultOf = (id) => {
    const item = lastSearch.items.get(id);
    if (!item) throw failure('gifNotInResults', { item: id }, `${id} is not among the results of the last search`);
    return item;
  };
  const collected = (id) => {
    const item = collection.find((c) => c.id === id);
    if (!item) throw failure('notInCollection', { item: id }, `${id} is not in the collection`);
    return item;
  };

  /** Asks KLIPY (the recorded pages) like the backend: a key first, then the answer. */
  function askKlipy(query) {
    if (key === null) throw failure('klipyNoKey', {}, 'no KLIPY API key saved');
    onQuery({ ...query });
    if (setup.rateLimited) throw failure('klipyRateLimited', {}, 'KLIPY answered 429: the key reached its request limit');
    if (key === DEMO_REFUSED_KEY) throw failure('klipyKeyRejected', {}, 'KLIPY refused the API key');
    return demoGifPage(query);
  }

  /** Keeps the results of the search `query` belongs to (a new one replaces them). */
  function remember(query, page) {
    const signature = JSON.stringify([query.kind, query.text, query.explicit]);
    if (signature !== lastSearch.signature || query.page === 1) lastSearch = { signature, items: new Map() };
    for (const item of page.items) lastSearch.items.set(item.id, { ...item, kind: query.kind });
  }

  function collect(id) {
    const result = resultOf(id);
    onCollect(id);
    const content = demoContentId(id);
    const known = collection.find((c) => c.id === content);
    if (known) return { ...known };
    const item = {
      id: content, name: result.title, kind: result.kind, width: result.width, height: result.height,
      bytes: 180_000 + (hashOf(id) % 2_000_000), addedAt: Math.floor(now()),
      source: { provider: 'klipy', id, url: `https://klipy.com/${result.kind}s/${id}` },
      preview: demoGifPreview(result, false),
    };
    collection.unshift(item);
    return { ...item };
  }

  return {
    klipyKey: async () => keyDto(key),
    /** Saves the key like the backend: no request, only its characters checked. */
    saveKlipyKey: async (next) => {
      if (typeof next !== 'string' || !KEY_PATTERN.test(next)) throw invalid('a KLIPY key has only letters, digits, _ and -, up to 128');
      key = next;
      return keyDto(key);
    },
    removeKlipyKey: async () => {
      key = null;
      return keyDto(key);
    },
    /** A page of results (`text` empty: trending), like `search_gifs`. */
    searchGifs: async (query) => {
      const checked = checkQuery(query ?? {});
      const page = askKlipy(checked);
      remember(checked, page);
      return structuredClone(page);
    },
    /** The preview of a result of the last search, as a `data:` URL. */
    gifPreview: async (id, still) => {
      const result = resultOf(id);
      onPreview(id);
      return demoGifPreview(result, Boolean(still));
    },
    /** Adds a result of the last search to the collection, once per content. */
    collectGif: async (id) => collect(id),
    /** The collection, newest first, previews still or moving. */
    gifCollection: async (still) => collection.map((item) => ({ ...item, preview: demoGifPreview({ ...item, id: item.source.id }, Boolean(still)) })),
    renameCollected: async (id, name) => {
      const item = collected(id);
      const clean = String(name ?? '').trim();
      if (!clean) throw invalid('a collection item needs a name');
      item.name = clean;
      return { ...item };
    },
    /** The user's themes, and whether the open one, use an item's bytes. */
    collectedUsers: async (id) => {
      collected(id);
      return themesUsing([...(refs.get(id) ?? [])]);
    },
    deleteCollected: async (id, confirmed) => {
      const item = collected(id);
      if (!confirmed) throw failure('notConfirmed', { detail: 'deleting a collection item needs confirmation' }, 'deleting a collection item needs confirmation');
      collection.splice(collection.indexOf(item), 1);
      return null;
    },
    /** Copies an item into the theme as an image or its background, like `use_collected`. */
    useCollected: async (id, target) => {
      const item = collected(id);
      if (!TARGETS.includes(target)) throw invalid(`use as "${target}"`);
      const added = await useInTheme({ ...item }, target);
      refs.set(id, new Set([...(refs.get(id) ?? []), added.ref]));
      return added;
    },
    /** Opens a link of the fixed list (the demo shows which on the page). */
    openLink: async (link) => {
      if (!DEMO_LINKS.includes(link)) throw invalid(`link "${link}"`);
      onLink(link);
    },
  };
}

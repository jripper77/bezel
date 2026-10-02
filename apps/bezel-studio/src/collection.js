// The GIF and sticker collection's logic (D-2026-10-01-gif-sticker-search-5):
// which items the filter shows, how many and how large they are, what an
// item's line says, the names a rename takes, what deleting says of the
// themes that use an item, where the focus goes once an item leaves, and the
// list kept in step with the backend (a later load wins over an earlier one).
// Nothing here touches the DOM: `ui/collection.js` draws the list.
import { formatBytes } from './storage-manager.js';

/** What the collection's filter offers, in order: everything, GIFs, stickers. */
export const COLLECTION_FILTERS = Object.freeze(['all', 'gif', 'sticker']);

/** The names of the providers an item may come from, as they spell them. */
const PROVIDERS = new Map([['klipy', 'KLIPY']]);

/** What a key does while a name is edited in place. */
const RENAME_KEYS = new Map([
  ['Enter', 'save'],
  ['Escape', 'cancel'],
]);

/** An item's kind as the filter and the facts name it (`gif` unless a sticker). */
const kindOf = (item) => (item?.kind === 'sticker' ? 'sticker' : 'gif');

/**
 * The items `filter` keeps, in order (`all`, or anything unknown, keeps them all).
 * @template {{kind: string}} T
 * @param {T[]} items
 * @param {string} filter
 * @returns {T[]}
 */
export function filterCollection(items, filter) {
  if (filter === 'all' || !COLLECTION_FILTERS.includes(filter)) return [...items];
  return items.filter((item) => kindOf(item) === filter);
}

/** The bytes `items` take together (an unknown size counts as none). */
export function totalBytes(items) {
  return items.reduce((sum, item) => sum + (Number.isFinite(item.bytes) ? item.bytes : 0), 0);
}

/**
 * What the count line says: how many items the filter shows (and of how
 * many, when it leaves some out) and how large they are together; empty
 * for an empty collection.
 * @param {(k: string, p?: object) => string} t
 * @param {{bytes: number}[]} items the whole collection
 * @param {{bytes: number}[]} shown what the filter shows
 * @param {string} locale
 */
export function collectionCount(t, items, shown, locale) {
  if (!items.length) return '';
  const size = formatBytes(totalBytes(shown), locale);
  if (shown.length !== items.length) return t('collection.countOf', { shown: shown.length, count: items.length, size });
  return t(items.length === 1 ? 'collection.countOne' : 'collection.count', { count: items.length, size });
}

/** How a provider spells its name (`klipy` → `KLIPY`); an unknown one as it came. */
export function providerName(provider) {
  return PROVIDERS.get(provider) ?? String(provider ?? '');
}

/**
 * An item's facts, in a line: its kind, size in pixels, bytes and source.
 * @param {(k: string, p?: object) => string} t
 * @param {{kind: string, width?: number, height?: number, bytes?: number, source?: {provider?: string}}} item
 * @param {string} locale
 */
export function itemFacts(t, item, locale) {
  const facts = [t(`collection.kind.${kindOf(item)}`)];
  if (item.width > 0 && item.height > 0) facts.push(`${item.width}×${item.height}`);
  facts.push(formatBytes(item.bytes, locale));
  const provider = providerName(item.source?.provider);
  if (provider) facts.push(t('collection.source', { provider }));
  return facts.join(' · ');
}

/** An item's preview when it is a picture the backend sent (`data:image/…`), else `null`. */
export function previewOf(item) {
  const url = item?.preview;
  return typeof url === 'string' && url.startsWith('data:image/') ? url : null;
}

/** The name a rename gives: the text without surrounding spaces; `null` (refused) when nothing is left. */
export function cleanName(text) {
  const name = String(text ?? '').trim();
  return name || null;
}

/** What `key` does while a name is edited: `save` (Enter), `cancel` (Esc), else `null`. */
export function renameKey(key) {
  return RENAME_KEYS.get(key) ?? null;
}

/**
 * What the confirmation of a deletion says: the user's themes (and the
 * theme open now) that use the item's bytes, or that none does; and that
 * deleting cannot be undone. The themes keep their own copy.
 * @param {(k: string, p?: object) => string} t
 * @param {{themes?: string[], openTheme?: boolean}|null} users what `collected_users` answered
 * @param {string} locale
 */
export function deleteText(t, users, locale) {
  const names = (users?.themes ?? []).map((name) => `“${name}”`);
  if (users?.openTheme) names.push(t('collection.openTheme'));
  const list = new Intl.ListFormat(locale, { style: 'long', type: 'conjunction' }).format(names);
  const used = names.length ? t('collection.usedBy', { themes: list }) : t('collection.unused');
  return `${used} ${t('collection.cannotUndo')}`;
}

/**
 * The item that gets the focus once `id` leaves the list `ids`: the next
 * one, else the one before; `null` when none is left (or `id` is not there).
 * @param {string[]} ids
 * @param {string} id
 */
export function focusAfterRemoval(ids, id) {
  const at = ids.indexOf(id);
  if (at < 0) return null;
  return ids[at + 1] ?? ids[at - 1] ?? null;
}

/**
 * The collection as the panel shows it: loaded from the backend (newest
 * first), with its previews still or moving as `still()` says when it is
 * loaded. A load started later wins; a rename or a deletion made while a
 * load is on its way is applied to its answer too, which may not have it.
 * @param {object} deps
 * @param {(still: boolean) => Promise<object[]>} deps.load `gif_collection`
 * @param {() => boolean} [deps.still] whether previews are stills (motion reduced)
 */
export function createCollectionList({ load, still = () => false }) {
  let items = [];
  let status = 'idle';
  let error = null;
  let round = 0;
  // The changes made while a load was on its way (each can be applied twice).
  let edits = [];

  function edit(change) {
    items = change(items);
    if (status === 'loading') edits.push(change);
  }

  return {
    /** The items, newest first. */
    items: () => items,
    /** `idle` (never loaded), `loading`, `ready` or `error`. */
    status: () => status,
    /** Why the last load failed, or `null`. */
    error: () => error,
    /** The item `id`, or `null`. */
    find: (id) => items.find((item) => item.id === id) ?? null,
    /**
     * Loads the collection again; `true` once its answer (items or a
     * failure) is the one shown, `false` when a later load won.
     */
    async refresh() {
      round += 1;
      const mine = round;
      status = 'loading';
      try {
        const next = await load(still());
        if (mine !== round) return false;
        items = edits.reduce((list, change) => change(list), Array.isArray(next) ? next : []);
        status = 'ready';
        error = null;
      } catch (e) {
        if (mine !== round) return false;
        status = 'error';
        error = e;
      }
      edits = [];
      return true;
    },
    /** An item was renamed (`rename_collected`'s answer): only its name changes. */
    renamed(item) {
      edit((list) => list.map((known) => (known.id === item.id ? { ...known, name: item.name } : known)));
    },
    /** An item was deleted. */
    removed(id) {
      edit((list) => list.filter((known) => known.id !== id));
    },
  };
}

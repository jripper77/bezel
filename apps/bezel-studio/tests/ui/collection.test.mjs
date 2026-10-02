// The GIF and sticker collection's logic (D-2026-10-01-gif-sticker-search-5):
// the filter, the count and size, an item's facts, the names a rename
// takes, what deleting says of the themes using an item, where the focus
// goes once an item leaves, and the list kept in step with the backend.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { translator } from '../../src/i18n/index.js';
import {
  COLLECTION_FILTERS, cleanName, collectionCount, createCollectionList, deleteText, filterCollection, focusAfterRemoval, itemFacts, previewOf, providerName,
  renameKey, totalBytes,
} from '../../src/collection.js';

const pt = translator('pt-BR');
const en = translator('en');

const item = (id, kind, bytes, extra = {}) => ({
  id, name: `Item ${id}`, kind, width: 480, height: 270, bytes, addedAt: 1, source: { provider: 'klipy', id: `k-${id}`, url: null }, preview: null, ...extra,
});
const gif1 = item('a', 'gif', 1_000_000);
const sticker = item('b', 'sticker', 250_000, { width: 512, height: 512 });
const gif2 = item('c', 'gif', 500_000);
const all = [gif1, sticker, gif2];

/** A promise and the functions that settle it. */
function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((ok, no) => {
    resolve = ok;
    reject = no;
  });
  return { promise, resolve, reject };
}

test('the filter keeps every item, the GIFs or the stickers, in order', () => {
  assert.deepEqual(COLLECTION_FILTERS, ['all', 'gif', 'sticker']);
  assert.deepEqual(filterCollection(all, 'all'), all);
  assert.notEqual(filterCollection(all, 'all'), all, 'a copy, never the list itself');
  assert.deepEqual(filterCollection(all, 'gif').map((i) => i.id), ['a', 'c']);
  assert.deepEqual(filterCollection(all, 'sticker').map((i) => i.id), ['b']);
  assert.deepEqual(filterCollection(all, 'video'), all, 'an unknown filter keeps everything');
  assert.deepEqual(filterCollection([], 'gif'), []);
});

test('the count says how many items and how large, and of how many when filtered', () => {
  assert.equal(totalBytes([...all, { bytes: undefined }]), 1_750_000);
  assert.equal(collectionCount(en, [], [], 'en'), '');
  assert.equal(collectionCount(en, all, all, 'en'), '3 items · 1.8 MB');
  assert.equal(collectionCount(pt, all, all, 'pt-BR'), '3 itens · 1,8 MB');
  assert.equal(collectionCount(en, [gif1], [gif1], 'en'), '1 item · 1 MB');
  assert.equal(collectionCount(en, all, [sticker], 'en'), '1 of 3 items · 250 kB', 'the size of what is shown');
  assert.equal(collectionCount(pt, all, [], 'pt-BR'), '0 de 3 itens · 0 B');
});

test('an item says its kind, size in pixels, bytes and source', () => {
  assert.equal(itemFacts(en, gif1, 'en'), 'GIF · 480×270 · 1 MB · Source: KLIPY');
  assert.equal(itemFacts(pt, sticker, 'pt-BR'), 'Sticker · 512×512 · 250 kB · Fonte: KLIPY');
  assert.equal(itemFacts(en, { kind: 'gif', bytes: 10, source: { provider: 'other' } }, 'en'), 'GIF · 10 B · Source: other', 'no size without one');
  assert.equal(itemFacts(en, { kind: 'odd', width: 0, height: 5, bytes: 10 }, 'en'), 'GIF · 10 B', 'no source without one');
  assert.equal(providerName('klipy'), 'KLIPY');
  assert.equal(providerName(undefined), '');
});

test('a preview is shown only when it is a picture the backend sent', () => {
  assert.equal(previewOf({ preview: 'data:image/gif;base64,R0lG' }), 'data:image/gif;base64,R0lG');
  assert.equal(previewOf({ preview: 'https://static.klipy.com/x.gif' }), null);
  assert.equal(previewOf({ preview: 'data:text/html,<b>' }), null);
  assert.equal(previewOf({ preview: null }), null);
  assert.equal(previewOf(null), null);
});

test('a rename takes the name without spaces around it, never an empty one; Enter saves, Esc cancels', () => {
  assert.equal(cleanName('  Party time '), 'Party time');
  assert.equal(cleanName('   '), null);
  assert.equal(cleanName(''), null);
  assert.equal(cleanName(undefined), null);
  assert.equal(renameKey('Enter'), 'save');
  assert.equal(renameKey('Escape'), 'cancel');
  assert.equal(renameKey('a'), null);
  assert.equal(renameKey('Tab'), null);
});

test('deleting names the themes that use the item and the open one, and cannot be undone', () => {
  assert.equal(
    deleteText(en, { themes: ['Mine', 'Desk'], openTheme: true }, 'en'),
    'Used by “Mine”, “Desk”, and the theme open now. Each keeps its own copy and goes on working. Deleting cannot be undone.',
  );
  assert.equal(
    deleteText(pt, { themes: ['Meu'], openTheme: true }, 'pt-BR'),
    'Usado por “Meu” e o tema aberto agora. Cada um guarda a própria cópia e continua funcionando. Excluir não pode ser desfeito.',
  );
  assert.equal(deleteText(en, { themes: [], openTheme: true }, 'en'), 'Used by the theme open now. Each keeps its own copy and goes on working. Deleting cannot be undone.');
  assert.equal(deleteText(en, { themes: ['Mine'], openTheme: false }, 'en'), 'Used by “Mine”. Each keeps its own copy and goes on working. Deleting cannot be undone.');
  assert.equal(deleteText(pt, { themes: [], openTheme: false }, 'pt-BR'), 'Nenhum dos seus temas o usa. Excluir não pode ser desfeito.');
  assert.equal(deleteText(en, null, 'en'), 'No theme of yours uses it. Deleting cannot be undone.');
});

test('once an item leaves, the focus goes to the next one, else the one before', () => {
  assert.equal(focusAfterRemoval(['a', 'b', 'c'], 'a'), 'b');
  assert.equal(focusAfterRemoval(['a', 'b', 'c'], 'b'), 'c');
  assert.equal(focusAfterRemoval(['a', 'b', 'c'], 'c'), 'b');
  assert.equal(focusAfterRemoval(['a'], 'a'), null);
  assert.equal(focusAfterRemoval(['a', 'b'], 'x'), null);
});

test('the list loads stills or moving previews, and a later load wins', async () => {
  const asked = [];
  const answers = [deferred(), deferred()];
  let still = true;
  const list = createCollectionList({ load: (s) => { asked.push(s); return answers[asked.length - 1].promise; }, still: () => still });
  assert.equal(list.status(), 'idle');
  const first = list.refresh();
  assert.equal(list.status(), 'loading');
  still = false;
  const second = list.refresh();
  assert.deepEqual(asked, [true, false]);
  answers[1].resolve([gif1, sticker]);
  assert.equal(await second, true);
  answers[0].resolve([gif2]);
  assert.equal(await first, false, 'an earlier load answering late is dropped');
  assert.deepEqual(list.items().map((i) => i.id), ['a', 'b']);
  assert.equal(list.status(), 'ready');
  assert.equal(list.find('b'), sticker);
  assert.equal(list.find('z'), null);
});

test('a failed load keeps the items and says why; the next one clears it', async () => {
  const answers = [Promise.resolve([gif1]), Promise.reject(Object.assign(new Error('unreadable'), { code: 'io' })), Promise.resolve([gif1, gif2])];
  const list = createCollectionList({ load: () => answers.shift() });
  await list.refresh();
  assert.equal(await list.refresh(), true);
  assert.equal(list.status(), 'error');
  assert.equal(list.error().code, 'io');
  assert.deepEqual(list.items().map((i) => i.id), ['a']);
  await list.refresh();
  assert.equal(list.status(), 'ready');
  assert.equal(list.error(), null);
  assert.deepEqual(list.items().map((i) => i.id), ['a', 'c']);
});

test('renames and deletions apply in place, and to a load already on its way', async () => {
  const pending = deferred();
  const loads = [Promise.resolve(all), pending.promise, Promise.resolve([gif1, { ...sticker, name: 'Renamed elsewhere' }])];
  const list = createCollectionList({ load: () => loads.shift() });
  await list.refresh();
  list.renamed({ ...gif1, name: 'Party time', preview: 'data:image/png;base64,x' });
  assert.equal(list.find('a').name, 'Party time');
  assert.equal(list.find('a').preview, null, 'only the name changes');
  list.removed('c');
  assert.deepEqual(list.items().map((i) => i.id), ['a', 'b']);
  // A load started before the next changes answers without them.
  const loading = list.refresh();
  list.renamed({ ...sticker, name: 'Star' });
  list.removed('a');
  pending.resolve([{ ...gif1, name: 'Party time' }, sticker, item('d', 'gif', 1)]);
  assert.equal(await loading, true);
  assert.deepEqual(list.items().map((i) => [i.id, i.name]), [['b', 'Star'], ['d', 'Item d']]);
  // Settled, the changes are not applied again to the next answer.
  await list.refresh();
  assert.deepEqual(list.items().map((i) => [i.id, i.name]), [['a', 'Item a'], ['b', 'Renamed elsewhere']]);
});

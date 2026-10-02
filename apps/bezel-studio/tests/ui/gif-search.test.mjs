// The GIF and sticker search's logic (D-2026-10-01-gif-sticker-search-4):
// typing waits for a pause, Enter does not; the result grid's keys; "Load
// more" appends only results not shown yet and asks only their previews,
// a few at a time; a 429 explains the 100 requests per hour and offers the
// Partner Panel, keeping the results shown.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { translator } from '../../src/i18n/index.js';
import {
  MIN_SEARCH_CHARS, PARTNER_PANEL, SEARCH_DELAY_MS, createGifResults, createSearchTrigger, gridMove, keyFailure, keyStatus, mergeResults, queryOf,
  resultsText, searchFailure,
} from '../../src/gif-search.js';

const pt = translator('pt-BR');
const en = translator('en');

/** A fake clock whose timers run when it is advanced. */
function fakeClock() {
  const clock = { now: 0, timers: new Map(), next: 1 };
  clock.wait = (ms, fn) => {
    const id = clock.next++;
    clock.timers.set(id, { at: clock.now + ms, fn });
    return id;
  };
  clock.cancel = (id) => clock.timers.delete(id);
  clock.advance = (ms) => {
    clock.now += ms;
    for (const [id, timer] of [...clock.timers]) {
      if (timer.at > clock.now) continue;
      clock.timers.delete(id);
      timer.fn();
    }
  };
  return clock;
}

/** Lets pending promise callbacks run. */
const settle = () => new Promise((resolve) => { setImmediate(resolve); });

/** A promise and the functions that settle it. */
function deferred() {
  const d = {};
  d.promise = new Promise((resolve, reject) => Object.assign(d, { resolve, reject }));
  return d;
}

const item = (id) => ({ id, title: id, width: 480, height: 270 });

test('debounce and Enter', () => {
  assert.equal(SEARCH_DELAY_MS, 600);
  assert.equal(MIN_SEARCH_CHARS, 2);
  const clock = fakeClock();
  const searched = [];
  const trigger = createSearchTrigger({ search: (text) => searched.push([clock.now, text]), wait: clock.wait, cancel: clock.cancel });

  trigger.typed('c');
  clock.advance(5000);
  assert.deepEqual(searched, [], 'one character never searches by typing');

  trigger.typed('ca');
  clock.advance(400);
  trigger.typed('cat ');
  clock.advance(599);
  assert.deepEqual(searched, [], 'each key starts the pause again');
  assert.equal(trigger.waiting(), true);
  clock.advance(1);
  assert.deepEqual(searched, [[6000, 'cat']], 'after 600 ms without a key, trimmed');
  assert.equal(trigger.waiting(), false);

  trigger.typed('dogs');
  clock.advance(100);
  trigger.submit(' dogs ');
  assert.deepEqual(searched.at(-1), [6100, 'dogs'], 'Enter searches at once');
  clock.advance(SEARCH_DELAY_MS);
  assert.equal(searched.length, 2, 'and drops the search waiting for the pause');

  trigger.submit('');
  assert.deepEqual(searched.at(-1), [6700, ''], 'Enter on an empty field: trending');
  trigger.submit('x');
  assert.deepEqual(searched.at(-1), [6700, 'x'], 'Enter searches even one character');

  trigger.typed('\u{1F600}');
  clock.advance(SEARCH_DELAY_MS);
  assert.equal(searched.length, 4, 'an emoji is one character');
  trigger.typed('\u{1F600}\u{1F431}');
  trigger.cancel();
  clock.advance(SEARCH_DELAY_MS);
  assert.equal(searched.length, 4, 'cancelled (the dialog closed)');
  trigger.typed('ok');
  trigger.typed('o');
  clock.advance(SEARCH_DELAY_MS);
  assert.equal(searched.length, 4, 'erasing below 2 characters drops the waiting search');

  // What a search asks: page 1, trimmed, explicit results only when chosen.
  assert.deepEqual(queryOf({ kind: 'sticker', text: ' cat ' }), { kind: 'sticker', text: 'cat', page: 1, explicit: false });
  assert.deepEqual(queryOf({ kind: 'video', text: null, explicit: 1 }), { kind: 'gif', text: '', page: 1, explicit: true });
});

test('grid arrows', () => {
  // 10 results in rows of 4: 0 1 2 3 / 4 5 6 7 / 8 9
  const move = (from, key) => gridMove(from, key, 10, 4);
  assert.equal(move(0, 'ArrowRight'), 1);
  assert.equal(move(3, 'ArrowRight'), 4, 'right goes on to the next row');
  assert.equal(move(9, 'ArrowRight'), 9, 'the last stays');
  assert.equal(move(4, 'ArrowLeft'), 3);
  assert.equal(move(0, 'ArrowLeft'), 0, 'the first stays');
  assert.equal(move(1, 'ArrowDown'), 5, 'down a row, same column');
  assert.equal(move(5, 'ArrowDown'), 9);
  assert.equal(move(6, 'ArrowDown'), 6, 'no result below: stays');
  assert.equal(move(9, 'ArrowUp'), 5, 'up a row');
  assert.equal(move(2, 'ArrowUp'), 2, 'the first row stays');
  assert.equal(move(6, 'Home'), 0);
  assert.equal(move(2, 'End'), 9);
  assert.equal(gridMove(0, 'ArrowDown', 3, 1), 1, 'one column: down is the next');
  assert.equal(gridMove(1, 'ArrowDown', 3, 0), 2, 'no layout yet: one column');
  assert.equal(gridMove(40, 'ArrowLeft', 10, 4), 8, 'a stale index comes back into the grid');
  for (const key of ['Enter', ' ', 'Tab', 'a', 'constructor', 'toString']) assert.equal(move(5, key), null, key);
  assert.equal(gridMove(0, 'ArrowRight', 0, 4), null, 'an empty grid');
});

test('load more dedupes', async () => {
  assert.deepEqual(mergeResults([item('a')], [item('a'), item('b'), item('b')]), { items: [item('a'), item('b')], added: [item('b')] });
  const pages = {
    1: { page: 1, hasNext: true, items: ['a', 'b', 'c'].map(item) },
    2: { page: 2, hasNext: true, items: ['b', 'c', 'd'].map(item) },
    3: { page: 3, hasNext: false, items: ['d', 'e'].map(item) },
  };
  const asked = [];
  const previews = [];
  const changes = [];
  const painted = [];
  let still = true;
  const results = createGifResults({
    search: async (query) => {
      asked.push(query);
      return structuredClone(pages[query.page]);
    },
    preview: (id, still) => {
      const d = deferred();
      previews.push({ id, still, ...d });
      return d.promise;
    },
    still: () => still,
    onChange: (view, reason) => changes.push([reason, view.items.map((i) => i.id).join(''), view.added]),
    onPreview: (id, url) => painted.push([id, url]),
    atOnce: 2,
  });
  const query = { kind: 'sticker', text: 'cat', explicit: true };

  await results.search({ ...query, page: 7 });
  assert.deepEqual(asked, [{ ...query, page: 1 }], 'a search starts at page 1');
  assert.deepEqual(changes, [['loading', '', 0], ['new', 'abc', 3]]);
  await settle();
  assert.deepEqual(previews.map((p) => [p.id, p.still]), [['a', true], ['b', true]], 'at most 2 previews at once, stills');
  previews[0].resolve('data:image/gif;base64,AA');
  await settle();
  assert.deepEqual(previews.map((p) => p.id), ['a', 'b', 'c'], 'the next one once a slot frees');

  await results.more();
  assert.deepEqual(asked.at(-1), { ...query, page: 2 }, 'the same search, next page');
  assert.equal(results.view().items.map((i) => i.id).join(''), 'abcd', 'b and c are not shown twice');
  assert.deepEqual(changes.at(-1), ['more', 'abcd', 1]);
  previews[1].resolve(null);
  previews[2].resolve('data:image/gif;base64,AA');
  await settle();
  assert.deepEqual(previews.map((p) => p.id), ['a', 'b', 'c', 'd'], 'only the new result\'s preview');

  assert.deepEqual(painted, [['a', 'data:image/gif;base64,AA'], ['b', null], ['c', 'data:image/gif;base64,AA']]);
  assert.equal(resultsText(en, { count: 1, text: '', more: true }), '1 more results.');
  assert.equal(resultsText(pt, { count: 0, text: '', more: true }), 'Nenhum resultado novo nesta página.');

  await results.more();
  assert.equal(results.view().items.map((i) => i.id).join(''), 'abcde');
  assert.equal(results.view().hasNext, false);
  await results.more();
  assert.equal(asked.length, 3, 'nothing past the last page');

  // Motion allowed again: every preview is asked again, moving; the dialog
  // closed: nothing answered reaches it.
  await settle();
  for (let round = 0; round < 2; round += 1) {
    for (const p of previews.splice(0)) p.resolve(null);
    await settle();
  }
  still = false;
  results.refreshPreviews();
  await settle();
  assert.deepEqual(previews.map((p) => [p.id, p.still]), [['a', false], ['b', false]]);
  results.close();
  const before = painted.length;
  for (const p of previews) p.resolve('data:image/gif;base64,BB');
  await settle();
  assert.equal(painted.length, before, 'closed: dropped');

  // A later search wins: the answer of the one before is dropped.
  const slow = deferred();
  const late = createGifResults({ search: (q) => (q.text === 'slow' ? slow.promise : Promise.resolve(pages[1])), preview: async () => null });
  const first = late.search({ ...query, text: 'slow' });
  await late.search({ ...query, text: 'fast' });
  slow.resolve(pages[3]);
  await first;
  assert.equal(late.view().items.map((i) => i.id).join(''), 'abc');
  assert.equal(late.view().query.text, 'fast');
});

test('429 message', async () => {
  const limited = { code: 'klipyRateLimited', args: {}, message: 'KLIPY answered 429' };
  for (const [t, panel] of [[en, 'Partner Panel'], [pt, 'Painel de Parceiros']]) {
    const failure = searchFailure(t, limited);
    assert.equal(failure.action, 'partnerPanel');
    assert.match(failure.text, /100/, 'test keys allow 100 requests per hour');
    assert.ok(failure.text.includes(panel), failure.text);
    assert.match(failure.detail, /Request Production/, 'how to ask for production');
    assert.equal(t('gifs.partnerPanel'), en === t ? 'Open the Partner Panel' : 'Abrir o Painel de Parceiros');
  }
  assert.equal(PARTNER_PANEL, 'klipyPartnerPanel');
  assert.deepEqual(searchFailure(en, { code: 'klipyKeyRejected', args: {} }), { text: en('error.klipyKeyRejected'), detail: null, action: 'help' });
  assert.equal(searchFailure(en, { code: 'klipyNoKey', args: {} }).action, 'help');
  assert.deepEqual(searchFailure(en, { code: 'klipyUnavailable', args: { detail: 'timed out' } }), { text: en('error.klipyUnavailable', { detail: 'timed out' }), detail: null, action: 'retry' });
  assert.equal(searchFailure(en, new Error('boom')).text, en('error.unknown', { message: 'boom' }));

  // What the key field says: the last 4 characters only, and which characters a key takes.
  assert.equal(keyStatus(en, { configured: true, last4: 'a1b2' }), 'Saved key ending in a1b2');
  assert.equal(keyStatus(pt, { configured: true, last4: 'a1b2' }), 'Chave salva, termina em a1b2');
  assert.equal(keyStatus(en, { configured: false, last4: null }), en('gifs.keyNone'));
  assert.equal(keyStatus(en, null), en('gifs.keyNone'));
  assert.equal(keyFailure(en, { code: 'invalidInput', args: { detail: 'key' } }), en('gifs.keyInvalid'));
  assert.equal(keyFailure(pt, { code: 'busy', args: {} }), pt('error.busy'));
  assert.equal(resultsText(en, { count: 24, text: 'cat' }), '24 results for “cat”.');
  assert.equal(resultsText(pt, { count: 1, text: 'gato' }), '1 resultado para “gato”.');
  assert.equal(resultsText(en, { count: 24, text: '' }), '24 trending results.');
  assert.equal(resultsText(en, { count: 0, text: 'zzz' }), en('gifs.noResults', { text: 'zzz' }));
  assert.equal(resultsText(en, { count: 0, text: '' }), en('gifs.noTrending'));

  // The results already shown stay; the same page is asked again on retry.
  let calls = 0;
  const results = createGifResults({
    search: async (query) => {
      calls += 1;
      if (calls === 2) throw limited;
      return { page: query.page, hasNext: true, items: [item(`p${query.page}`)] };
    },
    preview: async () => null,
  });
  await results.search({ kind: 'gif', text: '', explicit: false });
  await results.more();
  assert.equal(results.view().status, 'error');
  assert.equal(results.view().error, limited);
  assert.equal(results.view().items.map((i) => i.id).join(), 'p1', 'the results shown stay');
  await results.retry();
  assert.equal(results.view().status, 'ready');
  assert.equal(results.view().items.map((i) => i.id).join(), 'p1,p2', 'the retry asked page 2 again');
  await results.retry();
  assert.equal(calls, 3, 'nothing to retry once it worked');
});

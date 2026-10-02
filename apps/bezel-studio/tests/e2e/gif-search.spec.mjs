// Searching GIFs and stickers on KLIPY (D-2026-10-01-gif-sticker-search-3,
// -4, -6), in demo mode, in pt-BR and en, light and dark: the dialog opens
// from the Media tab's Collection and asks nothing of KLIPY; without a key
// searching waits for one and the "?" discloses how to get it (Esc and a
// click outside close it, the focus goes back), the Partner Panel and the
// guide open through the backend; a refused key opens the help; explicit
// results are off at every start and kept while the app runs; typing
// searches only after a 600 ms pause from the last key, with 2 characters or
// more, and Enter at once (on the page's clock, which the test holds); a 429
// explains the 100 requests per hour with the Partner Panel. The collection
// (D-2026-10-01-gif-sticker-search-5) lists what was
// added, filters it, puts an item on the canvas (in the middle, or where its
// preview is dropped) or makes it the background, in vertical and horizontal
// themes alike, renames it in place and deletes it after a confirmation that
// names the themes using it; its previews are stills while motion is
// reduced. No console errors, no serious or critical accessibility
// violations.
import { test, expect, watchErrors, expectAccessible, dragTo } from './helpers.mjs';

const root = (page) => page.locator('html');
/** Where the demo writes each query its KLIPY is asked (the bridge's `onGifQuery`). */
const QUERY_ATTRIBUTE = 'data-demo-gif-query';
/** The pause after the last key before typing searches (D-2026-10-01-gif-sticker-search-4), ms. */
const PAUSE_MS = 600;
/** The last query the demo's KLIPY was asked (`null`: none). */
const lastQuery = async (page) => JSON.parse((await root(page).getAttribute(QUERY_ATTRIBUTE)) ?? 'null');

/**
 * Watches every query the demo's KLIPY is asked from now on, not only the
 * last: each one is a write of the query attribute, which a mutation
 * observer records with the value it replaced. So each write's value is the
 * one the next write replaced, and the last one's is the attribute now.
 * @returns {Promise<() => Promise<object[]>>} the queries asked so far, in order
 */
async function watchQueries(page) {
  await page.evaluate((name) => {
    const html = document.documentElement;
    const replaced = [];
    const keep = (records) => replaced.push(...records.map((record) => record.oldValue));
    const observer = new MutationObserver(keep);
    observer.observe(html, { attributeFilter: [name], attributeOldValue: true });
    globalThis.demoGifQueries = () => {
      keep(observer.takeRecords());
      return replaced.length ? [...replaced.slice(1), html.getAttribute(name)] : [];
    };
  }, QUERY_ATTRIBUTE);
  return async () => (await page.evaluate(() => globalThis.demoGifQueries())).map((value) => JSON.parse(value));
}

async function openSearch(page, t, scenario) {
  await page.goto(`/index.html?demo=${scenario}`);
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: t('library.media') }).click();
  await expect(page.getByRole('tab', { name: t('media.thisTheme') })).toHaveAttribute('aria-selected', 'true');
  await page.getByRole('tab', { name: t('media.collection') }).click();
  await page.getByRole('button', { name: t('media.searchGifs') }).click();
  const dialog = page.getByRole('dialog', { name: t('gifs.title') });
  await expect(dialog).toBeVisible();
  return dialog;
}

test.describe('gif search', () => {
  test('no key: help', async ({ page, t, lang }) => {
    const errors = watchErrors(page);
    const dialog = await openSearch(page, t, 'gifsNoKey');
    const key = dialog.getByLabel(t('gifs.key'));
    await expect(key).toBeFocused();
    await expect(key).toHaveAttribute('type', 'password');
    await expect(key).toHaveAttribute('autocomplete', 'off');
    await expect(dialog.getByText(t('gifs.keyNone'))).toBeVisible();
    const field = dialog.getByRole('searchbox', { name: 'Search KLIPY' });
    await expect(field).toHaveAttribute('placeholder', 'Search KLIPY');
    await expect(field).toBeDisabled();
    await expect(dialog.getByText(t('gifs.needKey'))).toBeVisible();
    await expect(dialog.getByRole('button', { name: t('gifs.trending') })).toBeDisabled();
    await expect(dialog.getByText('Powered by KLIPY')).toBeVisible();

    // The "?" discloses the 4 steps and the two buttons.
    const help = dialog.getByRole('button', { name: t('gifs.help') });
    const steps = dialog.getByRole('group', { name: t('gifs.helpTitle') });
    await expect(help).toHaveAttribute('aria-expanded', 'false');
    await expect(steps).toBeHidden();
    await help.click();
    await expect(help).toHaveAttribute('aria-expanded', 'true');
    await expect(help).toHaveAttribute('aria-controls', 'gif-key-help');
    await expect(steps).toBeVisible();
    await expect(steps).toBeFocused();
    await expect(steps.getByRole('listitem')).toHaveText(['gifs.helpStep1', 'gifs.helpStep2', 'gifs.helpStep3', 'gifs.helpStep4'].map((k) => t(k)));
    await expect(steps).toContainText('100');
    await expectAccessible(page);
    await steps.getByRole('button', { name: t('gifs.partnerPanel') }).click();
    await expect(root(page)).toHaveAttribute('data-demo-link', 'klipyPartnerPanel');
    await steps.getByRole('button', { name: t('gifs.guide') }).click();
    await expect(root(page)).toHaveAttribute('data-demo-guide', `gifs-and-stickers ${lang}`);

    // Esc closes the help, not the dialog, and the focus goes back to "?".
    await page.keyboard.press('Escape');
    await expect(steps).toBeHidden();
    await expect(help).toHaveAttribute('aria-expanded', 'false');
    await expect(help).toBeFocused();
    await expect(dialog).toBeVisible();
    // A click outside closes it too.
    await help.click();
    await expect(steps).toBeVisible();
    await dialog.getByRole('heading', { name: t('gifs.title') }).click();
    await expect(steps).toBeHidden();
    await expect(help).toBeFocused();

    // Saving with the field empty says so on the field, in the UI's language.
    await expect(key).toHaveValue('');
    await dialog.getByRole('button', { name: t('gifs.keySave') }).click();
    await expect(dialog.getByText(t('gifs.keyEmpty'), { exact: true })).toBeVisible();
    await expect(key).toHaveAttribute('aria-invalid', 'true');
    await expect(key).toHaveAttribute('aria-describedby', /\bgif-key-error\b/);
    await expect(dialog.locator('#gif-key-error')).toHaveText(t('gifs.keyEmpty'));
    expect(await lastQuery(page)).toBeNull();

    // A key that is not one; then a key KLIPY refuses at the first search, which opens the help.
    await key.fill('not a key!');
    await dialog.getByRole('button', { name: t('gifs.keySave') }).click();
    await expect(dialog.getByText(t('gifs.keyInvalid'))).toBeVisible();
    await expect(key).toHaveAttribute('aria-invalid', 'true');
    await key.fill('refused-key');
    await key.press('Enter');
    await expect(dialog.getByText(t('gifs.keySaved', { last4: '-key' }))).toBeVisible();
    await expect(key).toHaveValue('');
    await expect(dialog.getByText(t('gifs.keyInvalid'))).toBeHidden();
    expect(await lastQuery(page)).toBeNull();
    await expect(field).toBeEnabled();
    await field.fill('cat');
    await field.press('Enter');
    await expect(dialog.getByRole('alert')).toContainText(t('error.klipyKeyRejected'));
    await expect(steps).toBeVisible();
    await expect(help).toHaveAttribute('aria-expanded', 'true');
    await expectAccessible(page);

    // Remove: searching waits for a key again.
    await page.keyboard.press('Escape');
    await dialog.getByRole('button', { name: t('gifs.keyRemove') }).click();
    await expect(dialog.getByText(t('gifs.keyNone'))).toBeVisible();
    await expect(field).toBeDisabled();
    await expect(key).toBeFocused();

    // Esc now closes the dialog, and the focus goes back to its button.
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(page.getByRole('button', { name: t('media.searchGifs') })).toBeFocused();
    expect(errors).toEqual([]);
  });

  test('explicit off by default', async ({ page, t }) => {
    const errors = watchErrors(page);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    // The page's clock is Playwright's: it runs as usual until the test holds it.
    await page.clock.install();
    const dialog = await openSearch(page, t, 'gifs');
    await expect(dialog.getByText(t('gifs.keySaved', { last4: 'a1b2' }))).toBeVisible();
    const field = dialog.getByRole('searchbox', { name: 'Search KLIPY' });
    await expect(field).toBeFocused();
    const explicit = dialog.getByRole('switch', { name: t('gifs.explicit') });
    await expect(explicit).not.toBeChecked();
    await expect(dialog.getByText(t('gifs.start'))).toBeVisible();
    await expectAccessible(page);
    expect(await lastQuery(page)).toBeNull();

    // Typing searches only after a 600 ms pause from the last key, with 2
    // characters or more; Enter at once. Every query KLIPY is asked is
    // recorded while the test holds the clock and moves it by hand.
    const asked = await watchQueries(page);
    const gifQuery = (text) => ({ kind: 'gif', text, page: 1, explicit: false });
    await page.clock.pauseAt((await page.evaluate(() => Date.now())) + PAUSE_MS);
    // One character: no search, however long the pause.
    await field.press('c');
    await page.clock.runFor(PAUSE_MS * 2);
    expect(await asked()).toEqual([]);
    // "ca", then quickly "t": nothing before the pause after "t", then one search, for "cat".
    await field.clear();
    await field.pressSequentially('ca');
    await page.clock.runFor(PAUSE_MS / 2);
    await field.press('t');
    await page.clock.runFor(PAUSE_MS - 1);
    expect(await asked()).toEqual([]);
    await page.clock.runFor(1);
    await expect.poll(asked).toEqual([gifQuery('cat')]);
    await page.clock.runFor(PAUSE_MS * 2);
    expect(await asked()).toEqual([gifQuery('cat')]);
    // Enter: at once, with the clock still held, and no search after the pause either.
    await field.fill('wave');
    await field.press('Enter');
    await expect.poll(asked).toEqual([gifQuery('cat'), gifQuery('wave')]);
    await page.clock.runFor(PAUSE_MS * 2);
    expect(await asked()).toEqual([gifQuery('cat'), gifQuery('wave')]);
    await field.clear();
    await page.clock.resume();

    // Typing searches after a pause: 24 results, stills while motion is reduced.
    await field.pressSequentially('ca');
    await expect.poll(() => lastQuery(page)).toEqual({ kind: 'gif', text: 'ca', page: 1, explicit: false });
    expect(await asked()).toEqual([gifQuery('cat'), gifQuery('wave'), gifQuery('ca')]);
    const grid = dialog.getByRole('list', { name: t('gifs.results') });
    const tiles = grid.getByRole('button');
    await expect(tiles).toHaveCount(24);
    await expect(dialog.locator('[aria-live="polite"]')).toHaveText(t('gifs.announce', { count: 24, text: 'ca' }));
    const preview = grid.locator('img').first();
    await expect(preview).toHaveAttribute('src', /^data:image\/svg/);
    expect(await preview.getAttribute('src')).not.toContain('animate');
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await expect(preview).toHaveAttribute('src', /animate/);
    await expectAccessible(page);

    // Explicit results on: page 1 again, unfiltered; then stickers.
    await dialog.getByText(t('gifs.explicit')).click();
    await expect(explicit).toBeChecked();
    await expect.poll(() => lastQuery(page)).toEqual({ kind: 'gif', text: 'ca', page: 1, explicit: true });
    const stickers = dialog.getByRole('button', { name: t('gifs.kind.sticker') });
    await stickers.click();
    await expect(stickers).toHaveAttribute('aria-pressed', 'true');
    await expect.poll(() => lastQuery(page)).toEqual({ kind: 'sticker', text: 'ca', page: 1, explicit: true });
    await expect(tiles.first()).toContainText('Star');
    await dialog.getByRole('button', { name: t('gifs.loadMore') }).click();
    await expect.poll(() => lastQuery(page)).toEqual({ kind: 'sticker', text: 'ca', page: 2, explicit: true });
    await expect(tiles).toHaveCount(46);

    // The keyboard: Left and Right move by one result, Up and Down by a row
    // of the grid as laid out (its columns, counted here on the page),
    // staying put at the first and the last rows; Home, End; Enter adds.
    const tops = await grid.getByRole('listitem').evaluateAll((items) => items.map((item) => Math.round(item.getBoundingClientRect().top)));
    const columns = tops.filter((top) => top === tops[0]).length;
    const last = tops.length - 1;
    expect(tops).toHaveLength(46);
    expect(columns).toBeGreaterThan(1);
    expect(columns).toBeLessThan(tops.length);
    await tiles.first().focus();
    await page.keyboard.press('ArrowUp');
    await expect(tiles.first()).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await expect(tiles.nth(1)).toBeFocused();
    await page.keyboard.press('ArrowDown');
    await expect(tiles.nth(1 + columns)).toBeFocused();
    await page.keyboard.press('ArrowLeft');
    await expect(tiles.nth(columns)).toBeFocused();
    await page.keyboard.press('ArrowUp');
    await expect(tiles.first()).toBeFocused();
    await page.keyboard.press('End');
    await expect(tiles.last()).toBeFocused();
    await page.keyboard.press('ArrowDown');
    await expect(tiles.last()).toBeFocused();
    await page.keyboard.press('ArrowUp');
    await expect(tiles.nth(last - columns)).toBeFocused();
    await page.keyboard.press('ArrowDown');
    await expect(tiles.nth(last)).toBeFocused();
    await page.keyboard.press('Home');
    await expect(tiles.first()).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('Enter');
    await expect(dialog.locator('[aria-live="polite"]')).toHaveText(t('gifs.added', { name: 'Heart' }));
    await expect(tiles.nth(1)).toContainText(t('gifs.inCollection'));

    // Kept while the app runs; nothing asked when the dialog opens again.
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await page.getByRole('button', { name: t('media.searchGifs') }).click();
    await expect(explicit).toBeChecked();
    await expect(stickers).toHaveAttribute('aria-pressed', 'true');
    expect(await lastQuery(page)).toEqual({ kind: 'sticker', text: 'ca', page: 2, explicit: true });
    // Off again at the next start.
    await openSearch(page, t, 'gifs');
    await expect(explicit).not.toBeChecked();
    expect(errors).toEqual([]);
  });

  test('429: 100 per hour', async ({ page, t }) => {
    const errors = watchErrors(page);
    const dialog = await openSearch(page, t, 'gifsRateLimited');
    await dialog.getByRole('button', { name: t('gifs.trending') }).click();
    await expect.poll(() => lastQuery(page)).toEqual({ kind: 'gif', text: '', page: 1, explicit: false });
    const alert = dialog.getByRole('alert');
    await expect(alert).toContainText(t('error.klipyRateLimited'));
    await expect(alert).toContainText('100');
    await expect(alert).toContainText(t('gifs.rateLimitedHow'));
    await expect(dialog.getByRole('group', { name: t('gifs.helpTitle') })).toBeHidden();
    await expectAccessible(page);
    await alert.getByRole('button', { name: t('gifs.partnerPanel') }).click();
    await expect(root(page)).toHaveAttribute('data-demo-link', 'klipyPartnerPanel');
    expect(errors).toEqual([]);
  });
});


/** Opens the Media tab's Collection. */
async function openCollection(page, t, scenario) {
  await page.goto(`/index.html?demo=${scenario}`);
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: t('library.media') }).click();
  await page.getByRole('tab', { name: t('media.collection') }).click();
  return page.locator('#collection-panel');
}

/** Adds the results at `picks` of a search for `text` to the collection, then closes the dialog. */
async function collect(page, t, { kind, text, picks }) {
  await page.getByRole('button', { name: t('media.searchGifs') }).click();
  const dialog = page.getByRole('dialog', { name: t('gifs.title') });
  await dialog.getByRole('button', { name: t(`gifs.kind.${kind}`), exact: true }).click();
  const field = dialog.getByRole('searchbox', { name: 'Search KLIPY' });
  await field.fill(text);
  await field.press('Enter');
  const tiles = dialog.getByRole('list', { name: t('gifs.results') }).getByRole('button');
  await expect(tiles).toHaveCount(24);
  for (const n of picks) {
    await tiles.nth(n).click();
    await expect(tiles.nth(n)).toContainText(t('gifs.inCollection'));
  }
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
}

/** The items the collection lists. */
const itemsOf = (panel, t) => panel.getByRole('list', { name: t('collection.list') }).getByRole('listitem');

/** The selected element's center, from the inspector, is `at` (canvas pixels, give or take a pointer's step). */
async function expectCenter(page, t, at) {
  const inspector = page.locator('#inspector');
  const value = async (key) => Number(await inspector.getByRole('spinbutton', { name: t(key), exact: true }).inputValue());
  await expect(async () => {
    const [x, y, width, height] = [await value('inspector.x'), await value('inspector.y'), await value('inspector.width'), await value('inspector.height')];
    expect(Math.abs(x + width / 2 - at.x)).toBeLessThanOrEqual(6);
    expect(Math.abs(y + height / 2 - at.y)).toBeLessThanOrEqual(6);
  }).toPass();
}

test.describe('gif collection', () => {
  test('add, rename, delete', async ({ page, t }) => {
    const errors = watchErrors(page);
    const panel = await openCollection(page, t, 'gifs');
    // Empty: it points to the search.
    await expect(panel.getByText(t('collection.empty'))).toBeVisible();
    await expect(panel.getByText(t('collection.emptyHint'))).toBeVisible();
    await expect(panel.getByRole('list', { name: t('collection.list') })).toBeHidden();
    await expectAccessible(page);

    // What the search adds shows up, newest first, with its facts.
    await collect(page, t, { kind: 'gif', text: 'wave', picks: [0, 1] });
    await collect(page, t, { kind: 'sticker', text: 'star', picks: [0] });
    const items = itemsOf(panel, t);
    await expect(items).toHaveCount(3);
    await expect(items.nth(0)).toContainText('Star');
    await expect(items.nth(1)).toContainText('Thumbs up');
    await expect(items.nth(2)).toContainText('Happy dance');
    const happy = items.filter({ hasText: 'Happy dance' });
    await expect(happy).toContainText(`${t('collection.kind.gif')} · 480×270 · `);
    await expect(happy).toContainText(t('collection.source', { provider: 'KLIPY' }));
    await expect(items.nth(0)).toContainText(`${t('collection.kind.sticker')} · 512×512 · `);
    await expect(happy.locator('img')).toHaveAttribute('src', /^data:image\/svg/);
    await expect(happy.getByRole('button', { name: t('collection.addImage') })).toHaveAccessibleDescription('Happy dance');
    const count = panel.locator('.collection-count');
    await expect(count).toContainText(t('collection.count', { count: 3, size: '' }).trim());

    // The filter: stickers, GIFs, all.
    const filters = panel.getByRole('group', { name: t('collection.filter') });
    const stickers = filters.getByRole('button', { name: t('collection.filter.sticker') });
    await stickers.click();
    await expect(stickers).toHaveAttribute('aria-pressed', 'true');
    await expect(items).toHaveCount(1);
    await expect(count).toContainText(t('collection.countOf', { shown: 1, count: 3, size: '' }).trim());
    await filters.getByRole('button', { name: t('collection.filter.gif') }).click();
    await expect(items).toHaveCount(2);
    await expect(items.first()).toContainText('Thumbs up');
    await filters.getByRole('button', { name: t('collection.filter.all') }).click();
    await expect(items).toHaveCount(3);
    await expectAccessible(page);

    // Add as image, from the keyboard: an image element in the middle (the Demo theme is
    // vertical, 480x1920), named after the item.
    await happy.getByRole('button', { name: t('collection.addImage') }).press('Enter');
    const status = panel.getByRole('status');
    await expect(status).toHaveText(t('collection.addedImage', { name: 'Happy dance' }));
    const inspector = page.locator('#inspector');
    await expect(inspector.getByRole('combobox', { name: t('inspector.asset'), exact: true })).toHaveValue('assets/happy-dance.gif');
    await expect(inspector.getByRole('textbox', { name: t('inspector.name'), exact: true })).toHaveValue('Happy dance');
    await expectCenter(page, t, { x: 240, y: 960 });

    // Rename in place: Esc keeps the name, an empty one is refused, Enter saves.
    const rename = happy.getByRole('button', { name: t('collection.rename') });
    await rename.click();
    const field = panel.getByRole('textbox', { name: t('collection.nameField', { name: 'Happy dance' }) });
    await expect(field).toBeFocused();
    await expect(field).toHaveValue('Happy dance');
    await field.fill('Not this');
    await field.press('Escape');
    await expect(field).toBeHidden();
    await expect(rename).toBeFocused();
    await expect(items.nth(2)).toContainText('Happy dance');
    await rename.press('Enter');
    await expect(field).toBeFocused();
    await field.fill('   ');
    await field.press('Enter');
    await expect(panel.getByText(t('collection.nameEmpty'))).toBeVisible();
    await expect(field).toHaveAttribute('aria-invalid', 'true');
    await expect(field).toBeFocused();
    await expectAccessible(page);
    await field.fill(' Party time ');
    await field.press('Enter');
    await expect(status).toHaveText(t('collection.renamed', { old: 'Happy dance', name: 'Party time' }));
    const party = items.filter({ hasText: 'Party time' });
    await expect(party.getByRole('button', { name: t('collection.rename') })).toBeFocused();
    await expect(items.nth(2)).toContainText('Party time');

    // Delete asks first; Cancel keeps the item, and the focus goes back.
    const thumbs = items.filter({ hasText: 'Thumbs up' });
    await thumbs.getByRole('button', { name: t('collection.delete') }).click();
    let confirm = page.getByRole('dialog', { name: t('collection.deleteTitle', { name: 'Thumbs up' }) });
    await expect(confirm).toContainText(t('collection.unused'));
    await expect(confirm).toContainText(t('collection.cannotUndo'));
    await expect(confirm.getByRole('button', { name: t('dialog.cancel') })).toBeFocused();
    await confirm.getByRole('button', { name: t('dialog.cancel') }).click();
    await expect(confirm).toBeHidden();
    await expect(items).toHaveCount(3);
    await expect(thumbs.getByRole('button', { name: t('collection.delete') })).toBeFocused();
    // Confirmed: it goes, and the focus moves to the next item.
    await page.keyboard.press('Enter');
    await confirm.getByRole('button', { name: t('collection.deleteAction'), exact: true }).click();
    await expect(items).toHaveCount(2);
    await expect(status).toHaveText(t('collection.deleted', { name: 'Thumbs up' }));
    await expect(party.getByRole('button', { name: t('collection.delete') })).toBeFocused();

    // An item a saved theme and the open one use: the dialog names them; they keep their copy.
    await page.locator('#theme-name').fill('Mine');
    await page.locator('#theme-name').press('Enter');
    await page.locator('#save').click();
    await expect(page.locator('#toast')).toHaveText(t('toast.saved'));
    await party.getByRole('button', { name: t('collection.delete') }).click();
    confirm = page.getByRole('dialog', { name: t('collection.deleteTitle', { name: 'Party time' }) });
    await expect(confirm).toContainText('“Mine”');
    await expect(confirm).toContainText(t('collection.openTheme'));
    await expectAccessible(page);
    await confirm.getByRole('button', { name: t('collection.deleteAction'), exact: true }).click();
    await expect(items).toHaveCount(1);
    await expect(items.first().getByRole('button', { name: t('collection.delete') })).toBeFocused();
    await expect(inspector.getByRole('combobox', { name: t('inspector.asset'), exact: true })).toHaveValue('assets/happy-dance.gif');

    // The last one: the collection is empty again, and the focus goes to the search.
    await items.first().getByRole('button', { name: t('collection.delete') }).click();
    await page.getByRole('dialog').getByRole('button', { name: t('collection.deleteAction'), exact: true }).click();
    await expect(panel.getByText(t('collection.empty'))).toBeVisible();
    await expect(count).toBeHidden();
    await expect(page.getByRole('button', { name: t('media.searchGifs') })).toBeFocused();
    expect(errors).toEqual([]);
  });

  test('vertical and horizontal', async ({ page, t }) => {
    const errors = watchErrors(page);
    const panel = await openCollection(page, t, 'gifs');
    await collect(page, t, { kind: 'sticker', text: 'star', picks: [0] });
    const star = itemsOf(panel, t).filter({ hasText: 'Star' });
    const status = panel.getByRole('status');
    const inspector = page.locator('#inspector');
    const sides = [
      { axis: 'horizontal', canvas: { width: 1920, height: 480 }, drop: { x: 0.25, y: 0.5 } },
      { axis: 'vertical', canvas: { width: 480, height: 1920 }, drop: { x: 0.5, y: 0.75 } },
    ];
    for (const { axis, canvas, drop } of sides) {
      const orient = page.locator(`#orient-${axis}`);
      await orient.click();
      await expect(orient).toHaveAttribute('aria-pressed', 'true');
      // Add as image: in the middle of the canvas.
      await star.getByRole('button', { name: t('collection.addImage') }).click();
      await expect(status).toHaveText(t('collection.addedImage', { name: 'Star' }));
      await expect(inspector.getByRole('combobox', { name: t('inspector.asset'), exact: true })).toHaveValue(/^assets\/star(-\d+)?\.gif$/);
      await expectCenter(page, t, { x: canvas.width / 2, y: canvas.height / 2 });
      // Its preview dragged onto the canvas: where it is dropped.
      await dragTo(page, star.locator('.collection-thumb'), page.locator('#canvas-box'), drop);
      await expectCenter(page, t, { x: canvas.width * drop.x, y: canvas.height * drop.y });
      // Use as background: the theme's background, in this orientation too.
      await star.getByRole('button', { name: t('collection.useBackground') }).click();
      await expect(status).toHaveText(t('collection.addedBackground', { name: 'Star' }));
      await page.keyboard.press('Escape');
      await expect(inspector.getByText(t('inspector.canvas', canvas))).toBeVisible();
      await expect(inspector.getByRole('group', { name: t('bg.video') })).toContainText(/star-\d+\.gif/);
      await expectAccessible(page);
    }
    // Each use copied the item into the theme: "This theme" lists it.
    await page.getByRole('tab', { name: t('media.thisTheme') }).click();
    await expect(page.locator('#media-list')).toContainText('star.gif');
    expect(errors).toEqual([]);
  });

  test('reduced motion: stills', async ({ page, t }) => {
    const errors = watchErrors(page);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    const panel = await openCollection(page, t, 'gifs');
    await collect(page, t, { kind: 'gif', text: 'wave', picks: [0] });
    await collect(page, t, { kind: 'sticker', text: 'star', picks: [0] });
    const previews = panel.getByRole('list', { name: t('collection.list') }).locator('img');
    await expect(previews).toHaveCount(2);
    for (const preview of await previews.all()) {
      await expect(preview).toHaveAttribute('src', /^data:image\/svg/);
      await expect(preview).not.toHaveAttribute('src', /animate/);
    }
    const filter = panel.getByRole('group', { name: t('collection.filter') }).getByRole('button', { name: t('collection.filter.all') });
    expect(await filter.evaluate((button) => getComputedStyle(button).transitionDuration)).toBe('0s');
    await expectAccessible(page);

    // Motion allowed: the previews move; reduced again: stills again.
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await expect(previews.first()).toHaveAttribute('src', /animate/);
    await expect(previews.last()).toHaveAttribute('src', /animate/);
    expect(await filter.evaluate((button) => getComputedStyle(button).transitionDuration)).not.toBe('0s');
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await expect(previews.first()).not.toHaveAttribute('src', /animate/);
    await expect(previews.last()).not.toHaveAttribute('src', /animate/);
    expect(errors).toEqual([]);
  });
});

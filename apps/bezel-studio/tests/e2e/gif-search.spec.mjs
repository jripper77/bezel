// Searching GIFs and stickers on KLIPY (D-2026-10-01-gif-sticker-search-3,
// -4, -6), in demo mode, in pt-BR and en, light and dark: the dialog opens
// from the Media tab's Collection and asks nothing of KLIPY; without a key
// searching waits for one and the "?" discloses how to get it (Esc and a
// click outside close it, the focus goes back), the Partner Panel and the
// guide open through the backend; a refused key opens the help; explicit
// results are off at every start and kept while the app runs; typing waits
// for a pause; a 429 explains the 100 requests per hour with the Partner
// Panel. No console errors, no serious or critical accessibility violations.
import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

const root = (page) => page.locator('html');
/** The last query the demo's KLIPY was asked (`null`: none). */
const lastQuery = async (page) => JSON.parse((await root(page).getAttribute('data-demo-gif-query')) ?? 'null');

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
    const dialog = await openSearch(page, t, 'gifs');
    await expect(dialog.getByText(t('gifs.keySaved', { last4: 'a1b2' }))).toBeVisible();
    const field = dialog.getByRole('searchbox', { name: 'Search KLIPY' });
    await expect(field).toBeFocused();
    const explicit = dialog.getByRole('switch', { name: t('gifs.explicit') });
    await expect(explicit).not.toBeChecked();
    await expect(dialog.getByText(t('gifs.start'))).toBeVisible();
    await expectAccessible(page);
    expect(await lastQuery(page)).toBeNull();

    // Typing searches after a pause: 24 results, stills while motion is reduced.
    await field.pressSequentially('ca');
    await expect.poll(() => lastQuery(page)).toEqual({ kind: 'gif', text: 'ca', page: 1, explicit: false });
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

    // The keyboard: arrows, Home, End; Enter adds.
    await tiles.first().focus();
    await page.keyboard.press('ArrowRight');
    await expect(tiles.nth(1)).toBeFocused();
    await page.keyboard.press('End');
    await expect(tiles.last()).toBeFocused();
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


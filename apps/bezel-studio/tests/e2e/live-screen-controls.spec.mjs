// The 8.8" live in demo mode, in pt-BR and en, light and dark, named by the
// backend by its MCU port instead of the key it is listed by (`mcuLive`, like
// 0.1.0-dev.287; D-2026-10-01-live-screen-controls-5).
//
// What this proves is the UI half of the fix: past the 1 s samples and the
// 5 s device refresh the UI keeps the listed 8.8" (its display port) chosen,
// so the Storage tab, the brightness slider and "For this screen" all target
// the listed key. Storage then shows its internal memory and its SD card,
// "For this screen" stays enabled and pressed once chosen, and the level set
// on the slider is the one the Storage dialog reads back.
//
// It does not prove the backend half. The demo backend is more permissive
// than the real one: its `setBrightness` ignores live mode and its Storage
// knows the live screen by either port, so neither can fail here the way
// 0.1.0-dev.287 did ("Device or resource busy"). That half, either port of
// the live screen reaching its open link instead of reopening it, is proven
// in Rust (CONTEXT DoD 3): in bezel-studio
// `backend::tests::either_port_of_the_live_screen_reaches_its_live_link`,
// `backend::tests::a_screen_put_live_by_its_mcu_port_is_keyed_by_its_display`,
// `manager::tests::the_overview_of_the_live_screen_borrows_its_link` and
// `storage::tests::playing_a_file_is_refused_on_either_port_of_the_live_screen`;
// in bezel-devices `connector::tests::a_port_this_app_holds_fails_at_once_without_a_wake`.
//
// The page's clock is Playwright's: the refresh comes without waiting for it.
// No console errors, no serious or critical accessibility violations.
import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

/** The 8.8" as the devices list it: by its display; its MCU is `/dev/ttyACM0`. */
const LISTED = '/dev/ttyACM1';
/** Long enough for a device refresh (every 5 s) and a few samples (every 1 s). */
const PAST_REFRESH_MS = 6_000;

/** Opens the `mcuLive` demo on a clock the test drives, and puts the 8.8" live. */
async function goLive(page, t) {
  await page.clock.install();
  await page.goto('/index.html?demo=mcuLive');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('switch').click({ force: true });
  await expect(page.getByRole('switch')).toBeChecked();
  await expect(page.locator('#status-device')).toHaveText(t('status.live'));
}

/** Lets the samples and a device refresh run; the 8.8" stays chosen and live. */
async function pastRefresh(page, t) {
  await page.clock.runFor(PAST_REFRESH_MS);
  await expect(page.locator('#screen-select')).toHaveValue(LISTED);
  await expect(page.locator('#status-device')).toHaveText(t('status.live'));
  await expect(page.getByRole('switch')).toBeChecked();
  // Its card under Screen says it is the live one.
  await expect(page.locator('#screen-panel .screen-card .meta')).toHaveText(`${LISTED} · ${t('screen.state.awake')} · ${t('status.live')}`);
}

async function openStorage(page, t) {
  await page.getByRole('tab', { name: t('library.screen') }).click();
  await page.getByRole('tab', { name: t('screen.storageTab') }).click();
  const internal = page.getByRole('region', { name: t('storage.medium.internal') });
  const card = page.getByRole('region', { name: t('storage.medium.sd') });
  return { internal, card };
}

test('live screen controls: the UI keeps the listed 8.8" chosen while it is live by its MCU port, so Storage shows its internal memory and SD card', async ({ page, t }) => {
  const errors = watchErrors(page);
  await goLive(page, t);
  await pastRefresh(page, t);
  const { internal, card } = await openStorage(page, t);
  await expect(internal.getByRole('listbox')).toBeVisible();
  await expect(card.getByRole('listbox')).toBeVisible();
  await expect(page.locator('#storage-panel')).not.toContainText(t('screen.none'));
  await expect(page.getByText(t('storage.liveHint'))).toBeVisible();
  await expectAccessible(page);

  // Another refresh while the Storage tab is shown: nothing goes away.
  await pastRefresh(page, t);
  await expect(internal.getByRole('listbox')).toBeVisible();
  await expect(card.getByRole('listbox')).toBeVisible();
  await expect(internal.getByText('logo.png', { exact: true })).toBeVisible();
  await expect(card.getByText('chuva.mp4', { exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test('live screen controls: with the listed 8.8" kept chosen, "For this screen" stays usable and the brightness is set under the listed key', async ({ page, t }) => {
  const errors = watchErrors(page);
  await goLive(page, t);
  await page.getByRole('tab', { name: t('library.themes') }).click();
  const scope = page.getByRole('group', { name: t('themes.scopeLabel') });
  const forScreen = scope.getByRole('button', { name: t('themes.forScreen') });
  const all = scope.getByRole('button', { name: t('themes.all') });
  await all.click();
  await expect(all).toHaveAttribute('aria-pressed', 'true');
  await pastRefresh(page, t);
  await expect(forScreen).toBeEnabled();
  await forScreen.click();
  await expect(forScreen).toHaveAttribute('aria-pressed', 'true');
  await expect(all).toHaveAttribute('aria-pressed', 'false');
  await pastRefresh(page, t);
  await expect(forScreen).toBeEnabled();
  await expect(forScreen).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#theme-grid .theme-card')).toHaveCount(1);
  await expectAccessible(page);

  // UI half only: the demo never refuses a brightness, so "no error" here
  // says nothing of the backend (that is the Rust DoD 3 tests named above).
  await test.step('the brightness slider of the listed 8.8" sets the level the Storage dialog of the same key reads', async () => {
    await page.getByRole('tab', { name: t('library.screen') }).click();
    await page.getByRole('tab', { name: t('screen.settingsTab') }).click();
    await page.getByRole('slider', { name: t('screen.brightness') }).fill('40');
    await pastRefresh(page, t);
    await expect(page.locator('#toast')).toBeHidden();
    await page.getByRole('tab', { name: t('screen.storageTab') }).click();
    await page.getByRole('region', { name: t('storage.bootTitle') }).getByRole('button', { name: t('storage.defaultAction') }).click();
    const reset = page.getByRole('dialog', { name: t('storage.confirmDefaultTitle') });
    await expect(reset).toContainText(t('storage.bootKeeps', { percent: 40 }));
    await expectAccessible(page);
    await reset.getByRole('button', { name: t('dialog.cancel') }).click();
  });
  expect(errors).toEqual([]);
});

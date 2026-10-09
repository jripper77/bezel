// The Themes tab in demo mode, in pt-BR and en, light and dark: the gallery
// lists the themes for the connected screen by default, with thumbnails and
// the screen each theme was made for; "All" and the orientation narrow or
// widen it; an empty result says why and offers new themes and "Show all";
// a theme that cannot be drawn shows "no preview"; a saved theme gets a new
// thumbnail; without a screen every theme is listed.
import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';
import { screenLabel } from '../../src/theme-filter.js';
import { DEMO_LIBRARY } from '../../src/demo-data.js';

const cards = (page) => page.locator('#theme-grid .theme-card');
const total = DEMO_LIBRARY.length + 1;

async function openThemes(page, t, scenario = 'turing88') {
  await page.goto(`/index.html?demo=${scenario}`);
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: t('library.themes') }).click();
  return {
    scope: page.getByRole('group', { name: t('themes.scopeLabel') }),
    axis: page.getByRole('group', { name: t('themes.axisLabel') }),
  };
}

test('the gallery lists the themes for this screen, with thumbnails', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  const { scope, axis } = await openThemes(page, t);
  const forScreen = scope.getByRole('button', { name: t('themes.forScreen') });
  const all = scope.getByRole('button', { name: t('themes.all') });
  await expect(forScreen).toHaveAttribute('aria-pressed', 'true');
  await expect(all).toHaveAttribute('aria-pressed', 'false');
  await expect(axis.getByRole('button', { name: t('themes.bothAxes') })).toHaveAttribute('aria-pressed', 'true');
  await expect(cards(page)).toHaveCount(1);
  const demo = cards(page).first();
  await expect(demo).toContainText('Demo');
  await expect(demo).toContainText(t('axis.vertical'));
  await expect(demo).toContainText(screenLabel({ canvas: { width: 480, height: 1920 }, diagonalHundredths: 880 }, lang));
  await expect(demo.locator('.thumb-screen')).toHaveAttribute('data-state', 'ready');
  expect(await demo.locator('.thumb-screen').evaluate((e) => e.style.backgroundImage)).toContain('data:image/svg+xml');
  await expect(page.locator('#theme-count')).toHaveText(t('themes.countSome', { shown: 1, total }));
  await expectAccessible(page);

  // Every theme, each with its thumbnail but the one that cannot be drawn.
  await all.click();
  await expect(all).toHaveAttribute('aria-pressed', 'true');
  await expect(forScreen).toHaveAttribute('aria-pressed', 'false');
  await expect(cards(page)).toHaveCount(total);
  await expect(page.locator('#theme-grid .thumb-screen[data-state="ready"]')).toHaveCount(total - 1);
  const broken = cards(page).filter({ hasText: 'TURZX' });
  await expect(broken.locator('.thumb-screen')).toHaveAttribute('data-state', 'none');
  await expect(broken.locator('.no-preview')).toHaveText(t('themes.noPreview'));
  await expect(cards(page).filter({ hasText: 'Midnight 5"' })).toContainText(screenLabel({ canvas: { width: 800, height: 480 }, diagonalHundredths: 500 }, lang));
  const bundled = cards(page).filter({ hasText: 'Midnight 2.1"' });
  await expect(bundled.locator('.theme-screen')).toContainText('480×480');
  await expect(bundled.locator('.tag')).toHaveText(t('themes.bundled'));
  await expect(page.locator('#theme-count')).toHaveText(t('themes.countAll', { count: total }));
  await expectAccessible(page);

  // One orientation, from the keyboard.
  const horizontal = axis.getByRole('button', { name: t('axis.horizontal'), exact: true });
  await horizontal.focus();
  await page.keyboard.press('Space');
  await expect(horizontal).toHaveAttribute('aria-pressed', 'true');
  await expect(cards(page)).toHaveCount(2);
  await expect(cards(page).nth(0)).toContainText('Midnight 5"');
  await expect(cards(page).nth(1)).toContainText('TURZX');
  await page.keyboard.press('Tab');
  await expect(cards(page).nth(0)).toBeFocused();

  // The choice stays when another tab is shown and the tab comes back.
  await page.getByRole('tab', { name: t('library.widgets') }).click();
  await page.getByRole('tab', { name: t('library.themes') }).click();
  await expect(all).toHaveAttribute('aria-pressed', 'true');
  await expect(horizontal).toHaveAttribute('aria-pressed', 'true');
  expect(errors).toEqual([]);
});

test('an empty result says why and offers new themes and "show all"', async ({ page, t }) => {
  const errors = watchErrors(page);
  const { scope, axis } = await openThemes(page, t);
  await axis.getByRole('button', { name: t('axis.horizontal'), exact: true }).click();
  const grid = page.locator('#theme-grid');
  await expect(cards(page)).toHaveCount(0);
  await expect(grid).toContainText(t('themes.noneForScreenAxis'));
  await expect(page.locator('#theme-count')).toHaveText('');
  await expect(grid.getByRole('button', { name: t('themes.newVertical') })).toBeVisible();
  await expect(grid.getByRole('button', { name: t('themes.newHorizontal') })).toBeVisible();
  await expectAccessible(page);

  await grid.getByRole('button', { name: t('themes.showAll') }).click();
  const all = scope.getByRole('button', { name: t('themes.all') });
  await expect(all).toHaveAttribute('aria-pressed', 'true');
  await expect(all).toBeFocused();
  await expect(axis.getByRole('button', { name: t('themes.bothAxes') })).toHaveAttribute('aria-pressed', 'true');
  await expect(cards(page)).toHaveCount(total);

  // Back to this screen, vertical: the new horizontal theme of the empty state.
  await scope.getByRole('button', { name: t('themes.forScreen') }).click();
  await axis.getByRole('button', { name: t('axis.horizontal'), exact: true }).click();
  await grid.getByRole('button', { name: t('themes.newHorizontal') }).click();
  await expect(page.locator('#theme-name')).toHaveValue(t('themes.untitled'));
  await expect(page.locator('#inspector')).toContainText(t('inspector.canvas', { width: 1920, height: 480 }));
  await page.keyboard.press('Control+s');
  await expect(cards(page)).toHaveCount(1);
  await expect(cards(page).first()).toContainText(t('themes.untitled'));
  await expect(cards(page).first().locator('.thumb-screen')).toHaveAttribute('data-state', 'ready');
  expect(errors).toEqual([]);
});

test('a saved theme gets a new thumbnail', async ({ page, t }) => {
  const errors = watchErrors(page);
  await openThemes(page, t);
  const screen = cards(page).first().locator('.thumb-screen');
  await expect(screen).toHaveAttribute('data-state', 'ready');
  const before = await screen.evaluate((e) => e.style.backgroundImage);
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await page.getByRole('button', { name: /^CPU/ }).click();
  await page.keyboard.press('Shift+ArrowRight');
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));
  await page.keyboard.press('Control+s');
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  await page.getByRole('tab', { name: t('library.themes') }).click();
  await expect(screen).toHaveAttribute('data-state', 'ready');
  await expect.poll(() => screen.evaluate((e) => e.style.backgroundImage)).not.toBe(before);
  expect(errors).toEqual([]);
});

test('without a screen every theme is listed', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=empty');
  await page.getByRole('tab', { name: t('library.themes') }).click();
  const scope = page.getByRole('group', { name: t('themes.scopeLabel') });
  const forScreen = scope.getByRole('button', { name: t('themes.forScreen') });
  await expect(forScreen).toBeDisabled();
  await expect(forScreen).toHaveAttribute('title', t('themes.forScreenNone'));
  await expect(scope.getByRole('button', { name: t('themes.all') })).toHaveAttribute('aria-pressed', 'true');
  await expect(cards(page)).toHaveCount(total);
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('the square screen lists the round theme', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turzx');
  await page.getByRole('tab', { name: t('library.themes') }).click();
  const scope = page.getByRole('group', { name: t('themes.scopeLabel') });
  await expect(scope.getByRole('button', { name: t('themes.forScreen') })).toHaveAttribute('title', t('themes.forScreenHint', { name: 'Turing 2.1" Round (USB)' }));
  await expect(cards(page)).toHaveCount(1);
  await expect(cards(page).first()).toContainText('Midnight 2.1"');
  expect(errors).toEqual([]);
});

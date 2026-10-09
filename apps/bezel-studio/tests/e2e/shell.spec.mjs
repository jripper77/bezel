// The window's shell: the vertical rail of panels, the floating bars over the
// stage and the status bar, by keyboard and pointer, in every scheme and language.
import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

const PANELS = ['widgets', 'sensors', 'layers', 'themes', 'media', 'screen'];

test('the panel rail is a vertical tab list the arrows walk', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  const rail = page.getByRole('tablist', { name: t('library.title'), exact: true });
  await expect(rail).toHaveAttribute('aria-orientation', 'vertical');
  for (const name of PANELS) {
    await expect(rail.getByRole('tab', { name: t(`library.${name}`), exact: true })).toHaveAttribute('id', `tab-${name}`);
  }
  const tab = (name) => page.locator(`#tab-${name}`);
  await tab('widgets').click();
  await page.keyboard.press('ArrowDown');
  await expect(tab('sensors')).toBeFocused();
  await expect(tab('sensors')).toHaveAttribute('aria-selected', 'true');
  await expect(page.locator('#panel-sensors')).toBeVisible();
  await expect(page.locator('#panel-widgets')).toBeHidden();
  const ring = await tab('sensors').evaluate((n) => getComputedStyle(n).outlineStyle);
  expect(ring).toBe('solid');
  await page.keyboard.press('ArrowUp');
  await expect(tab('widgets')).toBeFocused();
  await page.keyboard.press('ArrowUp');
  await expect(tab('screen')).toBeFocused();
  await expect(page.locator('#panel-screen')).toBeVisible();
  await page.keyboard.press('Home');
  await expect(tab('widgets')).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('End');
  await expect(tab('screen')).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Home');
  await expect(tab('widgets')).toHaveAttribute('tabindex', '0');
  await expect(tab('sensors')).toHaveAttribute('tabindex', '-1');
  expect(await page.evaluate(() => getComputedStyle(document.body).fontFamily)).toContain('IBM Plex Sans');
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('the floating bars over the stage edit and zoom', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const stage = page.locator('#stage');
  for (const id of ['undo', 'redo', 'copy', 'paste', 'zoom-in', 'zoom-out', 'zoom-fit', 'orient-vertical', 'orient-horizontal', 'orient-turn']) {
    await expect(stage.locator(`#${id}`)).toHaveCount(1);
  }
  await expect(page.locator('.topbar #undo')).toHaveCount(0);
  const box = await stage.boundingBox();
  for (const bar of ['.stage-tools', '.stage-zoom']) {
    const b = await page.locator(bar).boundingBox();
    expect(b.x).toBeGreaterThanOrEqual(box.x);
    expect(b.x + b.width).toBeLessThanOrEqual(box.x + box.width + 1);
  }

  await page.getByRole('button', { name: t('library.addWidget', { name: t('widget.text') }), exact: true }).click();
  await expect(page.locator('#undo')).toBeEnabled();
  await page.locator('#undo').click();
  await expect(page.locator('#redo')).toBeEnabled();
  await page.locator('#redo').click();
  await expect(page.locator('#undo')).toBeEnabled();

  const label = page.locator('#zoom-label');
  const fitted = await label.textContent();
  await page.locator('#zoom-in').click();
  await expect(label).not.toHaveText(fitted);
  await page.locator('#zoom-fit').click();
  await expect(label).toHaveText(fitted);
  await expect(page.getByRole('toolbar', { name: t('top.edit'), exact: true })).toBeVisible();
  await expect(page.getByRole('toolbar', { name: t('top.zoom'), exact: true })).toBeVisible();
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('the status bar keeps its regions', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  const bar = page.getByRole('status').filter({ has: page.locator('#status-main') });
  for (const id of ['status-main', 'status-device', 'status-libre', 'status-libre-toggle', 'status-libre-dot', 'status-libre-label']) {
    await expect(bar.locator(`#${id}`)).toHaveCount(1);
  }
  // Restarting Libre lives in the dot's dialog, out of the live region.
  await expect(page.locator('#libre-popover #restart-libre')).toHaveCount(1);
  await expect(page.locator('#status-libre-toggle')).toHaveAttribute('aria-controls', 'libre-popover');
  await expect(page.locator('#status-device')).not.toBeEmpty();
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

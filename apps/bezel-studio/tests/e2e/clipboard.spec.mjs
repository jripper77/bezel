import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

test('copy and paste objects with keyboard, toolbar and grouped Undo', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await expect(page.locator('#copy')).toBeDisabled();
  await expect(page.locator('#paste')).toBeDisabled();
  await page.getByRole('button', { name: t('library.addWidget', { name: t('widget.clock') }), exact: true }).click();
  await page.locator('#copy').focus();
  await page.keyboard.press('Control+c');
  await expect(page.locator('#paste')).toBeEnabled();
  await page.keyboard.press('Control+v');
  await page.getByRole('tab', { name: t('library.layers'), exact: true }).click();
  const layers = page.locator('#layer-list .layer-row');
  await expect(layers).toHaveCount(5);
  await page.locator('#undo').click();
  await expect(layers).toHaveCount(4);
  await page.locator('#redo').click();
  await expect(layers).toHaveCount(5);
  await page.locator('#copy').focus();
  await page.keyboard.press('Control+a');
  await page.locator('#copy').click();
  await page.locator('#paste').click();
  await expect(layers).toHaveCount(10);
  await expect(page.locator('.sel-box')).toHaveCount(5);
  await page.locator('#undo').click();
  await expect(layers).toHaveCount(5);
  // Ordinary text editing must never paste editor objects.
  await page.locator('#theme-name').focus();
  await page.keyboard.press('Control+c');
  await page.keyboard.press('Control+v');
  await expect(layers).toHaveCount(5);
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

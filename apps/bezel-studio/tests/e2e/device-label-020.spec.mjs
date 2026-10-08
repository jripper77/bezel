import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

test('device picker names ambiguous USB devices and retains distinct port addresses', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=two');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const picker = page.locator('#screen-select');
  await expect(picker.locator('option[value="COM3"]')).toHaveText('Turing UsbMonitor · COM3');
  await expect(picker.locator('option[value="/dev/ttyACM1"]')).toHaveText('Turing Smart Screen 8.8" · 480×1920 · /dev/ttyACM1');
  await picker.selectOption('COM3');
  await expect(picker).toHaveValue('COM3');
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

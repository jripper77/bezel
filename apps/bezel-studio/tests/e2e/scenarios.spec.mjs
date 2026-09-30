// Each demo scenario renders without console errors and without serious
// accessibility violations, in light and dark themes.
import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

function watchErrors(page) {
  const errors = [];
  page.on('console', (msg) => msg.type() === 'error' && errors.push(msg.text()));
  page.on('pageerror', (err) => errors.push(err.message));
  return errors;
}

async function expectAccessible(page) {
  const { violations } = await new AxeBuilder({ page }).analyze();
  const serious = violations.filter((v) => ['critical', 'serious'].includes(v.impact));
  expect(serious.map((v) => `${v.id}: ${v.help} @ ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
}

test('turing 8.8" is listed, selected and previewed', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#status')).toHaveText('1 tela conectada');
  const card = page.getByRole('button', { name: /Turing Smart Screen 8.8"/ });
  await expect(card).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#device-frame')).toBeVisible();
  await expect(page.locator('#device-size')).toHaveText('480×1920');
  await expect(page.locator('#details')).toContainText('/dev/ttyACM0 (1a86:ca88, CT88INCH)');
  const box = await page.locator('#device-screen').boundingBox();
  expect(Math.round(box.height / box.width)).toBe(4);
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('two screens: selecting the asleep one hides the preview', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=two');
  await expect(page.locator('#status')).toHaveText('2 telas conectadas');
  await page.getByRole('button', { name: /2\.1"/ }).click();
  await expect(page.locator('#device-frame')).toBeHidden();
  await expect(page.locator('#details')).toContainText('Modelo confirmado ao conectar');
  await page.keyboard.press('Shift+Tab');
  await page.keyboard.press('Enter');
  await expect(page.locator('#device-frame')).toBeVisible();
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('empty and error scenarios explain themselves', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=empty');
  await expect(page.locator('#empty')).toBeVisible();
  await expect(page.locator('#status')).toHaveText('Nenhuma tela conectada');
  await expectAccessible(page);
  await page.goto('/index.html?demo=error');
  await expect(page.locator('#status')).toContainText('permission denied');
  await page.getByRole('button', { name: 'Procurar telas de novo' }).click();
  await expect(page.locator('#status')).toContainText('permission denied');
  expect(errors).toEqual([]);
});

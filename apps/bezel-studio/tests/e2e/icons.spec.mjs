import { readFileSync } from 'node:fs';
import { test, expect, watchErrors, expectAccessible, dragTo } from './helpers.mjs';

test('offline icons search, insert as SVG and undo in one step', async ({ page, t }) => {
  const errors = watchErrors(page);
  // Exercise the actual native policy: fetch to self is deliberately blocked.
  const { app: { security: { csp } } } = JSON.parse(readFileSync(new URL('../../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
  await page.route('**/index.html?demo=turing88', async (route) => {
    const response = await route.fetch();
    await route.fulfill({ response, headers: { ...response.headers(), 'content-security-policy': csp } });
  });
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: t('library.media'), exact: true }).click();
  await page.getByRole('tab', { name: t('media.icons'), exact: true }).click();
  await expect(page.locator('#icon-grid .icon-choice')).toHaveCount(120);
  const search = page.locator('#icon-search');
  await search.fill('ventola');
  await expect(page.locator('#icon-grid .icon-choice')).not.toHaveCount(0);
  await search.fill('cpu');
  await page.locator('#media-icons').getByLabel(t('icons.color'), { exact: true }).fill('#38bdf8');
  await page.getByLabel(t('icons.size'), { exact: true }).fill('96');
  await page.getByLabel(t('icons.size'), { exact: true }).press('Tab');
  await page.locator('[data-icon="cpu"]').click();
  await expect(page.getByRole('combobox', { name: t('inspector.asset'), exact: true })).toHaveValue(/tabler-cpu.*\.svg$/);
  await expect(page.getByRole('spinbutton', { name: t('inspector.width'), exact: true })).toHaveValue('96');
  const asset = page.getByRole('combobox', { name: t('inspector.asset'), exact: true });
  const original = await asset.inputValue();
  const detail = page.locator('#inspector');
  await detail.locator('input[type="color"]').first().fill('#ff0000');
  await detail.locator('input[type="color"]').first().press('Tab');
  await expect(asset).not.toHaveValue(original);
  await expect(detail.locator('input[type="color"]').first()).toHaveValue('#ff0000');
  const red = await asset.inputValue();
  await detail.getByLabel(t('inspector.strokeWidth'), { exact: true }).fill('3');
  await detail.getByLabel(t('inspector.strokeWidth'), { exact: true }).press('Tab');
  await expect(asset).not.toHaveValue(red);
  await expect(detail.getByLabel(t('inspector.strokeWidth'), { exact: true })).toHaveValue('3');
  const thick = await asset.inputValue();
  await detail.getByLabel(t('icons.shadow'), { exact: true }).fill('2');
  await detail.getByLabel(t('icons.shadow'), { exact: true }).press('Tab');
  await expect(asset).not.toHaveValue(thick);
  await expect(detail.getByLabel(t('icons.shadow'), { exact: true })).toHaveValue('2');
  const shadowed = await asset.inputValue();
  await detail.locator('input[type="color"]').last().fill('#38bdf8');
  await detail.locator('input[type="color"]').last().press('Tab');
  await expect(asset).not.toHaveValue(shadowed);
  for (const ref of [shadowed, thick, red, original]) {
    await page.getByRole('button', { name: t('top.undo'), exact: true }).click();
    await expect(asset).toHaveValue(ref);
  }
  await page.getByRole('button', { name: t('top.undo'), exact: true }).click();
  await page.getByRole('tab', { name: t('library.layers'), exact: true }).click();
  await expect(page.locator('#layer-list .layer-row')).toHaveCount(3);
  await page.getByRole('tab', { name: t('library.media'), exact: true }).click();
  await search.fill('thermometer');
  await dragTo(page, page.locator('[data-icon="thermometer"]'), page.locator('#canvas-box'));
  await expect(page.getByRole('combobox', { name: t('inspector.asset'), exact: true })).toHaveValue(/tabler-thermometer.*\.svg$/);
  await search.fill('heart');
  await page.locator('[data-icon="heart-filled"]').click();
  await expect(asset).toHaveValue(/tabler-heart-filled.*\.svg$/);
  await expect(detail.getByLabel(t('inspector.strokeWidth'), { exact: true })).toHaveCount(0);
  await expect(detail.getByLabel(t('icons.shadow'), { exact: true })).toHaveValue('0');
  await expectAccessible(page);
  await page.screenshot({ path: test.info().outputPath('media-icons.png') });
  expect(errors).toEqual([]);
});


test('electrical aliases and hardware pump icons remain editable and undoable', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await page.getByRole('tab', { name: t('library.media'), exact: true }).click();
  await page.getByRole('tab', { name: t('media.icons'), exact: true }).click();
  const search = page.locator('#icon-search');
  await search.fill('fulmine per elettricit\u00e0');
  await expect(page.locator('[data-icon="bolt"]')).toBeVisible();
  await search.fill('pompa di raffreddamento');
  await page.locator('#icon-source').selectOption('mdi');
  await page.locator('[data-icon="mdi-water-pump"]').click();
  const detail = page.locator('#inspector');
  const asset = detail.getByRole('combobox', { name: t('inspector.asset'), exact: true });
  await expect(asset).toHaveValue(/mdi-water-pump.*\.svg$/);
  const original = await asset.inputValue();
  await detail.locator('input[type="color"]').first().fill('#ff0000');
  await detail.locator('input[type="color"]').first().press('Tab');
  await expect(asset).not.toHaveValue(original);
  await expect(detail.getByLabel(t('inspector.strokeWidth'), { exact: true })).toHaveCount(0);
  await detail.getByLabel(t('icons.shadow'), { exact: true }).fill('2');
  await detail.getByLabel(t('icons.shadow'), { exact: true }).press('Tab');
  await expect(detail.getByLabel(t('icons.shadow'), { exact: true })).toHaveValue('2');
  await page.locator('#undo').click(); await page.locator('#undo').click();
  await expect(asset).toHaveValue(original);
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

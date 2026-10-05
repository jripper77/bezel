import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

test('date language, presets, custom pattern, letter case and Undo', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.clock.setFixedTime(new Date(2026, 9, 5, 14, 30, 45));
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('button', { name: t('library.addWidget', { name: t('widget.clock') }), exact: true }).click();
  const detail = page.locator('#inspector');
  await detail.getByRole('combobox', { name: t('clock.language'), exact: true }).selectOption('it');
  await detail.getByRole('combobox', { name: t('clock.format'), exact: true }).selectOption('%A %e %B');
  await expect(page.locator('#clock-preview')).toContainText('luned\u00ec 5 ottobre');
  await detail.getByRole('combobox', { name: t('clock.casing'), exact: true }).selectOption('upper');
  await expect(page.locator('#clock-preview')).toContainText('LUNED\u00cc 5 OTTOBRE');
  await page.getByRole('button', { name: t('top.undo'), exact: true }).click();
  await expect(page.locator('#clock-preview')).toContainText('luned\u00ec 5 ottobre');
  await detail.getByRole('combobox', { name: t('clock.casing'), exact: true }).selectOption('title');
  await expect(page.locator('#clock-preview')).toContainText('Luned\u00ec 5 Ottobre');
  await detail.getByRole('combobox', { name: t('clock.language'), exact: true }).selectOption('en');
  await expect(page.locator('#clock-preview')).toContainText('Monday 5 October');
  await detail.getByRole('combobox', { name: t('clock.format'), exact: true }).selectOption('custom');
  await detail.getByLabel(t('inspector.pattern'), { exact: true }).fill('%d-%m-%Y %H:%M');
  await detail.getByLabel(t('inspector.pattern'), { exact: true }).press('Tab');
  await expect(page.locator('#clock-preview')).toContainText('05-10-2026 14:30');
  await expect(detail.getByRole('combobox', { name: t('clock.format'), exact: true })).toHaveValue('custom');
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

import { test, expect, expectAccessible, watchErrors } from './helpers.mjs';

test('Libre status detects a failed PSU and restarts the reader', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88&libre=partial');
  await expect(page.locator('#status-libre-label')).toHaveText(t('status.libre.partial'));
  await expect(page.locator('#status-libre-dot')).toHaveAttribute('data-state', 'partial');
  await expect(page.locator('#status-libre')).toHaveAttribute('title', /Corsair/);
  await page.getByRole('button', { name: t('status.libreRestart'), exact: true }).click();
  await expect(page.locator('#restart-libre')).toBeDisabled();
  await expect(page.locator('#status-libre-label')).toHaveText(t('status.libre.restarting'));
  await expect(page.locator('#status-libre-label')).toHaveText(t('status.libre.ok'));
  await expect(page.locator('#status-libre-dot')).toHaveAttribute('data-state', 'ok');
  await expect(page.locator('#restart-libre')).toBeEnabled();
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('Libre status is red without readings and hidden on other providers', async ({ page, t }) => {
  await page.goto('/index.html?demo=turing88&libre=error');
  await expect(page.locator('#status-libre-label')).toHaveText(t('status.libre.error'));
  await expect(page.locator('#status-libre-dot')).toHaveAttribute('data-state', 'error');
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await expect(page.locator('#status-libre')).toBeHidden();
});

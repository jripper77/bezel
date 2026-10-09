import { test, expect, expectAccessible, watchErrors } from './helpers.mjs';

test('Libre status detects a failed PSU and restarts the reader', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88&libre=partial');
  await expect(page.locator('#status-libre-label')).toHaveText(t('status.libre.partial'));
  await expect(page.locator('#status-libre-dot')).toHaveAttribute('data-state', 'partial');
  const dot = page.locator('#status-libre-toggle');
  await expect(dot).toHaveAttribute('title', /Corsair/);
  await expect(dot).toHaveAttribute('aria-expanded', 'false');
  await dot.click();
  await expect(dot).toHaveAttribute('aria-expanded', 'true');
  const popover = page.getByRole('dialog', { name: t('status.libre.partial') });
  await expect(popover).toBeVisible();
  const hardware = popover.getByRole('list', { name: t('sensorStatus.hardware') }).getByRole('listitem');
  await expect(hardware.filter({ hasText: 'Corsair' })).toContainText(t('sensorStatus.missing'));
  await expect(hardware.filter({ hasNotText: 'Corsair' }).first()).toContainText(t('sensorStatus.reading'));
  await expect(popover).toContainText(/Corsair/);
  await expectAccessible(page);
  await popover.getByRole('button', { name: t('status.libreRestart'), exact: true }).click();
  await expect(page.locator('#restart-libre')).toBeDisabled();
  await expect(page.locator('#status-libre-label')).toHaveText(t('status.libre.restarting'));
  await expect(page.locator('#status-libre-label')).toHaveText(t('status.libre.ok'));
  await expect(page.locator('#status-libre-dot')).toHaveAttribute('data-state', 'ok');
  await expect(page.locator('#restart-libre')).toBeEnabled();
  await expect(popover).toBeHidden();
  await expect(page.getByRole('dialog', { name: t('status.libre.ok') })).toBeVisible();
  await expectAccessible(page);
  await page.keyboard.press('Escape');
  await expect(page.locator('#libre-popover')).toBeHidden();
  await expect(dot).toHaveAttribute('aria-expanded', 'false');
  await expect(dot).toBeFocused();
  expect(errors).toEqual([]);
});

test('the Libre dialog opens Sensors and closes on a click outside', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88&libre=error');
  const dot = page.locator('#status-libre-toggle');
  await expect(page.locator('#status-libre-dot')).toHaveAttribute('data-state', 'error');
  await dot.press('Enter');
  const popover = page.locator('#libre-popover');
  await expect(popover).toBeVisible();
  await expect(page.locator('#restart-libre')).toBeFocused();
  await expect(popover).toContainText(t('sensorStatus.hint', { hardware: '' }).split(':')[0]);
  await page.locator('#status-main').click();
  await expect(popover).toBeHidden();
  await dot.click();
  await popover.getByRole('button', { name: t('sensorStatus.openSensors') }).click();
  await expect(popover).toBeHidden();
  await expect(page.getByRole('tab', { name: t('library.sensors') })).toHaveAttribute('aria-selected', 'true');
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

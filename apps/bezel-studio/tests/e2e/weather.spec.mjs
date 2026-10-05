import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

test('weather city selection, appearance options and Undo', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('button', { name: t('library.addWidget', { name: t('widget.weather') }), exact: true }).click();
  const detail = page.locator('#inspector');
  await expect(detail).toContainText(t('weather.location', { city: 'Roma' }));
  await detail.getByLabel(t('weather.city'), { exact: true }).fill('Milano');
  await detail.getByRole('button', { name: t('weather.search'), exact: true }).click();
  await page.locator('#weather-results').getByRole('button', { name: 'Milano, Lazio, Italia', exact: true }).click();
  await expect(detail).toContainText(t('weather.location', { city: 'Milano' }));
  await detail.getByRole('combobox', { name: t('weather.language'), exact: true }).selectOption('it');
  await detail.getByRole('checkbox', { name: t('inspector.fahrenheit'), exact: true }).check();
  await detail.getByRole('checkbox', { name: t('weather.icon'), exact: true }).uncheck();
  await page.locator('#undo').click();
  await expect(detail.getByRole('checkbox', { name: t('weather.icon'), exact: true })).toBeChecked();
  await page.locator('#copy').click(); await page.locator('#paste').click();
  await expect(detail).toContainText(t('weather.location', { city: 'Milano' }));
  await expect(detail.getByRole('checkbox', { name: t('inspector.fahrenheit'), exact: true })).toBeChecked();
  await expect(detail).toContainText('Open-Meteo.com');
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

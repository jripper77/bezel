// "Connect your smart screen" (artboard Connect), in pt-BR and en, light and
// dark: the stage shows it while no screen can be used (none listed, or the
// system denied the selected one), with only what the bridge reports, the
// udev command of a denied port and the theme import; it can be hidden to
// edit the theme, and it never shows while a screen works.
import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

const connectOf = (page, t) => page.getByRole('region', { name: t('connect.title') });

test('without a screen the stage says how to connect one', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=empty');
  const connect = connectOf(page, t);
  await expect(connect).toBeVisible();
  await expect(connect.getByRole('heading', { name: t('connect.devices.searching') })).toBeVisible();
  await expect(connect).toContainText(t('connect.devices.every'));
  await expect(connect.getByRole('listitem')).toHaveCount(3);
  for (const step of ['cable', 'vendor', 'permissions']) await expect(connect).toContainText(t(`connect.step.${step}`));
  // Nothing the bridge did not report: no screen rows, no udev command.
  await expect(connect.getByRole('group', { name: t('udev.command') })).toHaveCount(0);
  await expectAccessible(page);

  await connect.getByRole('button', { name: t('connect.import') }).click();
  await expect(page.locator('#theme-name')).toHaveValue('Imported');

  await connect.getByRole('button', { name: t('connect.hide') }).click();
  await expect(connect).toBeHidden();
  await expect(page.locator('#zoom-fit')).toBeFocused();
  await expect(page.locator('#canvas-box')).toBeVisible();
  expect(errors).toEqual([]);
});

test('a port the system denied is listed with the udev command', async ({ page, context, t }) => {
  const errors = watchErrors(page);
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/index.html?demo=denied');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const connect = connectOf(page, t);
  // The screen is listed: until opening it fails, the editor is all there is.
  await expect(connect).toBeHidden();
  await page.getByRole('switch').click({ force: true });
  await page.getByRole('dialog', { name: t('udev.title') }).getByRole('button', { name: t('dialog.close') }).first().click();
  await expect(connect).toBeVisible();
  await expect(connect.getByRole('heading', { name: t('connect.devices.found') })).toBeVisible();
  await expect(connect).toContainText('Turing Smart Screen 8.8"');
  await expect(connect).toContainText(t('error.accessDenied', { address: '/dev/ttyACM1', reason: 'Permission denied (os error 13)' }));
  await expect(connect).toContainText(t('udev.never'));
  await expectAccessible(page);
  await connect.getByRole('group', { name: t('udev.command') }).getByRole('button', { name: t('udev.copy') }).click();
  await expect(page.locator('#toast')).toHaveText(t('udev.copied'));
  expect(await page.evaluate(() => navigator.clipboard.readText())).toMatch(/^sudo install -m 644 .* && sudo udevadm trigger$/);
  expect(errors).toEqual([]);
});

test('a listing that failed says why; a working screen shows no guide', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=error');
  const connect = connectOf(page, t);
  await expect(connect.getByRole('heading', { name: t('connect.devices.error') })).toBeVisible();
  await expect(connect.getByRole('alert')).toContainText('permission denied');
  await expectAccessible(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await expect(page.locator('#connect')).toBeHidden();
  expect(errors).toEqual([]);
});

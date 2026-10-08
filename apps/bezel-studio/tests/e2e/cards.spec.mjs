import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

test('card faces, shared base, grouping and clipboard are editable', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const detail = page.locator('#inspector');
  const add = async widget => {
    await page.locator('#tab-widgets').click();
    await page.getByRole('button', { name: t('library.addWidget', { name: t(`widget.${widget}`) }), exact: true }).click();
  };
  await add('card');
  const cardName = await detail.getByLabel(t('inspector.name'), { exact: true }).inputValue();
  await detail.getByLabel(t('card.faceTitle'), { exact: true }).fill('Metrics');
  await detail.getByLabel(t('card.faceTitle'), { exact: true }).press('Tab');
  await add('text');
  await expect(detail.getByRole('combobox', { name: t('card.face'), exact: true })).toHaveValue('0');
  await detail.getByRole('combobox', { name: t('card.face'), exact: true }).selectOption('');
  const selectCard = async () => {
    await page.locator('#tab-layers').click();
    await page.locator('.layer-name').filter({ hasText: cardName }).first().click();
  };
  await selectCard();
  await detail.getByRole('button', { name: t('card.addFace'), exact: true }).click();
  await expect(detail.getByRole('combobox', { name: t('card.activeFace'), exact: true })).toHaveValue('1');
  await add('ring');
  await expect(detail.getByRole('combobox', { name: t('card.face'), exact: true })).toHaveValue('1');
  await selectCard();
  await detail.getByRole('button', { name: t('card.duplicateFace'), exact: true }).click();
  await expect(detail.getByRole('combobox', { name: t('card.activeFace'), exact: true })).toHaveValue('2');
  await detail.getByRole('button', { name: t('card.previous'), exact: true }).click();
  await expect(detail.getByRole('combobox', { name: t('card.activeFace'), exact: true })).toHaveValue('1');
  await page.locator('#copy').click(); await page.locator('#paste').click();
  await expect(detail).toContainText(t('inspector.multi', { count: 4 }));
  await page.locator('#undo').click();
  await selectCard();
  await detail.getByRole('button', { name: t('card.removeFace'), exact: true }).click();
  await expect(detail.getByRole('combobox', { name: t('card.activeFace'), exact: true }).locator('option')).toHaveCount(2);
  await expectAccessible(page);
  expect(errors).toEqual([]);
  await page.screenshot({ path: test.info().outputPath('card-editor.png') });
});

import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

test('property sections keep their order and folds through edits and Undo', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await page.getByRole('button', { name: t('library.addWidget', { name: t('widget.ring') }), exact: true }).click();
  await page.locator('#save').click();
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  const sections = page.locator('#inspector .property-section');
  expect(await sections.evaluateAll(nodes => nodes.map(n => n.dataset.section))).toEqual(['position', 'content', 'appearance', 'actions']);
  const appearance = page.locator('#inspector [data-section="appearance"]');
  await appearance.locator('summary').click();
  await expect(appearance).not.toHaveAttribute('open');
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  const x = page.locator('#inspector').getByRole('spinbutton', { name: t('inspector.x'), exact: true });
  const before = await x.inputValue();
  await x.fill('60'); await x.press('Tab');
  await expect(x).toHaveValue('60'); await expect(appearance).not.toHaveAttribute('open');
  await page.locator('#undo').click();
  await expect(x).toHaveValue(before); await expect(appearance).not.toHaveAttribute('open');
  await appearance.locator('summary').focus(); await page.keyboard.press('Enter');
  await expect(appearance).toHaveAttribute('open');
  await expect(appearance.getByRole('combobox', { name: t('ring.fillMode'), exact: true })).toBeVisible();
  await expectAccessible(page); expect(errors).toEqual([]);
});

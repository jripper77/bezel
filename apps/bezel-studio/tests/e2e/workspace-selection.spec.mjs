import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';
async function setup(page, t) {
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.locator('#copy').focus();
  await page.keyboard.press('Control+a'); await page.keyboard.press('Delete');
  await expect(page.locator('.sel-box')).toHaveCount(0);
}
async function field(page, t, key, value) {
  const input = page.locator('#inspector').getByRole('spinbutton', { name: t(`inspector.${key}`), exact: true });
  await input.fill(String(value)); await input.press('Tab');
}
async function shape(page, t, x, y) {
  await page.locator('#tab-widgets').click();
  await page.getByRole('button', { name: t('library.addWidget', { name: t('widget.shape') }), exact: true }).click();
  for (const [key, value] of [['width', 80], ['height', 80], ['x', x], ['y', y]]) await field(page, t, key, value);
}
async function point(page, x, y) {
  const box = await page.locator('#canvas-box').boundingBox();
  return { x: box.x + x * box.width / 480, y: box.y + y * box.height / 1920 };
}
async function drag(page, from, to) {
  await page.mouse.move(from.x, from.y); await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 10 }); await page.mouse.up();
}

test('marquee starts outside the canvas, stays selected on release and exposes card grouping', async ({ page, t }) => {
  const errors = watchErrors(page); await setup(page, t);
  await shape(page, t, 40, 120); await shape(page, t, 170, 120);
  await drag(page, await point(page, -20, 100), await point(page, 270, 210));
  await expect(page.locator('.sel-box')).toHaveCount(2);
  await expect(page.locator('.marquee')).toHaveCount(0);
  const group = page.locator('#inspector').getByRole('button', { name: t('card.group'), exact: true });
  await expect(group).toBeVisible(); await group.click();
  await page.locator('#tab-layers').click();
  await expect(page.locator('.layer-card')).toHaveCount(1);
  await expect(page.locator('.layer-card [data-face="0"] .layer-row')).toHaveCount(2);
  await page.locator('#undo').click();
  await expect(page.locator('.layer-card')).toHaveCount(0);
  await expectAccessible(page); expect(errors).toEqual([]);
});

test('off-canvas objects are visible in the editor and draggable back with one Undo', async ({ page, t }) => {
  const errors = watchErrors(page); await setup(page, t);
  await shape(page, t, -160, 120);
  await page.mouse.click((await page.locator('#stage-scroll').boundingBox()).x + 8, (await page.locator('#stage-scroll').boundingBox()).y + 8);
  await expect(page.locator('.off-canvas-box')).toHaveCount(1);
  const from = await point(page, -120, 160), to = await point(page, 80, 160);
  await page.keyboard.down('Alt'); await drag(page, from, to); await page.keyboard.up('Alt');
  await expect(page.locator('.sel-box')).toHaveCount(1);
  await expect(page.locator('#inspector').getByRole('spinbutton', { name: t('inspector.x'), exact: true })).toHaveValue('40');
  await expect(page.locator('.off-canvas-box')).toHaveCount(0);
  await page.locator('#undo').click();
  await expect(page.locator('#inspector').getByRole('spinbutton', { name: t('inspector.x'), exact: true })).toHaveValue('-160');
  // Selecting from Layers also exposes resize handles outside the screen.
  await page.locator('#tab-layers').click(); await page.locator('.layer-name').first().click();
  const handle = page.locator('.handle[data-h="e"]');
  const h = await handle.boundingBox();
  await page.keyboard.down('Alt'); await drag(page, { x: h.x + h.width / 2, y: h.y + h.height / 2 }, { x: h.x + h.width / 2 + 20, y: h.y + h.height / 2 }); await page.keyboard.up('Alt');
  const width = Number(await page.locator('#inspector').getByRole('spinbutton', { name: t('inspector.width'), exact: true }).inputValue());
  expect(width).toBeGreaterThan(80);
  await expectAccessible(page); expect(errors).toEqual([]);
});


test('ordinary groups select by canvas, transform together and expose individual members in Layers', async ({ page, t }) => {
  const errors = watchErrors(page); await setup(page, t);
  await shape(page, t, 40, 120); await shape(page, t, 170, 120);
  await drag(page, await point(page, -20, 100), await point(page, 270, 210));
  await page.locator('#inspector').getByRole('button', { name: t('group.create'), exact: true }).click();
  await expect(page.locator('#inspector h2')).toHaveText(t('widget.group'));
  await page.locator('#tab-layers').click();
  await expect(page.locator('[data-group-id] > .layer-children .layer-row')).toHaveCount(2);
  await expect(page.locator('.layer-group-heading')).toHaveCount(0);
  // Empty work space clears the selection; clicking a member picks its group.
  const space = await point(page, -20, 90); await page.mouse.click(space.x, space.y);
  const member = await point(page, 60, 140); await page.mouse.click(member.x, member.y);
  await expect(page.locator('#inspector h2')).toHaveText(t('widget.group'));
  await page.locator('#copy').focus(); await page.keyboard.press('ArrowRight');
  await expect(page.locator('#inspector').getByRole('spinbutton', { name: t('inspector.x'), exact: true })).toHaveValue('41');
  await field(page, t, 'width', 420);
  // Members are independently editable via the tree.
  await page.locator('[data-group-id] > .layer-children .layer-name').last().click();
  await expect(page.locator('#inspector h2')).toHaveText(t('widget.shape'));
  await expect(page.locator('#inspector').getByRole('spinbutton', { name: t('inspector.width'), exact: true })).toHaveValue('160');
  await page.locator('[data-group-id] > .layer-list:not(.layer-children) > .layer-row .layer-name').click();
  await page.locator('#copy').focus(); await page.keyboard.press('Control+Shift+g');
  await expect(page.locator('[data-group-id]')).toHaveCount(0);
  await expect(page.locator('.sel-box')).toHaveCount(2);
  await page.locator('#undo').click(); await expect(page.locator('[data-group-id]')).toHaveCount(1);
  await expectAccessible(page); expect(errors).toEqual([]);
});

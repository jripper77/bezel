import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

async function dragRow(page, source, target, before = true, cancel = false, allowed = true) {
  const from = await source.boundingBox(), to = await target.boundingBox();
  await page.mouse.move(from.x + 42, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + 50, from.y + from.height / 2, { steps: 3 });
  await expect(page.locator('.layer-drag-ghost')).toBeVisible();
  await expect(page.locator('.layer-drag-ghost')).toHaveText(await source.locator('.layer-name').evaluate(node => node.firstChild.textContent));
  await page.mouse.move(to.x + 24, before ? to.y + 4 : to.y + to.height - 4, { steps: 8 });
  await expect(page.locator('.layer-drop-before, .layer-drop-after')).toHaveCount(allowed ? 1 : 0);
  if (allowed && !cancel) await page.screenshot({ path: test.info().outputPath('layer-drag-marker.png') });
  if (cancel) await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect(page.locator('.layer-drag-ghost')).toHaveCount(0);
}

test('layer row dragging reorders with Undo, cancellation, and outside-canvas deselection', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.locator('#tab-layers').click();
  const rows = page.locator('#layer-list .layer-row');
  const ids = () => rows.evaluateAll(items => items.map(row => row.dataset.elementId));
  const original = await ids();
  await rows.first().locator('.layer-name').click();
  await expect(page.locator('.sel-box')).toHaveCount(1);
  // Clicking controls outside the stage keeps the current object selected.
  await page.locator('#copy').click();
  await expect(page.locator('.sel-box')).toHaveCount(1);
  const stage = await page.locator('#stage-scroll').boundingBox();
  await page.mouse.click(stage.x + 8, stage.y + 8);
  await expect(page.locator('.sel-box')).toHaveCount(0);
  await expect(page.locator('#copy')).toBeDisabled();
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  await dragRow(page, rows.last(), rows.first());
  expect(await ids()).toEqual([original.at(-1), ...original.slice(0, -1)]);
  await page.locator('#undo').click(); expect(await ids()).toEqual(original);
  await dragRow(page, rows.first(), rows.last(), false);
  expect(await ids()).toEqual([...original.slice(1), original[0]]);
  await page.locator('#undo').click(); expect(await ids()).toEqual(original);
  await dragRow(page, rows.last(), rows.first(), true, true);
  expect(await ids()).toEqual(original);
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  // Name clicking/renaming and the visibility buttons still work after a drag.
  await rows.first().locator('.layer-name').dblclick();
  const name = rows.first().locator('input.layer-name');
  await name.fill('Renamed layer'); await name.press('Enter');
  await expect(rows.first()).toContainText('Renamed layer');
  await rows.first().getByRole('button', { name: t('layers.hide'), exact: true }).click();
  await expect(rows.first()).toHaveClass(/hidden-el/);
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('dragging card rows keeps member scopes, moves complete cards and preserves selection', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const add = async widget => {
    await page.locator('#tab-widgets').click();
    await page.getByRole('button', { name: t('library.addWidget', { name: t(`widget.${widget}`) }), exact: true }).click();
  };
  await add('card'); await add('text'); await add('ring');
  await page.locator('#tab-layers').click();
  const card = page.locator('.layer-card').first();
  const cardRow = card.locator(':scope > .layer-list > .layer-row');
  await cardRow.locator('.layer-name').click();
  await page.locator('#inspector').getByRole('button', { name: t('card.addFace'), exact: true }).click();
  await add('text');
  await page.locator('#tab-layers').click();
  const rows = card.locator('[data-face="0"] .layer-row');
  const ids = () => rows.evaluateAll(items => items.map(row => row.dataset.elementId));
  const before = await ids();
  await dragRow(page, rows.last(), rows.first());
  expect(await ids()).toEqual([...before].reverse());
  await expect(page.locator('#inspector').getByRole('combobox', { name: t('card.face'), exact: true })).toHaveValue('1');
  await page.locator('#undo').click(); expect(await ids()).toEqual(before);
  await dragRow(page, rows.last(), card.locator('[data-face="1"] .layer-row').first(), true, false, true);
  await expect(card.locator('[data-face="1"] .layer-row')).toHaveCount(2);
  await page.locator('#undo').click();
  expect(await ids()).toEqual(before);
  await expect(card.locator('[data-face="1"] .layer-row')).toHaveCount(1);
  const originalRoots = await page.locator('#layer-list > li:not(.layer-root-drop)').evaluateAll(items => items.map(row => row.dataset.cardId ?? row.dataset.elementId));
  await dragRow(page, cardRow, page.locator('#layer-list > .layer-row').last(), false);
  const reorderedRoots = await page.locator('#layer-list > li:not(.layer-root-drop)').evaluateAll(items => items.map(row => row.dataset.cardId ?? row.dataset.elementId));
  expect(reorderedRoots).toEqual([...originalRoots.slice(1), originalRoots[0]]);
  await expect(card.locator('.layer-row')).toHaveCount(4);
  await expect(card.locator('[data-face="0"] .layer-row')).toHaveCount(2);
  await expect(card.locator('[data-face="1"] .layer-row')).toHaveCount(1);
  await page.locator('#undo').click();
  expect(await page.locator('#layer-list > li:not(.layer-root-drop)').evaluateAll(items => items.map(row => row.dataset.cardId ?? row.dataset.elementId))).toEqual(originalRoots);
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

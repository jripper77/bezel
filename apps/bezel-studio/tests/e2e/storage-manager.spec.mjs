// The storage manager (D-2026-09-30-storage-manager-4, -13) in demo mode, in
// pt-BR and en, light and dark, on the user's real card (`vendorCard`): the
// Storage tab takes the whole window with the internal memory and the card
// side by side; files move by dragging or by keyboard, always after a
// confirmation that lists source → target, and the source goes only once the
// copy is done; the cleanup assistant lists the vendor app's copies, checks
// only exact signals and deletes exactly the list confirmed; renaming,
// restoring, associating an original and clearing the local copies. Every job
// waits in the middle until the test lets it go: nothing depends on timing.
// No console errors, no serious or critical accessibility violations.
import { test, expect, watchErrors, expectAccessible, dragTo } from './helpers.mjs';
import { DEMO_LET_GO_EVENT } from '../../src/bridge.js';
import { formatBytes } from '../../src/storage-manager.js';

/** The user's card: the vendor app's re-converted copies and the file that stays of each. */
const VARIANTS = [
  ['demon_open.mp4.mp4.mp4', 'demon_open.mp4.mp4'],
  ['demon.mp401115025.mp4', 'demon.mp4.mp4.mp4'],
  ['NVI.mp427034822.mp4', 'NVI.mp4'],
  ['Rani.mp417075004.mp4', 'Rani.mp4'],
  ['m04.mp424045157.mp4', 'm04.mp4'],
];
const USER_CARD = [
  'demon_open.mp4.mp4.mp4', 'demon.mp4.mp4.mp4', 'demon.mp401115025.mp4', '8.8APEX_2.mp4', 'demon_open.mp4.mp4', 'AMD.mp4',
  'NVI.mp427034822.mp4', 'NVI.mp4', 'Rani.mp4', 'm04.mp4', 'Rani.mp417075004.mp4', 'm04.mp424045157.mp4',
];
const HANG_PARTIAL = 29_577_216;

const escape = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** Opens the Storage tab of a demo scenario whose jobs hold until let go. */
async function openManager(page, t, demo = 'vendorCard') {
  await page.goto(`/index.html?demo=${demo}&hold`);
  await page.getByRole('tab', { name: t('library.screen') }).click();
  await page.getByRole('tab', { name: t('screen.storageTab') }).click();
  const internal = page.getByRole('region', { name: t('storage.medium.internal') });
  const card = page.getByRole('region', { name: t('storage.medium.sd') });
  await expect(internal.getByRole('listbox')).toBeVisible();
  return { internal, card };
}

/** The option of the file named exactly `name` in a medium's list. */
const option = (region, name) => region.getByRole('option').filter({ has: region.page().getByText(name, { exact: true }) });

/** A file and its medium, as the confirmations write it. */
const place = (t, name, medium) => t('storage.plan.place', { name, medium: t(`storage.medium.${medium}`) });

/** Drags the option of `name` from one side onto the other, like a person. */
async function dragFile(page, from, name, to) {
  const source = option(from, name);
  await source.scrollIntoViewIfNeeded();
  await dragTo(page, source, to.getByRole('heading', { level: 3 }));
}

/** Lets `count` held job phases go on: one now, the others when they come. */
async function letGo(page, count = 1) {
  await page.evaluate(({ name, times }) => {
    for (let i = 0; i < times; i += 1) window.dispatchEvent(new Event(name));
  }, { name: DEMO_LET_GO_EVENT, times: count });
}

/** Moves the list's focus with the arrow keys to the option of `name`. */
async function focusOption(page, list, name) {
  await page.keyboard.press('Home');
  for (let i = 0; i < 30; i += 1) {
    const active = await list.getAttribute('aria-activedescendant');
    if ((await list.locator(`[id="${active}"] .file-name`).textContent()) === name) return;
    await page.keyboard.press('ArrowDown');
  }
  throw new Error(`${name} is not in the list`);
}

test('storage manager moves files by drag and by keyboard after confirming', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  const { internal, card } = await openManager(page, t);
  // The whole window, the two media side by side.
  await expect(page.locator('#stage')).toBeHidden();
  const left = await internal.boundingBox();
  const right = await card.boundingBox();
  expect(left.x + left.width).toBeLessThanOrEqual(right.x);
  expect(Math.abs(left.y - right.y)).toBeLessThan(2);
  // A thumbnail from the local copy; a vendor file is played on the screen to be seen.
  await expect(option(internal, 'earth.mp4').locator('img')).toHaveAttribute('src', /^data:image\/svg\+xml/);
  await expect(option(card, 'AMD.mp4')).toContainText(t('storage.noCopy'));
  await expectAccessible(page);

  // Dragging a file to the other side asks first, listing source → target.
  const step = t('storage.plan.step', { source: place(t, 'earth.mp4', 'internal'), target: place(t, 'earth.mp4', 'sd'), size: formatBytes(2_516_582, lang) });
  const moveToCard = page.getByRole('dialog', { name: t('storage.plan.title.move', { to: t('storage.to.sd') }) });
  await dragFile(page, internal, 'earth.mp4', card);
  await expect(moveToCard).toContainText(step);
  await expect(moveToCard).toContainText(t('storage.plan.intro.move'));
  await expect(moveToCard).toContainText(t('storage.warning.bootMedia', { name: 'earth.mp4' }));
  await expectAccessible(page);
  await moveToCard.getByRole('button', { name: t('dialog.cancel') }).click();
  await expect(moveToCard).toHaveCount(0);
  await expect(option(internal, 'earth.mp4')).toHaveCount(1);
  await expect(option(card, 'earth.mp4')).toHaveCount(0);

  // Confirmed, the copy goes first; the job says which file it is at.
  await dragFile(page, internal, 'earth.mp4', card);
  await moveToCard.getByRole('button', { name: t('storage.plan.action.move'), exact: true }).click();
  await expect(page.getByRole('progressbar', { name: t('storage.progressLabel') })).toBeVisible();
  await expect(page.locator('#storage-panel').getByRole('status')).toHaveText(t('storage.job.move', { name: 'earth.mp4', index: 1, count: 1 }));
  await expect(page.getByText(t('storage.job.planHint'))).toBeVisible();
  await expect(internal.getByRole('button', { name: t('storage.action.moveTo.sd') })).toBeDisabled();
  await expectAccessible(page);
  await letGo(page);
  const report = page.getByRole('region', { name: t('storage.report.finished') });
  await expect(report).toContainText(t('storage.report.done.move', { count: 1 }));
  await expect(option(card, 'earth.mp4')).toHaveCount(1);
  await expect(option(internal, 'earth.mp4')).toHaveCount(0);

  // By keyboard: Ctrl+A, Esc, Shift+arrows, then arrows and Space.
  const list = card.getByRole('listbox');
  const selected = list.getByRole('option', { selected: true });
  await list.focus();
  await page.keyboard.press('ControlOrMeta+a');
  await expect(selected).toHaveCount(USER_CARD.length + 2);
  await page.keyboard.press('Escape');
  await expect(selected).toHaveCount(0);
  await page.keyboard.press('Home');
  await page.keyboard.press('Shift+ArrowDown');
  await page.keyboard.press('Shift+ArrowDown');
  await expect(selected).toHaveCount(3);
  await page.keyboard.press('Escape');
  await focusOption(page, list, 'earth.mp4');
  await page.keyboard.press('Space');
  await focusOption(page, list, 'AMD.mp4');
  await page.keyboard.press('Space');
  await expect(selected).toHaveCount(2);
  await expect(card).toContainText(t('storage.list.selected', { count: 2, size: formatBytes(2_516_582 + 4_079_432, lang) }));

  // "Move to the internal memory" does what dragging does; a file without a
  // local copy is left out, with the reason.
  await card.getByRole('button', { name: t('storage.action.moveTo.internal') }).focus();
  await page.keyboard.press('Enter');
  const back = page.getByRole('dialog', { name: t('storage.plan.title.move', { to: t('storage.to.internal') }) });
  await expect(back).toContainText(t('storage.plan.step', { source: place(t, 'earth.mp4', 'sd'), target: place(t, 'earth.mp4', 'internal'), size: formatBytes(2_516_582, lang) }));
  await expect(back).toContainText(t('storage.skip.noLocalCopy', { name: 'AMD.mp4' }));
  await expectAccessible(page);
  await expect(back.getByRole('button', { name: t('storage.plan.action.move'), exact: true })).toBeFocused();
  await page.keyboard.press('Enter');
  await letGo(page);
  await expect(report).toContainText(t('storage.report.done.move', { count: 1 }));
  await expect(option(internal, 'earth.mp4')).toHaveCount(1);
  await expect(option(card, 'earth.mp4')).toHaveCount(0);
  await expect(option(card, 'AMD.mp4')).toHaveCount(1);

  // The editor comes back with any other tab.
  await page.getByRole('tab', { name: t('library.widgets') }).click();
  await expect(page.locator('#stage')).toBeVisible();
  expect(errors).toEqual([]);
});

test('storage manager cleanup lists the vendor duplicates and deletes only what was confirmed', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  const { card } = await openManager(page, t);
  const open = page.getByRole('button', { name: t('storage.cleanup.open') });
  const assistant = page.getByRole('dialog', { name: t('storage.cleanup.title') });
  await open.click();

  // The five vendor copies, each with the name that stays, none checked.
  const variants = assistant.getByRole('region', { name: t('storage.findingGroup.variant') });
  await expect(variants.getByRole('checkbox')).toHaveCount(VARIANTS.length);
  for (const [copy, kept] of VARIANTS) {
    const row = variants.getByRole('listitem').filter({ has: page.getByText(copy, { exact: true }) });
    await expect(row).toContainText(t('storage.finding.variant', { kept }));
    await expect(row.getByRole('checkbox')).not.toBeChecked();
  }
  // Only the partial a hung upload left is checked; the rest of the user's
  // files are only listed, `8.8APEX_2.mp4` as unused; Bezel's own get none.
  const partial = assistant.getByRole('region', { name: t('storage.findingGroup.hangPartial') });
  await expect(partial.getByRole('checkbox', { name: /^bezel_test_cancel\.mp4/ })).toBeChecked();
  await expect(assistant.getByRole('checkbox', { checked: true })).toHaveCount(1);
  const unused = assistant.getByRole('region', { name: t('storage.findingGroup.unused') });
  await expect(unused.getByRole('checkbox')).toHaveCount(8);
  await expect(unused.getByRole('checkbox', { name: new RegExp(`^${escape('8.8APEX_2.mp4')}`) })).not.toBeChecked();
  await expect(assistant).not.toContainText('earth.mp4');
  await expect(assistant).toContainText(t('storage.cleanup.total', { count: 1, size: formatBytes(HANG_PARTIAL, lang) }));
  await expectAccessible(page);

  // One more checked: the confirmation lists exactly both and the space freed.
  await variants.getByRole('checkbox', { name: /^m04\.mp424045157\.mp4/ }).check();
  await assistant.getByRole('button', { name: t('storage.cleanup.next') }).click();
  let confirm = page.getByRole('dialog', { name: t('storage.cleanup.confirmTitle', { count: 2 }) });
  const exact = confirm.getByRole('list', { name: t('storage.cleanup.confirmList') });
  await expect(exact.getByRole('listitem')).toHaveCount(2);
  await expect(exact).toContainText(place(t, 'bezel_test_cancel.mp4', 'sd'));
  await expect(exact).toContainText(place(t, 'm04.mp424045157.mp4', 'sd'));
  await expect(confirm).toContainText(t('storage.cleanup.confirmFreed', { size: formatBytes(HANG_PARTIAL + 841_053, lang) }));
  // A destructive answer is never the default one.
  await expect(confirm.getByRole('button', { name: t('dialog.cancel') })).toBeFocused();
  await expectAccessible(page);
  // Cancelled: nothing is deleted.
  await page.keyboard.press('Escape');
  await expect(confirm).toHaveCount(0);
  await expect(option(card, 'm04.mp424045157.mp4')).toHaveCount(1);
  await expect(option(card, 'bezel_test_cancel.mp4')).toHaveCount(1);

  // Again, as it comes: only the pre-checked partial goes.
  await open.click();
  await expect(assistant.getByRole('checkbox', { checked: true })).toHaveCount(1);
  await assistant.getByRole('button', { name: t('storage.cleanup.next') }).click();
  confirm = page.getByRole('dialog', { name: t('storage.cleanup.confirmTitle', { count: 1 }) });
  await expect(confirm).toContainText(t('storage.cleanup.confirmFreed', { size: formatBytes(HANG_PARTIAL, lang) }));
  await confirm.getByRole('button', { name: t('storage.cleanup.deleteAction') }).click();
  const report = page.getByRole('region', { name: t('storage.report.finished') });
  await expect(report).toContainText(t('storage.report.deleted', { count: 1, size: formatBytes(HANG_PARTIAL, lang) }));
  await expect(option(card, 'bezel_test_cancel.mp4')).toHaveCount(0);
  await expect(card.getByRole('option')).toHaveCount(USER_CARD.length);
  for (const name of USER_CARD) await expect(option(card, name)).toHaveCount(1);
  expect(errors).toEqual([]);
});

test('storage manager renames and restores from the local copies', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  const { internal, card } = await openManager(page, t);

  // F2 renames the selected file; the preview follows the upload rule.
  const list = internal.getByRole('listbox');
  await list.focus();
  await focusOption(page, list, 'jyanme.mp4');
  await page.keyboard.press('Space');
  await page.keyboard.press('F2');
  const rename = page.getByRole('dialog', { name: t('storage.rename.title', { name: 'jyanme.mp4' }) });
  const field = rename.getByRole('textbox', { name: t('storage.rename.label') });
  await expect(field).toBeFocused();
  await field.fill('jyanme 2.mp4');
  await expect(rename).toContainText(t('storage.plan.refused.invalidChar', { char: ' ' }));
  await expect(rename.getByRole('button', { name: t('storage.rename.next') })).toBeDisabled();
  await field.fill('Jyanme_2.MP4');
  await expect(rename).toContainText(t('storage.rename.preview', { name: 'jyanme_2.mp4' }));
  await expectAccessible(page);
  await field.press('Enter');
  const confirmRename = page.getByRole('dialog', { name: t('storage.plan.title.rename', { name: 'jyanme.mp4' }) });
  await expect(confirmRename).toContainText(t('storage.plan.step', { source: place(t, 'jyanme.mp4', 'internal'), target: place(t, 'jyanme_2.mp4', 'internal'), size: formatBytes(4_404_019, lang) }));
  await confirmRename.getByRole('button', { name: t('storage.plan.action.rename'), exact: true }).click();
  await letGo(page);
  await expect(page.getByRole('region', { name: t('storage.report.finished') })).toContainText(t('storage.report.done.rename', { count: 1 }));
  await expect(option(internal, 'jyanme_2.mp4')).toHaveCount(1);
  await expect(option(internal, 'jyanme.mp4')).toHaveCount(0);

  // Restore: what the catalog has of this card and of another one, checked
  // before anything is sent; nothing is deleted.
  await card.getByRole('button', { name: t('storage.restore.open', { count: 2 }) }).click();
  const restore = page.getByRole('dialog', { name: t('storage.restore.title', { to: t('storage.to.sd') }) });
  await expect(restore.getByRole('checkbox', { checked: true })).toHaveCount(2);
  await expect(restore).toContainText(t('storage.restore.otherCard'));
  await expectAccessible(page);
  await restore.getByRole('button', { name: t('storage.restore.next') }).click();
  const confirmRestore = page.getByRole('dialog', { name: t('storage.plan.title.restore', { to: t('storage.to.sd') }) });
  await expect(confirmRestore).toContainText(t('storage.plan.intro.restore'));
  await expect(confirmRestore.getByRole('list', { name: t('storage.plan.stepsLabel') }).getByRole('listitem')).toHaveCount(2);
  await confirmRestore.getByRole('button', { name: t('storage.plan.action.restore'), exact: true }).click();
  await letGo(page, 2);
  await expect(page.getByRole('region', { name: t('storage.report.finished') })).toContainText(t('storage.report.done.restore', { count: 2 }));
  await expect(option(card, 'relogio.mp4')).toHaveCount(1);
  await expect(option(card, 'foto.png')).toHaveCount(1);
  await expect(card.getByRole('button', { name: t('storage.restore.open', { count: 2 }) })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('storage manager associates an original, clears local copies and keeps TUR_USB from deleting', async ({ page, t }) => {
  const errors = watchErrors(page);
  const { internal } = await openManager(page, t);

  // A file Bezel has no copy of gets one from its original on the PC.
  await option(internal, 'DARIUS.mp4').click();
  await internal.getByRole('button', { name: t('storage.action.associate') }).click();
  const associate = page.getByRole('dialog', { name: t('storage.associate.title', { name: 'DARIUS.mp4' }) });
  await associate.getByRole('button', { name: t('storage.associate.pickFolder') }).click();
  const candidates = associate.getByRole('radio');
  await expect(candidates).toHaveCount(2);
  await expect(associate.getByRole('radio', { name: /^DARIUS\.mp4/ })).toBeChecked();
  await expect(associate).toContainText(t('storage.associate.sameName'));
  await expect(associate).not.toContainText('darius_final.mp4');
  await expectAccessible(page);
  await associate.getByRole('button', { name: t('storage.associate.action'), exact: true }).click();
  await expect(page.locator('#toast')).toHaveText(t('storage.associate.done', { name: 'DARIUS.mp4' }));
  await expect(option(internal, 'DARIUS.mp4')).not.toContainText(t('storage.noCopy'));
  await expect(option(internal, 'DARIUS.mp4').locator('img')).toHaveAttribute('src', /^data:image\/svg\+xml/);

  // The local copies: nothing deleted through Bezel, so only "all" clears;
  // the files keep their thumbnails, without a local copy.
  await page.getByRole('button', { name: t('storage.cache.open') }).click();
  const cache = page.getByRole('dialog', { name: t('storage.cache.title') });
  const clear = cache.getByRole('button', { name: t('storage.cache.clear') });
  await expect(clear).toBeDisabled();
  await expect(cache.getByRole('combobox', { name: t('storage.cache.limit') })).toHaveValue(String(2 * 2 ** 30));
  await cache.getByRole('checkbox', { name: t('storage.cache.all') }).check();
  await expectAccessible(page);
  await clear.click();
  const sure = page.getByRole('dialog', { name: t('storage.cache.confirmTitle') });
  await expect(sure).toContainText(t('storage.cache.confirmKeeps'));
  await sure.getByRole('button', { name: t('storage.cache.clearAction') }).click();
  await expect(option(internal, 'earth.mp4')).toContainText(t('storage.noCopy'));
  await expect(option(internal, 'earth.mp4').locator('img')).toHaveAttribute('src', /^data:/);

  // TUR_USB: copying works, what ends in a delete is off with the reason.
  const usb = await openManager(page, t, 'turzx');
  await option(usb.internal, 'logo.png').click();
  const move = usb.internal.getByRole('button', { name: t('storage.action.moveTo.sd') });
  await expect(move).toBeDisabled();
  await expect(move).toHaveAttribute('title', t('storage.reason.deleteUnsupported'));
  await expect(usb.internal.getByRole('button', { name: t('storage.action.rename') })).toBeDisabled();
  await expect(usb.internal.getByRole('button', { name: t('storage.action.copyTo.sd') })).toBeEnabled();
  await expect(page.getByRole('button', { name: t('storage.cleanup.open') })).toBeDisabled();
  await expect(page.getByText(t('storage.managerLimitedHint'))).toBeVisible();
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

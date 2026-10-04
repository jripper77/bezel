// "When the computer shuts down" under Screen › Settings, in demo mode, in
// pt-BR and en, light and dark (D-2026-10-03-power-off-standby-2, -4, -6):
// four choices in a radio group the keyboard moves along without choosing,
// each explained by its "?" in a popover (Esc and a click outside close it,
// the focus goes back), an option the screen does not offer disabled with
// the reason; choosing opens an in-app dialog that says what happens at
// shutdown and exactly what is written to the screen, and nothing is
// written until it is confirmed (the demo shows every plan B written on the
// page); `keep` undoes another choice, and `keep` again sends nothing. The
// card album frames a photo in the shape the screen stands in, horizontal
// and vertical alike, before sending it, and removes a photo only after a
// confirmation that names it. No console errors, no serious or critical
// accessibility violations.
import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';
import { DEMO_LET_GO_EVENT } from '../../src/bridge.js';

const SCREEN = '/dev/ttyACM1';
const CHOICES = ['keep', 'off', 'video', 'album'];
const PHOTO = 'Praia do Forte.jpg';

const toast = (page) => page.locator('#toast');

/** Every plan B the demo wrote to a screen so far. */
async function written(page) {
  return JSON.parse((await page.locator('html').getAttribute('data-demo-standby')) ?? '[]');
}

/** Opens the studio in `scenario` and its Screen tab, whose Settings show the section. */
async function openSettings(page, t, scenario) {
  await page.goto(`/index.html?demo=${scenario}`);
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: t('library.screen') }).click();
  await expect(page.getByRole('tab', { name: t('screen.settingsTab') })).toHaveAttribute('aria-selected', 'true');
  const section = page.getByRole('region', { name: t('standby.title') });
  await expect(section.getByRole('radiogroup', { name: t('standby.title') })).toBeVisible();
  return section;
}

/** Lets the photo upload the demo holds (a page opened with `&hold`) go on, now or when it comes. */
async function letGo(page) {
  await page.evaluate((name) => window.dispatchEvent(new Event(name)), DEMO_LET_GO_EVENT);
}

/** The radio of `choice`. */
const radio = (section, t, choice) => section.getByRole('radio', { name: t(`standby.choice.${choice}`), exact: true });

/** "About “<choice>”": the name of a choice's "?" and of its popover. */
const aboutOf = (t, choice) => t('standby.helpFor', { choice: t(`standby.choice.${choice}`) });

/** The text of `count` minutes. */
const minutes = (t, count) => (count === 1 ? t('standby.minuteOne') : t('standby.minutes', { count }));

/** The photos the album dialog lists. */
const photosIn = (album, t) => album.getByRole('list', { name: t('standby.album.photos') }).getByRole('listitem');

/** The width over the height of an element's box, and of its picture's own size. */
async function ratios(frame) {
  const box = await frame.boundingBox();
  const natural = await frame.locator('img').evaluate((img) => img.naturalWidth / img.naturalHeight);
  return { box: box.width / box.height, natural };
}

test.describe('standby', () => {
  test('four choices, each explained', async ({ page, t }) => {
    const errors = watchErrors(page);
    const section = await openSettings(page, t, 'turing88');
    await expect(section).toContainText('Turing Smart Screen 8.8"');
    const radios = section.getByRole('radiogroup', { name: t('standby.title') }).getByRole('radio');
    await expect(radios).toHaveCount(4);
    for (const [i, choice] of CHOICES.entries()) {
      await expect(radios.nth(i)).toHaveAccessibleName(t(`standby.choice.${choice}`));
      await expect(radios.nth(i)).toBeEnabled();
    }
    await expect(radio(section, t, 'keep')).toBeChecked();
    for (const choice of CHOICES.slice(1)) await expect(radio(section, t, choice)).not.toBeChecked();

    // Each "?" opens its explanation in a popover, not on the page; Esc
    // closes it and gives the focus back to the "?".
    for (const choice of CHOICES) {
      const help = section.getByRole('button', { name: aboutOf(t, choice) });
      const popover = section.getByRole('group', { name: aboutOf(t, choice) });
      await expect(popover).toBeHidden();
      await expect(section.getByText(t(`standby.explain.${choice}`))).toBeHidden();
      await help.click();
      await expect(help).toHaveAttribute('aria-expanded', 'true');
      await expect(popover).toBeVisible();
      await expect(popover).toBeFocused();
      await expect(popover).toContainText(t(`standby.explain.${choice}`));
      if (choice === 'video') await expectAccessible(page);
      await page.keyboard.press('Escape');
      await expect(popover).toBeHidden();
      await expect(help).toHaveAttribute('aria-expanded', 'false');
      await expect(help).toBeFocused();
    }
    // One at a time; a click outside closes it, and the section's own "?" says what it is about.
    const about = section.getByRole('button', { name: t('standby.help') });
    await section.getByRole('button', { name: aboutOf(t, 'album') }).click();
    await expect(section.getByRole('group', { name: aboutOf(t, 'album') })).toBeVisible();
    await about.click();
    await expect(section.getByRole('group', { name: aboutOf(t, 'album') })).toBeHidden();
    await expect(section.getByRole('group', { name: t('standby.help') })).toContainText(t('standby.helpText'));
    await section.getByRole('heading', { name: t('standby.title') }).click();
    await expect(section.getByRole('group', { name: t('standby.help') })).toBeHidden();
    await expect(about).toBeFocused();
    await expect(page.getByRole('dialog')).toHaveCount(0);

    // The arrows move the focus along the group without choosing; Home and End go to the ends.
    await radio(section, t, 'keep').focus();
    await page.keyboard.press('ArrowDown');
    await expect(radio(section, t, 'off')).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await expect(radio(section, t, 'video')).toBeFocused();
    await page.keyboard.press('End');
    await expect(radio(section, t, 'album')).toBeFocused();
    await page.keyboard.press('ArrowDown');
    await expect(radio(section, t, 'keep')).toBeFocused();
    await page.keyboard.press('ArrowUp');
    await expect(radio(section, t, 'album')).toBeFocused();
    await page.keyboard.press('Home');
    await expect(radio(section, t, 'keep')).toBeFocused();
    await expect(radio(section, t, 'keep')).toBeChecked();
    await expect(radio(section, t, 'off')).not.toBeChecked();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    // The group is one stop of Tab: it leaves to the next "?".
    await page.keyboard.press('Tab');
    await expect(section.getByRole('button', { name: aboutOf(t, 'keep') })).toBeFocused();
    await expectAccessible(page);
    expect(await written(page)).toEqual([]);

    // A screen without a card: the album is disabled and says why, and choosing it does nothing.
    const noCard = await openSettings(page, t, 'noCard');
    const album = radio(noCard, t, 'album');
    await expect(album).toBeDisabled();
    await expect(album).toHaveAccessibleDescription(t('standby.reason.noCard'));
    await expect(noCard).toContainText(t('standby.reason.noCard'));
    await expect(radio(noCard, t, 'video')).toBeEnabled();
    await album.click({ force: true });
    await radio(noCard, t, 'video').focus();
    await page.keyboard.press('ArrowDown');
    await expect(album).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(radio(noCard, t, 'keep')).toBeChecked();
    await expectAccessible(page);
    expect(await written(page)).toEqual([]);
    expect(errors).toEqual([]);
  });

  test('plan B asks first', async ({ page, t }) => {
    const errors = watchErrors(page);
    const section = await openSettings(page, t, 'turing88');
    const off = radio(section, t, 'off');

    // Choosing opens the dialog that says what is written; the minutes start at 5.
    await off.click();
    const dialog = page.getByRole('dialog', { name: t('standby.off.title') });
    await expect(dialog).toBeVisible();
    const select = dialog.getByRole('combobox', { name: t('standby.off.minutes') });
    await expect(select).toBeFocused();
    await expect(select).toHaveValue('5');
    await expect(dialog).toContainText(t('standby.atShutdown.off'));
    await expect(dialog).toContainText(t('standby.written.startBoot'));
    await expect(dialog).toContainText(t('standby.written.sleep', { minutes: minutes(t, 5) }));
    await expect(dialog).toContainText(t('standby.written.brightness'));
    // The timer is explained by its "?", in the dialog; Esc closes only the popover.
    const timerHelp = dialog.getByRole('button', { name: t('standby.off.minutesHelp') });
    await timerHelp.click();
    await expect(dialog.getByRole('group', { name: t('standby.off.minutesHelp') })).toContainText(t('standby.off.minutesExplain'));
    await expectAccessible(page);
    await page.keyboard.press('Escape');
    await expect(dialog.getByRole('group', { name: t('standby.off.minutesHelp') })).toBeHidden();
    await expect(timerHelp).toBeFocused();
    await expect(dialog).toBeVisible();

    // Esc and Cancel write nothing: the choice stays, the focus goes back.
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(off).toBeFocused();
    await expect(radio(section, t, 'keep')).toBeChecked();
    await page.keyboard.press('Space');
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: t('dialog.cancel') }).click();
    await expect(dialog).toHaveCount(0);
    await expect(off).not.toBeChecked();
    expect(await written(page)).toEqual([]);

    // Other minutes, said before writing; then written, and shown under the choice.
    await off.press('Enter');
    await select.selectOption('3');
    await expect(dialog).toContainText(t('standby.written.sleep', { minutes: minutes(t, 3) }));
    await dialog.getByRole('button', { name: t('standby.write') }).click();
    await expect(dialog).toHaveCount(0);
    await expect(toast(page)).toHaveText(t('standby.saved', { choice: t('standby.choice.off') }));
    await expect(off).toBeChecked();
    await expect(off).toBeFocused();
    await expect(off).toHaveAccessibleDescription(t('standby.detail.off', { minutes: minutes(t, 3) }));
    await expect(radio(section, t, 'keep')).not.toBeChecked();
    expect(await written(page)).toEqual([{ screen: SCREEN, choice: 'off', sleepMinutes: 3, file: null, startMode: 0 }]);

    // A video: the dialog lists the screen's videos by medium and names the one that plays.
    await radio(section, t, 'video').click();
    const videos = page.getByRole('dialog', { name: t('standby.video.title') });
    const internal = videos.getByRole('group', { name: t('storage.medium.internal') });
    const card = videos.getByRole('group', { name: t('storage.medium.sd') });
    await expect(internal.getByRole('radio', { name: /^amd_90\.mp4/ })).toBeChecked();
    await expect(card.getByRole('radio', { name: /^chuva\.mp4/ })).not.toBeChecked();
    await expect(videos).toContainText(t('standby.atShutdown.video', { name: 'amd_90.mp4' }));
    await card.getByRole('radio', { name: /^chuva\.mp4/ }).check();
    await expect(videos).toContainText(t('standby.atShutdown.video', { name: 'chuva.mp4' }));
    await expect(videos).toContainText(t('standby.written.startVideo'));
    await expect(videos).toContainText(t('standby.written.sleepOff'));
    await expectAccessible(page);
    await page.keyboard.press('Escape');
    await expect(videos).toHaveCount(0);
    expect(await written(page)).toHaveLength(1);
    await radio(section, t, 'video').click();
    await card.getByRole('radio', { name: /^chuva\.mp4/ }).check();
    await videos.getByRole('button', { name: t('standby.write') }).click();
    await expect(radio(section, t, 'video')).toBeChecked();
    await expect(radio(section, t, 'video')).toHaveAccessibleDescription(t('standby.detail.video.sd', { name: 'chuva.mp4' }));
    expect((await written(page)).at(-1)).toEqual({ screen: SCREEN, choice: 'video', sleepMinutes: 0, file: 'sd/video/chuva.mp4', startMode: 2 });
    expect(errors).toEqual([]);
  });

  test('keep undoes', async ({ page, t }) => {
    const errors = watchErrors(page);
    const section = await openSettings(page, t, 'standbyOff');
    const keep = radio(section, t, 'keep');
    const off = radio(section, t, 'off');
    await expect(off).toBeChecked();
    await expect(off).toHaveAccessibleDescription(t('standby.detail.off', { minutes: minutes(t, 5) }));

    // Keep, after the dialog that says the timer goes off and the boot media is back.
    await keep.click();
    const dialog = page.getByRole('dialog', { name: t('standby.keep.title') });
    await expect(dialog).toContainText(t('standby.atShutdown.keep'));
    await expect(dialog).toContainText(t('standby.written.startBoot'));
    await expect(dialog).toContainText(t('standby.written.sleepOff'));
    await expect(dialog.getByRole('button', { name: t('standby.write') })).toBeFocused();
    await expectAccessible(page);
    await page.keyboard.press('Escape');
    await expect(off).toBeChecked();
    expect(await written(page)).toEqual([]);
    await keep.click();
    await dialog.getByRole('button', { name: t('standby.write') }).click();
    await expect(toast(page)).toHaveText(t('standby.saved', { choice: t('standby.choice.keep') }));
    await expect(keep).toBeChecked();
    await expect(off).not.toBeChecked();
    await expect(off).not.toHaveAttribute('aria-describedby', /.+/);
    expect(await written(page)).toEqual([{ screen: SCREEN, choice: 'keep', sleepMinutes: 0, file: null, startMode: 0 }]);

    // Keep again sends nothing and asks nothing.
    await keep.click();
    await keep.press('Space');
    await expect(page.getByRole('dialog')).toHaveCount(0);
    expect(await written(page)).toHaveLength(1);

    // The choice is the screen's: the tab shown again reads it back.
    await page.getByRole('tab', { name: t('library.themes') }).click();
    await page.getByRole('tab', { name: t('library.screen') }).click();
    await expect(keep).toBeChecked();
    await expectAccessible(page);
    expect(errors).toEqual([]);
  });

  test('album: vertical and horizontal', async ({ page, t }) => {
    const errors = watchErrors(page);
    // Each photo's upload waits until the test lets it go.
    const section = await openSettings(page, t, 'turing88&hold');

    // Choosing the album opens its manager: empty, it cannot be used yet.
    await radio(section, t, 'album').click();
    const album = page.getByRole('dialog', { name: t('standby.album.title') });
    await expect(photosIn(album, t)).toHaveText([t('standby.album.empty')]);
    const use = album.getByRole('button', { name: t('standby.album.use') });
    await expect(use).toBeDisabled();
    await expect(album).toContainText(t('standby.album.needsPhoto'));
    await expect(album).toContainText(t('standby.written.startAlbum'));
    const explain = album.getByRole('button', { name: t('standby.album.help') });
    await explain.click();
    await expect(album.getByRole('group', { name: t('standby.album.help') })).toContainText(t('standby.album.explain'));
    await page.keyboard.press('Escape');
    await expect(album).toBeVisible();

    // The 8.8" stands horizontally: the phone photo is framed 4:1, Fill first.
    const addButton = album.getByRole('button', { name: t('standby.album.add') });
    await addButton.click();
    const add = page.getByRole('dialog', { name: t('standby.add.title', { name: PHOTO }) });
    const frame = add.locator('.album-frame');
    await expect(frame).toHaveAttribute('data-axis', 'horizontal');
    await expect(frame).toHaveAttribute('data-state', 'ready');
    await expect(add.getByRole('img', { name: t('standby.add.previewAlt', { name: PHOTO }) })).toBeVisible();
    await expect(add).toContainText(t('standby.add.onHorizontal'));
    let shape = await ratios(frame);
    expect(shape.box).toBeCloseTo(4, 1);
    expect(shape.natural).toBeCloseTo(4, 5);
    const fill = add.getByRole('button', { name: t('framing.cover') });
    const fit = add.getByRole('button', { name: t('framing.contain') });
    await expect(fill).toHaveAttribute('aria-pressed', 'true');
    await expect(fill).toBeFocused();
    await fit.click();
    await expect(fit).toHaveAttribute('aria-pressed', 'true');
    await expect(frame).toHaveAttribute('data-fit', 'contain');
    await expect(frame).toHaveAttribute('data-state', 'ready');
    const name = add.getByRole('textbox', { name: t('standby.add.name') });
    await expect(name).toHaveValue('praia_do_forte.png');
    await expect(add).toContainText(t('standby.add.sendsTo', { path: 'sd/image/praia_do_forte.png' }));
    await expectAccessible(page);
    await add.getByRole('button', { name: t('standby.add.send') }).click();
    // While it sends, nothing closes the dialog: not Esc, not Cancel nor the X (review W1).
    await expect(add).toContainText(t('standby.add.sending'));
    await page.keyboard.press('Escape');
    await expect(add).toBeVisible();
    await expect(add.getByRole('button', { name: t('dialog.cancel') })).toBeDisabled();
    await expect(add.getByRole('button', { name: t('dialog.close') })).toBeDisabled();
    await letGo(page);
    await expect(add).toHaveCount(0);
    await expect(toast(page)).toHaveText(t('standby.add.added', { name: 'praia_do_forte.png' }));
    await expect(photosIn(album, t)).toHaveCount(1);
    await expect(photosIn(album, t)).toContainText('praia_do_forte.png');
    await expect(addButton).toBeFocused();
    expect(await written(page)).toEqual([], 'a photo is not the choice');

    // Used: the plan B is written, and the album is the choice.
    await use.click();
    await expect(album).toHaveCount(0);
    await expect(radio(section, t, 'album')).toBeChecked();
    expect(await written(page)).toEqual([{ screen: SCREEN, choice: 'album', sleepMinutes: 0, file: null, startMode: 1 }]);

    // A vertical theme for the screen: it stands vertically now, and the photo is framed 1:4.
    await page.getByRole('tab', { name: t('library.themes') }).click();
    await page.locator('#theme-new-vertical').click();
    await expect(page.locator('#orient-vertical')).toHaveAttribute('aria-pressed', 'true');
    await page.getByRole('tab', { name: t('library.screen') }).click();
    await section.getByRole('button', { name: t('standby.manageAlbum') }).click();
    await expect(album.getByRole('button', { name: t('standby.album.use') })).toHaveCount(0);
    await addButton.click();
    await expect(frame).toHaveAttribute('data-axis', 'vertical');
    await expect(frame).toHaveAttribute('data-state', 'ready');
    await expect(add).toContainText(t('standby.add.onVertical'));
    shape = await ratios(frame);
    expect(shape.box).toBeCloseTo(0.25, 1);
    expect(shape.natural).toBeCloseTo(0.25, 5);
    // The same name: replacing it is said, and the button says so.
    await expect(add).toContainText(t('standby.add.replaces', { name: 'praia_do_forte.png' }));
    await expectAccessible(page);
    // Enter in the name never replaces: the danger button takes its own click.
    await name.press('Enter');
    await expect(add).toBeVisible();
    await expect(add).not.toContainText(t('standby.add.sending'));
    await expect(add.getByRole('button', { name: t('standby.add.replace') })).toBeEnabled();
    await name.fill('praia_em_pe.png');
    await expect(add).not.toContainText(t('standby.add.replaces', { name: 'praia_do_forte.png' }));
    await name.fill('praia_em_pe.jpg');
    await expect(add.getByRole('button', { name: t('standby.add.send') })).toBeDisabled();
    await name.fill('praia_em_pe.png');
    await letGo(page);
    await name.press('Enter');
    await expect(add).toHaveCount(0);
    await expect(photosIn(album, t)).toHaveCount(2);

    // Replacing the namesake is its own confirmation, the danger button (D-2026-10-03-power-off-standby-4 (3)).
    await addButton.click();
    await expect(frame).toHaveAttribute('data-state', 'ready');
    const replace = add.getByRole('button', { name: t('standby.add.replace') });
    await expect(replace).toHaveClass(/danger-button/);
    await letGo(page);
    await replace.click();
    await expect(add).toHaveCount(0);
    await expect(toast(page)).toHaveText(t('standby.add.added', { name: 'praia_do_forte.png' }));
    await expect(photosIn(album, t)).toHaveCount(2);
    await page.keyboard.press('Escape');
    await expect(album).toHaveCount(0);
    expect(await written(page)).toHaveLength(1);
    expect(errors).toEqual([]);
  });

  test('album: remove asks first', async ({ page, t }) => {
    const errors = watchErrors(page);
    const section = await openSettings(page, t, 'album');
    await expect(radio(section, t, 'album')).toBeChecked();
    // The album as the choice: its manager, where nothing is written.
    await radio(section, t, 'album').click();
    const album = page.getByRole('dialog', { name: t('standby.album.title') });
    await expect(album.getByRole('button', { name: t('standby.album.use') })).toHaveCount(0);
    await album.getByRole('button', { name: t('dialog.close') }).first().click();
    await expect(album).toHaveCount(0);
    await section.getByRole('button', { name: t('standby.manageAlbum') }).click();
    const photos = photosIn(album, t);
    await expect(photos).toHaveCount(2);
    await expect(photos.nth(0)).toContainText('img_0042.jpg');
    await expect(photos.nth(1)).toContainText('praia.png');
    // Bezel's photo has its thumbnail; the vendor's shows by its name.
    await expect(photos.nth(1).locator('.album-thumb')).toHaveClass(/has-picture/);
    await expect(photos.nth(0).locator('.album-thumb')).not.toHaveClass(/has-picture/);
    await expectAccessible(page);

    // Remove asks first and names the photo; Esc keeps it, and the album stays open.
    const remove = album.getByRole('button', { name: t('standby.album.remove', { name: 'praia.png' }) });
    await remove.click();
    const confirm = page.getByRole('dialog', { name: t('standby.album.removeTitle', { name: 'praia.png' }) });
    await expect(confirm).toContainText(t('storage.confirmDelete', { name: 'praia.png', place: t('storage.from.sd') }));
    await expect(confirm.getByRole('button', { name: t('dialog.cancel') })).toBeFocused();
    await expectAccessible(page);
    await page.keyboard.press('Escape');
    await expect(confirm).toHaveCount(0);
    await expect(album).toBeVisible();
    await expect(remove).toBeFocused();
    await expect(photos).toHaveCount(2);
    await remove.click();
    await confirm.getByRole('button', { name: t('dialog.cancel') }).click();
    await expect(photos).toHaveCount(2);

    // Confirmed: deleted from the card, the next photo's Remove takes the focus.
    await remove.click();
    await confirm.getByRole('button', { name: t('standby.album.removeAction') }).click();
    await expect(toast(page)).toHaveText(t('standby.album.removed', { name: 'praia.png' }));
    await expect(photos).toHaveCount(1);
    await expect(album).not.toContainText('praia.png');
    await expect(album.getByRole('button', { name: t('standby.album.remove', { name: 'img_0042.jpg' }) })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(album).toHaveCount(0);

    // The Storage tab lists the card again: the photo is gone there too.
    await page.getByRole('tab', { name: t('screen.storageTab') }).click();
    const card = page.getByRole('region', { name: t('storage.medium.sd') });
    await expect(card.getByRole('option').filter({ hasText: 'img_0042.jpg' })).toHaveCount(1);
    await expect(card.getByRole('option').filter({ hasText: 'praia.png' })).toHaveCount(0);
    expect(await written(page)).toEqual([]);
    expect(errors).toEqual([]);
  });
});

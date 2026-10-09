// The editor in demo mode, in pt-BR and en, light and dark: it loads without
// console errors or serious accessibility violations, widgets and sensors
// drag onto the canvas, keyboard edits undo (arrows on tabs only switch
// tabs), unsaved edits are asked about before they could be lost (closing
// the window, quitting from the tray), the screen turns between vertical and
// horizontal, imports list what had no equivalent, the preferences switch
// the language and set the sensors, the sensor list tells the app what it
// shows, a denied port shows the udev command (in a dialog and on the
// stage), a panel in desktop mode goes back only after a dialog, a hung
// screen is restarted after one, an animated GIF moves in the preview by
// itself and a live screen that drops is connected again, saying so.
import { test, expect, watchErrors, expectAccessible, dragTo, literally, prefixOf } from './helpers.mjs';
import { translator } from '../../src/i18n/index.js';

const layers = (page) => page.locator('#layer-list .layer-row');
const addWidget = (t, widget) => t('library.addWidget', { name: t(`widget.${widget}`) });

test('the editor opens the demo theme', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await expect(page.locator('html')).toHaveAttribute('lang', lang);
  await expect(page.locator('#screen-select')).toContainText('Turing Smart Screen 8.8"');
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  await expect(page.locator('#status-render')).toContainText(prefixOf(t, 'status.render', ['ms']));
  await expect(page.getByRole('heading', { name: t('inspector.theme') })).toBeVisible();
  const box = await page.locator('#canvas-box').boundingBox();
  expect(Math.round(box.height / box.width)).toBe(4);
  await expectAccessible(page);
  for (const tab of ['sensors', 'layers', 'themes', 'media', 'screen']) {
    await page.getByRole('tab', { name: t(`library.${tab}`) }).click();
    await expectAccessible(page);
  }
  expect(errors).toEqual([]);
});

test('every panel reads in one language, with no key or placeholder showing', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=desktop');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const leftovers = /\b(?:storage|error|importWarning|inspector|library|toast|top|status|screen|prefs|desktop|udev|sensor|widget|category|themes|media|layers|unsaved|restart)\.[a-zA-Z]|\{[a-z]+\}|undefined|NaN/;
  const tabs = ['widgets', 'sensors', 'layers', 'themes', 'media', 'screen'];
  for (const tab of tabs) {
    await page.getByRole('tab', { name: t(`library.${tab}`) }).click();
    expect(await page.locator('body').innerText()).not.toMatch(leftovers);
  }
  await page.getByRole('tab', { name: t('screen.storageTab') }).click();
  await expect(page.getByRole('region', { name: t('storage.medium.internal') })).toBeVisible();
  expect(await page.locator('body').innerText()).not.toMatch(leftovers);
  await page.getByRole('button', { name: t('top.preferences') }).click();
  await expect(page.getByRole('dialog', { name: t('prefs.title') })).toBeVisible();
  expect(await page.locator('body').innerText()).not.toMatch(leftovers);
  expect(errors).toEqual([]);
});

test('drag a widget onto the canvas', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await dragTo(page, page.getByRole('button', { name: addWidget(t, 'bar') }), page.locator('#canvas-box'), { x: 0.5, y: 0.6 });
  await expect(page.locator('.sel-box')).toHaveCount(1);
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));
  await expect(page.locator('#inspector')).toContainText(t('inspector.position'));
  await expect(page.locator('#inspector').getByRole('textbox', { name: t('inspector.name'), exact: true })).toHaveValue(t('widget.bar'));
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await expect(layers(page)).toHaveCount(4);
  await expectAccessible(page);

  await page.getByRole('tab', { name: t('library.sensors') }).click();
  const gpuTemperature = t('sensor.gpu.temperature');
  await page.getByRole('searchbox', { name: t('library.searchSensors') }).fill(gpuTemperature.toLowerCase());
  await dragTo(page, page.getByRole('button', { name: literally(gpuTemperature) }), page.locator('#canvas-box'), { x: 0.3, y: 0.8 });
  await expect(page.locator('#inspector').getByRole('combobox', { name: t('inspector.sensor'), exact: true })).toHaveValue('gpu.temperature');
  await expect(page.locator('#inspector').getByRole('textbox', { name: t('inspector.name'), exact: true })).toHaveValue(gpuTemperature);
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await expect(layers(page)).toHaveCount(5);

  // A drop outside the canvas adds nothing; a plain click adds at the center.
  await page.getByRole('tab', { name: t('library.widgets') }).click();
  await dragTo(page, page.getByRole('button', { name: addWidget(t, 'text') }), page.locator('#inspector'));
  await page.getByRole('button', { name: addWidget(t, 'ring') }).click();
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await expect(layers(page)).toHaveCount(6);
  expect(errors).toEqual([]);
});

test('keyboard move and undo', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await page.getByRole('button', { name: /^CPU/ }).click();
  const x = page.getByRole('spinbutton', { name: t('inspector.x'), exact: true });
  await expect(x).toHaveValue('90');

  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.press('ArrowRight');
  await expect(x).toHaveValue('101');
  await page.keyboard.press('Control+z');
  await expect(x).toHaveValue('100');
  await page.keyboard.press('Control+z');
  await expect(x).toHaveValue('90');
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  await page.keyboard.press('Control+Shift+z');
  await expect(x).toHaveValue('100');

  // Typing in a field does not nudge; the field edit is one undo step.
  await x.fill('120');
  await x.press('Enter');
  await x.press('ArrowLeft');
  await expect(x).toHaveValue('120');
  await page.getByRole('button', { name: t('top.undo') }).click();
  await expect(x).toHaveValue('100');

  // Duplicate (named in the UI's language), then delete, then save.
  await page.getByRole('button', { name: /^CPU/ }).click();
  await page.keyboard.press('Control+d');
  await expect(layers(page)).toHaveCount(4);
  await expect(page.locator('#layer-list')).toContainText(t('layers.copyOf', { name: 'CPU' }));
  await page.keyboard.press('Delete');
  await expect(layers(page)).toHaveCount(3);
  await page.keyboard.press('Control+s');
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  await expect(page.locator('#toast')).toHaveText(t('toast.saved'));
  expect(errors).toEqual([]);
});

test('arrow keys on tabs switch tabs and leave the element alone', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await page.getByRole('button', { name: /^CPU/ }).click();
  const x = page.getByRole('spinbutton', { name: t('inspector.x'), exact: true });
  await expect(x).toHaveValue('90');

  await page.getByRole('tab', { name: t('library.layers') }).focus();
  await page.keyboard.press('ArrowRight');
  const themes = page.getByRole('tab', { name: t('library.themes') });
  await expect(themes).toBeFocused();
  await expect(themes).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('ArrowRight');
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('tab', { name: t('library.screen') })).toHaveAttribute('aria-selected', 'true');

  // The Screen panel's subtabs as well.
  const settings = page.getByRole('tab', { name: t('screen.settingsTab') });
  await settings.focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByRole('tab', { name: t('screen.storageTab') })).toBeFocused();
  await page.keyboard.press('ArrowLeft');
  await expect(settings).toBeFocused();

  await expect(x).toHaveValue('90');
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  await expect(page.getByRole('button', { name: t('top.undo') })).toBeDisabled();
  expect(errors).toEqual([]);
});

test('unsaved edits are not lost to another theme', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await page.getByRole('button', { name: /^CPU/ }).click();
  await page.keyboard.press('ArrowRight');
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));

  await page.getByRole('tab', { name: t('library.themes') }).click();
  const newHorizontal = page.getByRole('button', { name: t('themes.newHorizontal') });
  const dialog = page.getByRole('dialog', { name: t('unsaved.title') });
  await newHorizontal.click();
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText(t('unsaved.body', { name: 'Demo' }));
  await expect(dialog.getByRole('button', { name: t('unsaved.save') })).toBeFocused();
  await expectAccessible(page);

  // Cancel and Esc keep the edits.
  await dialog.getByRole('button', { name: t('dialog.cancel') }).click();
  await expect(dialog).toHaveCount(0);
  await expect(newHorizontal).toBeFocused();
  await newHorizontal.click();
  await expect(dialog).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));

  // Discard lets the new theme in.
  await newHorizontal.click();
  await dialog.getByRole('button', { name: t('unsaved.discard') }).click();
  await expect(page.locator('#theme-name')).toHaveValue(t('themes.untitled'));
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));

  // Save keeps them, then the chosen theme opens.
  await page.locator('#theme-name').fill('Rascunho');
  await page.locator('#theme-name').press('Tab');
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));
  await page.locator('#theme-grid .theme-card').first().click();
  await dialog.getByRole('button', { name: t('unsaved.save') }).click();
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  await expect(page.locator('#theme-grid .theme-card')).toHaveCount(2);
  await expect(page.locator('#theme-grid')).toContainText('Rascunho');
  expect(errors).toEqual([]);
});

test('closing the window over unsaved edits asks first', async ({ page, t }) => {
  const errors = watchErrors(page);
  const closeButton = () => page.evaluate(() => window.dispatchEvent(new Event('bezel-demo-close')));
  const html = page.locator('html');
  const dialog = page.getByRole('dialog', { name: t('unsaved.title') });

  // Nothing unsaved: the window closes at once.
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await closeButton();
  await expect(html).toHaveAttribute('data-demo-window', 'closed');
  await expect(dialog).toHaveCount(0);

  await page.goto('/index.html?demo=turing88');
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await page.getByRole('button', { name: /^CPU/ }).click();
  await page.keyboard.press('Delete');
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));
  await closeButton();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: t('dialog.cancel') }).click();
  await expect(dialog).toHaveCount(0);
  expect(await html.getAttribute('data-demo-window')).toBeNull();
  await closeButton();
  await dialog.getByRole('button', { name: t('unsaved.discard') }).click();
  await expect(html).toHaveAttribute('data-demo-window', 'closed');
  expect(errors).toEqual([]);
});

test('quitting from the tray over unsaved edits shows the window and asks', async ({ page, t }) => {
  const errors = watchErrors(page);
  const html = page.locator('html');
  const quit = () => page.evaluate(() => window.dispatchEvent(new Event('bezel-demo-quit')));
  const dialog = page.getByRole('dialog', { name: t('unsaved.title') });

  // Nothing unsaved: the app ends at once.
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await quit();
  await expect(html).toHaveAttribute('data-demo-window', 'quit');
  await expect(dialog).toHaveCount(0);

  // Live and hidden in the tray with unsaved edits: Quit shows the window
  // and asks; Cancel keeps the app, Discard ends it.
  await page.goto('/index.html?demo=turing88');
  await page.getByRole('button', { name: t('top.preferences') }).click();
  await page.getByRole('switch', { name: t('prefs.lightOnClose') }).uncheck();
  await page.getByRole('button', { name: t('prefs.done') }).click();
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await page.getByRole('button', { name: /^CPU/ }).click();
  await page.keyboard.press('Delete');
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));
  await page.getByRole('switch').click({ force: true });
  await expect(page.getByRole('switch')).toBeChecked();
  await page.evaluate(() => window.dispatchEvent(new Event('bezel-demo-close')));
  await expect(html).toHaveAttribute('data-demo-window', 'hidden');
  await quit();
  await expect(html).toHaveAttribute('data-demo-window', 'open');
  await expect(dialog).toBeVisible();
  await expectAccessible(page);
  await dialog.getByRole('button', { name: t('dialog.cancel') }).click();
  await expect(html).toHaveAttribute('data-demo-window', 'open');
  await quit();
  await dialog.getByRole('button', { name: t('unsaved.discard') }).click();
  await expect(html).toHaveAttribute('data-demo-window', 'quit');
  expect(errors).toEqual([]);
});

test('without a screen, live mode explains itself', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=empty');
  await expect(page.locator('#screen-select')).toContainText(t('top.noScreen'));
  const connect = page.getByRole('region', { name: t('connect.title') });
  await expect(connect).toBeVisible();
  await expect(connect).toContainText(t('connect.devices.searching'));
  await page.getByRole('switch').click({ force: true });
  await expect(page.locator('#toast')).toHaveText(t('toast.noScreen'));
  await expect(page.getByRole('switch')).not.toBeChecked();
  await page.getByRole('tab', { name: t('library.screen') }).click();
  await expect(page.locator('#screen-panel')).toContainText(t('screen.none'));
  await expectAccessible(page);
  await page.goto('/index.html?demo=error');
  await expect(page.locator('#status-device')).toContainText('permission denied');
  await expect(page.getByRole('region', { name: t('connect.title') }).getByRole('alert')).toContainText('permission denied');
  expect(errors).toEqual([]);
});

test('live mode and start at login', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('switch').click({ force: true });
  await expect(page.getByRole('switch')).toBeChecked();
  await expect(page.locator('#status-device')).toHaveText(t('status.live'));
  await page.getByRole('tab', { name: t('library.screen') }).click();
  const autostart = page.getByRole('checkbox', { name: t('screen.autostart') });
  await autostart.check();
  await expect(autostart).toBeChecked();
  await page.getByRole('switch').click({ force: true });
  await expect(page.getByRole('switch')).not.toBeChecked();
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

async function expectInside(page, inner, outer) {
  const o = await page.locator(outer).boundingBox();
  for (const b of await page.locator(inner).all()) {
    const i = await b.boundingBox();
    expect(i.x).toBeGreaterThanOrEqual(o.x - 1);
    expect(i.y).toBeGreaterThanOrEqual(o.y - 1);
    expect(i.x + i.width).toBeLessThanOrEqual(o.x + o.width + 1);
    expect(i.y + i.height).toBeLessThanOrEqual(o.y + o.height + 1);
  }
}

test('switch between vertical and horizontal', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const group = page.getByRole('group', { name: t('top.orientation') });
  const vertical = group.getByRole('button', { name: t('axis.vertical'), exact: true });
  const horizontal = group.getByRole('button', { name: t('axis.horizontal'), exact: true });
  const turn = group.getByRole('button', { name: t('top.turn') });
  const orientation = page.locator('#inspector').getByRole('combobox', { name: t('inspector.orientation') });
  await expect(vertical).toHaveAttribute('aria-pressed', 'true');
  await expect(horizontal).toHaveAttribute('aria-pressed', 'false');
  await expect(turn).toHaveAttribute('aria-pressed', 'true');
  await expect(orientation).toHaveValue('reverse-portrait');

  await horizontal.click();
  await expect(horizontal).toHaveAttribute('aria-pressed', 'true');
  await expect(vertical).toHaveAttribute('aria-pressed', 'false');
  await expect(turn).toHaveAttribute('aria-pressed', 'true');
  await expect(orientation).toHaveValue('reverse-landscape');
  await expect(page.locator('#inspector')).toContainText(t('inspector.canvas', { width: 1920, height: 480 }));
  const wide = await page.locator('#canvas-box').boundingBox();
  expect(wide.width).toBeGreaterThan(wide.height);
  await page.keyboard.press('Control+a');
  await expect(page.locator('.sel-box')).toHaveCount(3);
  await expectInside(page, '.sel-box', '#canvas-box');
  await page.keyboard.press('Escape');
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));
  await expectAccessible(page);

  // One undo step brings the vertical layout back.
  await page.keyboard.press('Control+z');
  await expect(vertical).toHaveAttribute('aria-pressed', 'true');
  await expect(orientation).toHaveValue('reverse-portrait');
  const tall = await page.locator('#canvas-box').boundingBox();
  expect(tall.height).toBeGreaterThan(tall.width);
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  await expect(page.getByRole('button', { name: t('top.undo') })).toBeDisabled();

  // The 180° turn toggles, by mouse and by keyboard, and keeps the layout.
  await turn.click();
  await expect(turn).toHaveAttribute('aria-pressed', 'false');
  await expect(orientation).toHaveValue('portrait');
  await expect(page.locator('#inspector')).toContainText(t('inspector.canvas', { width: 480, height: 1920 }));
  await turn.focus();
  await page.keyboard.press('Space');
  await expect(turn).toHaveAttribute('aria-pressed', 'true');
  await expect(orientation).toHaveValue('reverse-portrait');

  // The inspector's select is the same command.
  await orientation.selectOption('landscape');
  await expect(horizontal).toHaveAttribute('aria-pressed', 'true');
  await expect(turn).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('#inspector')).toContainText(t('inspector.canvas', { width: 1920, height: 480 }));
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('new vertical and horizontal themes', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: t('library.themes') }).click();
  const cards = page.locator('#theme-grid .theme-card');
  await expect(cards).toHaveCount(1);
  await expect(cards.first()).toContainText(t('axis.vertical'));

  await page.getByRole('button', { name: t('themes.newHorizontal') }).click();
  await expect(page.locator('#theme-name')).toHaveValue(t('themes.untitled'));
  const topBar = page.getByRole('group', { name: t('top.orientation') });
  await expect(topBar.getByRole('button', { name: t('axis.horizontal'), exact: true })).toHaveAttribute('aria-pressed', 'true');
  const box = await page.locator('#canvas-box').boundingBox();
  expect(box.width).toBeGreaterThan(box.height);
  await page.keyboard.press('Control+s');
  await expect(cards).toHaveCount(2);
  await expect(cards.nth(1)).toContainText(t('axis.horizontal'));
  await expect(cards.nth(1)).toContainText('1920×480');

  await page.getByRole('button', { name: t('themes.newVertical') }).click();
  await expect(topBar.getByRole('button', { name: t('axis.vertical'), exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#inspector')).toContainText(t('inspector.canvas', { width: 480, height: 1920 }));
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('an import lists what had no equivalent', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: t('library.themes') }).click();
  await page.getByRole('button', { name: t('themes.import') }).click();
  await expect(page.locator('#theme-name')).toHaveValue('Imported');
  await expect(page.locator('#toast')).toHaveText(t('toast.importedWithWarnings', { count: 2 }));
  const report = page.getByRole('region', { name: t('import.title') });
  await expect(report).toBeVisible();
  await expect(report.getByRole('listitem')).toHaveCount(2);
  await expect(report).toContainText(t('importWarning.backplateLed'));
  await expect(report).toContainText(t('importWarning.cpuFanGuessed', { name: 'CPU.FAN_SPEED.TEXT' }));
  await expectAccessible(page);
  await report.getByRole('button', { name: t('import.dismiss') }).click();
  await expect(report).toHaveCount(0);
  await expect(page.getByRole('button', { name: t('themes.import') })).toBeFocused();
  expect(errors).toEqual([]);
});

test('the language follows the system until one is chosen', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  const html = page.locator('html');
  const other = lang === 'pt-BR' ? 'en' : 'pt-BR';
  const u = translator(other);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await expect(html).toHaveAttribute('lang', lang);
  await page.getByRole('button', { name: t('top.preferences') }).click();
  const dialog = page.getByRole('dialog', { name: t('prefs.title') });
  const language = dialog.getByRole('combobox', { name: t('prefs.language') });
  await expect(language).toBeFocused();
  await expect(language).toHaveValue('');
  await expect(language.locator('option:checked')).toHaveText(t('prefs.languageSystem', { language: t(`language.${lang}`) }));
  await expectAccessible(page);

  // Another language applies at once, the dialog included.
  await language.selectOption(other);
  await expect(html).toHaveAttribute('lang', other);
  await expect(page.getByRole('dialog', { name: u('prefs.title') })).toBeVisible();
  await expect(page.getByRole('tab', { name: u('library.sensors') })).toBeVisible();
  await expect(page.locator('#status-main')).toHaveText(u('status.saved'));
  await expect(page.locator('#inspector').getByRole('heading', { name: u('inspector.theme') })).toBeVisible();
  await expectAccessible(page);
  await page.keyboard.press('Escape');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByRole('button', { name: u('top.preferences') })).toBeFocused();

  // Back to the system's.
  await page.getByRole('button', { name: u('top.preferences') }).click();
  await page.getByRole('dialog').getByRole('combobox').selectOption('');
  await expect(html).toHaveAttribute('lang', lang);
  await page.getByRole('dialog').getByRole('button', { name: t('prefs.done') }).click();
  await expect(page.getByRole('tab', { name: t('library.sensors') })).toBeVisible();
  expect(errors).toEqual([]);
});

test('the ping target and the MangoHud folder are set in the preferences', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('button', { name: t('top.preferences') }).click();
  const dialog = page.getByRole('dialog', { name: t('prefs.title') });
  const host = dialog.getByRole('textbox', { name: t('prefs.pingHost') });
  await expect(host).toHaveValue('8.8.8.8');

  // A value that is no host is refused next to the field.
  await host.fill('-c 5 host');
  await host.press('Enter');
  await expect(dialog.getByRole('alert')).toHaveText(t('error.invalidHost', { host: '-c 5 host' }));
  await expect(host).toHaveAttribute('aria-invalid', 'true');
  await expect(host).toHaveValue('-c 5 host');
  await expect(host).toBeFocused();
  await expectAccessible(page);
  await host.fill('1.1.1.1');
  await host.press('Enter');
  await expect(page.locator('#toast')).toHaveText(t('prefs.sensorsSaved'));
  await expect(dialog.getByRole('alert')).toHaveCount(0);

  // The MangoHud folder: its own by default, a chosen one, and back.
  const folder = dialog.getByRole('group', { name: t('prefs.mangohud') });
  await expect(folder).toContainText(t('prefs.mangohudDefault'));
  await folder.getByRole('button', { name: t('prefs.mangohudChoose') }).click();
  await expect(folder).toContainText('/home/demo/mangohud');
  await folder.getByRole('button', { name: t('prefs.mangohudReset') }).click();
  await expect(folder).toContainText(t('prefs.mangohudDefault'));
  await expect(host).toHaveValue('1.1.1.1');
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

// The app measures what is shown: `net.ping` sends packets only while the
// theme or this list shows it (D-2026-09-30-release-polish-11).
test('the sensor list tells the app which sensors it shows', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const html = page.locator('html');
  await expect(html).toHaveAttribute('data-demo-sensors', '');
  await page.getByRole('tab', { name: t('library.sensors') }).click();
  await expect(html).toHaveAttribute('data-demo-sensors', /^cpu\.usage .*net\.down net\.up/);
  await page.getByRole('searchbox', { name: t('library.searchSensors') }).fill('net.');
  await expect(html).toHaveAttribute('data-demo-sensors', 'net.down net.up');
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await expect(html).toHaveAttribute('data-demo-sensors', '');
  expect(errors).toEqual([]);
});

test('a denied port shows the udev command to copy', async ({ page, context, t }) => {
  const errors = watchErrors(page);
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/index.html?demo=denied');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const live = page.getByRole('switch');
  await live.click({ force: true });
  const dialog = page.getByRole('dialog', { name: t('udev.title') });
  const denied = t('error.accessDenied', { address: '/dev/ttyACM1', reason: 'Permission denied (os error 13)' });
  await expect(dialog).toContainText(denied);
  const command = dialog.getByRole('group', { name: t('udev.command') });
  await expect(command).toContainText('sudo install -m 644 /home/demo/.cache/io.github.slipalison.bezel/60-bezel.rules /etc/udev/rules.d/60-bezel.rules');
  await expect(dialog).toContainText(t('udev.never'));
  const copy = command.getByRole('button', { name: t('udev.copy') });
  await expect(copy).toBeFocused();
  await expectAccessible(page);
  await copy.click();
  await expect(page.locator('#toast')).toHaveText(t('udev.copied'));
  expect(await page.evaluate(() => navigator.clipboard.readText())).toMatch(/^sudo install -m 644 .* && sudo udevadm trigger$/);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(live).not.toBeChecked();

  // The stage says how to connect it, with the same command.
  const connect = page.getByRole('region', { name: t('connect.title') });
  await expect(connect).toContainText(denied);
  await expect(connect.getByRole('group', { name: t('udev.command') })).toContainText('sudo install -m 644');

  // The storage tab says the same, with the command.
  await page.getByRole('tab', { name: t('library.screen') }).click();
  await page.getByRole('tab', { name: t('screen.storageTab') }).click();
  const storage = page.locator('#storage-panel');
  await expect(storage.getByRole('alert')).toContainText(denied);
  await expect(storage.getByRole('group', { name: t('udev.command') })).toBeVisible();
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('a panel in desktop mode is labelled and switched back only after a dialog', async ({ page, t }) => {
  const errors = watchErrors(page);
  const key = 'hid:/dev/hidraw7';
  await page.goto('/index.html?demo=desktop');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: t('library.screen') }).click();
  const panel = page.getByRole('group', { name: t('desktop.cardLabel', { address: key }) });
  await expect(panel).toContainText(t('desktop.notValidated'));
  await expect(panel).toContainText('Turing 8.8" V1.x (USB)');
  await expectAccessible(page);

  const leave = panel.getByRole('button', { name: t('desktop.leave') });
  await leave.click();
  const dialog = page.getByRole('dialog', { name: t('desktop.confirmTitle', { address: key }) });
  await expect(dialog).toContainText(t('desktop.confirmRisk'));
  // Not validated: the safe answer has the focus.
  await expect(dialog.getByRole('button', { name: t('dialog.cancel') })).toBeFocused();
  await expectAccessible(page);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(leave).toBeFocused();
  await expect(panel).toBeVisible();

  await leave.click();
  await dialog.getByRole('button', { name: t('desktop.confirmAction') }).click();
  await expect(page.locator('#toast')).toHaveText(t('desktop.doneModel', { model: 'Turing 8.8" V1.x (USB)' }));
  await expect(panel).toHaveCount(0);
  await expect(page.locator('#screen-select option')).toHaveCount(2);
  expect(errors).toEqual([]);
});

test('a hung screen offers the restart, which asks first and brings it back', async ({ page, t }) => {
  // D-2026-09-30-release-polish-13: the screen stopped reading what Bezel
  // sent; live mode and uploads stop until it is restarted, no replug.
  const errors = watchErrors(page);
  const name = 'Turing Smart Screen 8.8"';
  await page.goto('/index.html?demo=hung');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('switch').click({ force: true });
  await expect(page.locator('#toast')).toHaveText(t('toast.liveStopped', { message: t('error.hung') }));
  await expect(page.getByRole('switch')).not.toBeChecked();
  await page.getByRole('tab', { name: t('library.screen') }).click();
  const card = page.getByRole('region', { name });
  await expect(card).toContainText(t('restart.hung'));
  await expect(card.getByRole('button', { name: t('screen.restart') })).toBeVisible();
  await expectAccessible(page);

  // An upload stops too, and its error offers the restart.
  await page.getByRole('tab', { name: t('screen.storageTab') }).click();
  const internal = page.getByRole('region', { name: t('storage.medium.internal') });
  await internal.getByRole('button', { name: t('storage.choose') }).click();
  await page.getByRole('dialog').getByRole('button', { name: t('storage.confirmUploadAction'), exact: true }).click();
  const failed = page.getByRole('region', { name: t('storage.errorTitle') });
  await expect(failed).toContainText(t('error.hung'), { timeout: 15_000 });

  // The restart asks first and says what stops; Esc keeps everything.
  const restart = failed.getByRole('button', { name: t('screen.restart') });
  await restart.click();
  const dialog = page.getByRole('dialog', { name: t('restart.confirmTitle', { name }) });
  await expect(dialog).toContainText(t('restart.confirmBody'));
  await expectAccessible(page);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(restart).toBeFocused();

  await restart.click();
  await dialog.getByRole('button', { name: t('restart.confirmAction') }).click();
  await expect(page.locator('#toast')).toHaveText(t('restart.done'));
  await expect(failed).toHaveCount(0);
  await page.getByRole('tab', { name: t('screen.settingsTab') }).click();
  await expect(card).not.toContainText(t('restart.hung'));

  // Back: live mode stays on.
  await page.getByRole('switch').click({ force: true });
  await expect(page.locator('#status-device')).toHaveText(t('status.live'));
  await page.waitForTimeout(1500);
  await expect(page.getByRole('switch')).toBeChecked();
  expect(errors).toEqual([]);
});

test('screens without a wake chip offer no restart', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turzx');
  await page.getByRole('tab', { name: t('library.screen') }).click();
  await expect(page.getByRole('region', { name: 'Turing 2.1" Round (USB)' })).toBeVisible();
  await expect(page.getByRole('button', { name: t('screen.restart') })).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('an animated GIF moves in the preview at its own pace', async ({ page }) => {
  // T-7.11: the theme refreshes every 5 s, its GIF every 100 ms; the
  // preview draws the GIF's frames in between, at most 15 a second.
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=gif');
  await expect(page.locator('#theme-name')).toHaveValue('GIF');
  const colors = await page.evaluate(async () => {
    const ctx = document.getElementById('preview').getContext('2d');
    const seen = new Set();
    const end = performance.now() + 900;
    while (performance.now() < end) {
      seen.add(ctx.getImageData(300, 240, 1, 1).data.slice(0, 3).join(','));
      await new Promise((resolve) => { setTimeout(resolve, 30); });
    }
    return [...seen];
  });
  expect(colors.length).toBeGreaterThanOrEqual(3);
  expect(errors).toEqual([]);
});

test('a live screen that drops is connected again by itself', async ({ page, t }) => {
  // T-7.11: the backend connects the screen again (2, 5, 10 s apart); the
  // status says so meanwhile, and a toast when it is back.
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=flaky');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('switch').click({ force: true });
  await expect(page.locator('#status-device')).toHaveText(t('restart.reconnecting', { attempt: 1, attempts: 3 }));
  await expectAccessible(page);
  await expect(page.locator('#toast')).toHaveText(t('restart.doneLive'), { timeout: 5_000 });
  await expect(page.locator('#status-device')).toHaveText(t('status.live'));
  await expect(page.getByRole('switch')).toBeChecked();
  expect(errors).toEqual([]);
});


test('the Light switch persists and protects unsaved live edits on close', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('button', { name: t('top.preferences') }).click();
  const toggle = page.getByRole('switch', { name: t('prefs.lightOnClose') });
  await expect(toggle).toBeChecked();
  await expectAccessible(page);
  await toggle.uncheck();
  await page.getByRole('button', { name: t('prefs.done') }).click();
  await page.getByRole('button', { name: t('top.preferences') }).click();
  await expect(toggle).not.toBeChecked();
  await toggle.check();
  await page.getByRole('button', { name: t('prefs.done') }).click();
  await page.getByRole('switch').click({ force: true });
  await expect(page.getByRole('switch')).toBeChecked();
  await page.getByRole('tab', { name: t('library.layers') }).click();
  await page.getByRole('button', { name: /^CPU/ }).click();
  await page.keyboard.press('Delete');
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));
  const close = () => page.evaluate(() => window.dispatchEvent(new Event('bezel-demo-close')));
  const dialog = page.getByRole('dialog', { name: t('unsaved.title') });
  await close();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: t('dialog.cancel') }).click();
  expect(await page.locator('html').getAttribute('data-demo-window')).toBeNull();
  await close();
  await dialog.getByRole('button', { name: t('unsaved.discard') }).click();
  await expect(page.locator('html')).toHaveAttribute('data-demo-window', 'closed');
  expect(errors).toEqual([]);
});

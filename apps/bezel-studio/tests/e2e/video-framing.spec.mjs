// Framing the video background and playing it in the preview
// (D-2026-10-01-video-background-framing-5, -6), in demo mode, in pt-BR and
// en, light and dark, on the user's Dragon Ball case: a 480x1920 video turned
// for the 8.8"'s panel in a 1920x480 theme. Auto turns it upright and says
// so; the inspector's Framing group turns, fits, zooms and places it, one
// undo step per change; "Frame on canvas" pans it with the pointer, zooms it
// with the wheel and moves it with the keys, one undo step per gesture, with
// a live readout and no element selectable meanwhile; the preview plays the
// video and stops with reduced motion; without ffmpeg it shows the poster and
// the guide is one click away. No console errors, no serious or critical
// accessibility violations.
import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

const inspector = (page) => page.locator('#inspector');
const framing = (page, t) => inspector(page).getByRole('group', { name: t('framing.title') });
const pressed = (locator) => expect(locator).toHaveAttribute('aria-pressed', 'true');
const readout = (t, zoom, x, y) => t('framing.readout', { zoom, x, y });
/** The zoom a readout says, percent. */
const zoomOf = (text) => Number(/(\d+)%/.exec(text)[1]);

/** The preview's color at canvas pixel (x, y), as `[r, g, b]`. */
const pixel = (page, x, y) => page.evaluate(([px, py]) => [...document.getElementById('preview').getContext('2d').getImageData(px, py, 1, 1).data.slice(0, 3)], [x, y]);
const isGreen = ([r, g, b]) => g > r + 40 && g > b + 40;
const isBlue = ([r, g, b]) => b > r + 40 && b > g + 40;
const isRed = ([r, g, b]) => r > 200 && g < 40 && b < 40;
const isBlack = ([r, g, b]) => r + g + b < 20;

/** How many different pictures the preview showed in `ms` (a sum over its pixels). */
const picturesIn = (page, ms) => page.evaluate(async (span) => {
  const ctx = document.getElementById('preview').getContext('2d');
  const seen = new Set();
  const end = performance.now() + span;
  while (performance.now() < end) {
    const { data } = ctx.getImageData(0, 0, 1920, 480);
    let sum = 0;
    for (let i = 0; i < data.length; i += 4) sum += data[i + 1] * 3 + data[i + 2];
    seen.add(sum);
    await new Promise((resolve) => { setTimeout(resolve, 40); });
  }
  return seen.size;
}, ms);

async function openDragon(page, scenario = 'dragon') {
  await page.goto(`/index.html?demo=${scenario}`);
  await expect(page.locator('#theme-name')).toHaveValue('Dragon Ball');
}

test('video framing: Auto turns the pre-turned video upright and the inspector frames it', async ({ page, t }) => {
  const errors = watchErrors(page);
  await openDragon(page);
  const group = framing(page, t);
  const rotation = group.getByRole('group', { name: t('framing.rotation') });
  const auto = rotation.getByRole('button', { name: t('framing.auto', { degrees: 270 }) });
  await pressed(auto);
  await expect(group).toContainText(t('framing.autoTurned', { degrees: 270 }));
  // Upright: sky at the top, grass at the bottom.
  await expect.poll(() => pixel(page, 960, 5).then(isBlue)).toBe(true);
  await expect.poll(() => pixel(page, 960, 470).then(isGreen)).toBe(true);
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  await expectAccessible(page);

  // 0°: the picture as stored, sideways and four times too big (the report).
  await rotation.getByRole('button', { name: t('framing.degrees', { degrees: 0 }), exact: true }).click();
  await pressed(rotation.getByRole('button', { name: t('framing.degrees', { degrees: 0 }), exact: true }));
  await expect(group).not.toContainText(t('framing.autoTurned', { degrees: 270 }));
  await expect.poll(() => pixel(page, 5, 240).then(isGreen)).toBe(true);
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));

  // Fit: the whole video, the pad around it; its color.
  const fit = group.getByRole('group', { name: t('framing.fit') });
  await fit.getByRole('button', { name: t('framing.contain') }).click();
  await pressed(fit.getByRole('button', { name: t('framing.contain') }));
  await expect.poll(() => pixel(page, 5, 240).then(isBlack)).toBe(true);
  const pad = group.getByRole('textbox', { name: t('framing.padHex') });
  await pad.fill('#ff0000');
  await pad.press('Enter');
  await expect.poll(() => pixel(page, 5, 240).then(isRed)).toBe(true);
  await expectAccessible(page);

  // Zoom by number, position by slider; Center; each one undo step.
  const zoom = group.getByRole('spinbutton', { name: t('framing.zoomNumber') });
  await zoom.fill('150');
  await zoom.press('Enter');
  await expect(group.getByText(t('framing.percent', { value: 150 }))).toBeVisible();
  const x = group.getByRole('slider', { name: t('framing.positionX') });
  await x.focus();
  await page.keyboard.press('ArrowRight');
  await expect(x).toHaveValue('51');
  await page.keyboard.press('ArrowRight');
  await expect(x).toHaveValue('52');
  await page.getByRole('button', { name: t('top.undo') }).click();
  await expect(group.getByRole('slider', { name: t('framing.positionX') })).toHaveValue('51');
  await group.getByRole('button', { name: t('framing.center') }).click();
  await expect(group.getByRole('slider', { name: t('framing.positionX') })).toHaveValue('50');
  await expect(group.getByRole('button', { name: t('framing.center') })).toBeDisabled();

  // Reset: Auto, Fill, 100 %, centered; undo brings the framing back.
  await group.getByRole('button', { name: t('framing.reset') }).click();
  await pressed(auto);
  await pressed(fit.getByRole('button', { name: t('framing.cover') }));
  await expect(group.getByRole('spinbutton', { name: t('framing.zoomNumber') })).toHaveValue('100');
  await expect(group.getByRole('textbox', { name: t('framing.padHex') })).toHaveCount(0);
  await expect(group.getByRole('button', { name: t('framing.reset') })).toBeDisabled();
  await expect.poll(() => pixel(page, 960, 470).then(isGreen)).toBe(true);
  await page.getByRole('button', { name: t('top.undo') }).click();
  await expect(group.getByRole('spinbutton', { name: t('framing.zoomNumber') })).toHaveValue('150');
  await pressed(fit.getByRole('button', { name: t('framing.contain') }));
  await page.getByRole('button', { name: t('top.redo') }).click();
  await pressed(auto);
  expect(errors).toEqual([]);
});

test('video framing on the canvas: drag, wheel and keys, one undo step each', async ({ page, t }) => {
  const errors = watchErrors(page);
  await openDragon(page);
  const toggle = framing(page, t).getByRole('button', { name: t('framing.onCanvas') });
  await expect(toggle).toHaveAttribute('aria-pressed', 'false');
  await toggle.click();
  await pressed(framing(page, t).getByRole('button', { name: t('framing.onCanvas') }));
  const surface = page.getByRole('application', { name: t('framing.surface') });
  await expect(surface).toBeFocused();
  await expect(page.locator('#framing-hud')).toContainText(t('framing.hudTitle'));
  await expect(page.locator('#framing-hud')).toContainText(t('framing.hudKeys'));
  const numbers = page.locator('#framing-numbers');
  const live = page.locator('#framing-live');
  await expect(numbers).toHaveText(readout(t, 100, 50, 50));
  await expect(live).toHaveText(readout(t, 100, 50, 50));
  await expectAccessible(page);

  // The wheel zooms around the pointer; the whole burst is one undo step.
  const box = await page.locator('#canvas-box').boundingBox();
  const scale = box.width / 1920;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.wheel(0, -150);
  await page.mouse.wheel(0, -150);
  await expect(numbers).not.toHaveText(readout(t, 100, 50, 50));
  await expect(page.getByRole('button', { name: t('top.undo') })).toBeEnabled();
  await expect(live).not.toHaveText(readout(t, 100, 50, 50));
  const zoomed = await numbers.textContent();
  expect(zoomOf(zoomed)).toBeGreaterThan(150);
  await page.keyboard.press('Control+z');
  await expect(numbers).toHaveText(readout(t, 100, 50, 50));
  await page.keyboard.press('Control+Shift+z');
  await expect(numbers).toHaveText(zoomed);

  // A drag pans it: the picture follows the pointer; one undo step.
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 40, box.y + box.height / 2 + 15, { steps: 6 });
  await page.mouse.move(box.x + box.width / 2 + 80, box.y + box.height / 2 + 30, { steps: 6 });
  await page.mouse.up();
  await expect(numbers).not.toHaveText(zoomed);
  const dragged = await numbers.textContent();
  await expect(live).toHaveText(dragged);
  await page.getByRole('button', { name: t('top.undo') }).click();
  await expect(numbers).toHaveText(zoomed);
  await page.getByRole('button', { name: t('top.redo') }).click();
  await expect(numbers).toHaveText(dragged);

  // Elements cannot be selected meanwhile: a click on the title frames, it does not select.
  await page.mouse.click(box.x + 1600 * scale, box.y + 385 * scale);
  await expect(inspector(page).getByRole('heading', { name: t('inspector.theme') })).toBeVisible();
  await expect(page.locator('#overlay .sel-box')).toHaveCount(0);

  // Keys: + zooms 5 %, - back, an arrow moves it, 0 resets; one undo step each.
  await surface.focus();
  await page.keyboard.press('+');
  await expect.poll(async () => zoomOf(await numbers.textContent())).toBe(zoomOf(dragged) + 5);
  await page.keyboard.press('-');
  await expect.poll(async () => zoomOf(await numbers.textContent())).toBe(zoomOf(dragged));
  const back = await numbers.textContent();
  await page.keyboard.press('ArrowRight');
  await expect(numbers).not.toHaveText(back);
  await page.keyboard.press('Shift+ArrowUp');
  const nudged = await numbers.textContent();
  await page.keyboard.press('0');
  await expect(numbers).toHaveText(readout(t, 100, 50, 50));
  await expect(live).toHaveText(readout(t, 100, 50, 50));
  await page.keyboard.press('Control+z');
  await expect(numbers).toHaveText(nudged);
  await expectAccessible(page);

  // Esc leaves, giving the focus back to the toggle; a double click on the video comes back; Enter leaves.
  await page.keyboard.press('Escape');
  await expect(page.locator('#framing-hud')).toBeHidden();
  await expect(framing(page, t).getByRole('button', { name: t('framing.onCanvas') })).toHaveAttribute('aria-pressed', 'false');
  await expect(framing(page, t).getByRole('button', { name: t('framing.onCanvas') })).toBeFocused();
  await expect(page.getByRole('application')).toHaveCount(0);
  await page.mouse.dblclick(box.x + 200 * scale, box.y + 100 * scale);
  await expect(page.locator('#framing-hud')).toBeVisible();
  await expect(surface).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('#framing-hud')).toBeHidden();
  // The done button and a selection end it too.
  await framing(page, t).getByRole('button', { name: t('framing.onCanvas') }).click();
  await page.getByRole('button', { name: t('framing.done') }).click();
  await expect(page.locator('#framing-hud')).toBeHidden();
  expect(errors).toEqual([]);
});

test('video background plays in the preview and stops with reduced motion', async ({ page, t }) => {
  const errors = watchErrors(page);
  await openDragon(page);
  const root = page.locator('html');
  await expect(root).toHaveAttribute('data-demo-decoder', 'running');
  expect(await picturesIn(page, 1000)).toBeGreaterThanOrEqual(4);
  await expect(framing(page, t)).not.toContainText(t('framing.reducedMotion'));

  // Reduced motion: the poster, no more pictures, the decoder ends.
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(framing(page, t)).toContainText(t('framing.reducedMotion'));
  await expect(root).toHaveAttribute('data-demo-decoder', 'stopped', { timeout: 5_000 });
  expect(await picturesIn(page, 800)).toBe(1);
  // Framing still shows on the poster.
  await framing(page, t).getByRole('group', { name: t('framing.fit') }).getByRole('button', { name: t('framing.contain') }).click();
  await framing(page, t).getByRole('group', { name: t('framing.rotation') }).getByRole('button', { name: t('framing.degrees', { degrees: 0 }), exact: true }).click();
  await expect.poll(() => pixel(page, 5, 240).then(isBlack)).toBe(true);
  await expect(root).toHaveAttribute('data-demo-decoder', 'stopped');
  await expectAccessible(page);

  // Motion back: it plays again.
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await expect(root).toHaveAttribute('data-demo-decoder', 'running');
  expect(await picturesIn(page, 800)).toBeGreaterThanOrEqual(3);
  expect(errors).toEqual([]);
});

test('video background plays only with ffmpeg: without it the poster shows and the guide is a click away', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  await openDragon(page, 'dragonNoFfmpeg');
  const group = framing(page, t);
  await expect(group).toContainText(t('framing.noFfmpeg'));
  // Auto still works (the MP4 header is read without ffmpeg).
  await pressed(group.getByRole('button', { name: t('framing.auto', { degrees: 270 }) }));
  await expect.poll(() => pixel(page, 960, 470).then(isGreen)).toBe(true);
  expect(await picturesIn(page, 600)).toBe(1);
  await expect(page.locator('html')).not.toHaveAttribute('data-demo-decoder', /.+/);
  await group.getByRole('button', { name: t('framing.guide') }).click();
  await expect(page.locator('html')).toHaveAttribute('data-demo-guide', `ffmpeg ${lang}`);
  // The framing stays editable and is kept.
  await group.getByRole('group', { name: t('framing.fit') }).getByRole('button', { name: t('framing.contain') }).click();
  await pressed(framing(page, t).getByRole('group', { name: t('framing.fit') }).getByRole('button', { name: t('framing.contain') }));
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

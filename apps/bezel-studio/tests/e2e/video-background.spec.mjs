// A video background from the studio, in demo mode, in pt-BR and en, light
// and dark: "Add video…" copies a video with its poster into the theme,
// "Use as background" makes it the background (undo brings the old one
// back), the inspector shows it with what the screen does with it and links
// to the storage tab that sends it, files dropped on the editing area become
// the background (an animated GIF is a video, a GIF of one picture an
// image), and without ffmpeg the video comes without a poster and the
// inspector says what ffmpeg is for. No console errors, no serious or
// critical accessibility violations.
import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';
import { formatBytes } from '../../src/ui/storage.js';
import { formatDuration } from '../../src/editor/background.js';

const toast = (page) => page.locator('#toast');
const inspector = (page) => page.locator('#inspector');

/** What the toast says of the demo's `ferias.mp4`: its play time and size. */
const feriasDetails = (lang) => `${formatDuration(12_400)} · ${formatBytes(24_117_248, lang)}`;

/** Drops a file called `name` of `size` bytes on `target`, like one dragged from the desktop. */
async function dropFile(page, target, name, size = 4096) {
  const dataTransfer = await page.evaluateHandle(({ fileName, bytes }) => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array(bytes)], fileName));
    return dt;
  }, { fileName: name, bytes: size });
  await target.dispatchEvent('drop', { dataTransfer });
}

test('a video added from the Media panel becomes the background, and undo takes it back', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: t('library.media') }).click();
  await expect(page.getByText(t('media.dropHint'))).toBeVisible();
  await page.getByRole('button', { name: t('media.addVideo') }).click();
  await expect(toast(page)).toHaveText(t('toast.videoAdded', { name: 'ferias.mp4', details: feriasDetails(lang) }));
  const item = page.locator('.media-item[data-ref="assets/ferias.mp4"]');
  await expect(item).toContainText(`${t('media.video')} · ${feriasDetails(lang)}`);
  await expect(item.locator('.thumb')).toHaveCSS('background-image', /^url\("data:image\/svg\+xml/);
  await expect(page.locator('.media-item[data-ref="assets/ferias-poster.png"]')).toHaveCount(0);
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'), { timeout: 1000 });
  await expectAccessible(page);

  await item.getByRole('button', { name: t('media.useBackground') }).click();
  await expect(page.locator('#status-main')).toHaveText(t('status.unsaved'));
  const shown = inspector(page).getByRole('group', { name: t('bg.video') });
  await expect(shown).toContainText('ferias.mp4');
  await expect(shown).toContainText(feriasDetails(lang));
  await expect(shown.locator('.thumb')).toHaveCSS('background-image', /^url\("data:image\/svg\+xml/);
  await expect(inspector(page)).toContainText(t('inspector.video.checkWhenLive'));
  for (const action of ['inspector.replaceVideo', 'inspector.useImage', 'inspector.useColor']) {
    await expect(inspector(page).getByRole('button', { name: t(action) })).toBeVisible();
  }
  await expectAccessible(page);

  // Live: the 8.8" does not store it yet; the storage tab sends it.
  await page.getByRole('switch').click({ force: true });
  await expect(inspector(page)).toContainText(t('inspector.video.missing'));
  await inspector(page).getByRole('button', { name: t('inspector.video.openStorage') }).click();
  const cta = page.getByRole('region', { name: t('storage.videoMissingTitle') });
  await expect(cta.getByRole('button', { name: t('storage.sendVideo') })).toBeVisible();
  await expectAccessible(page);
  await page.getByRole('switch').click({ force: true });

  await page.getByRole('button', { name: t('top.undo') }).click();
  await expect(inspector(page).getByRole('group', { name: t('bg.video') })).toHaveCount(0);
  await expect(inspector(page).getByRole('button', { name: t('inspector.useVideo') })).toBeVisible();
  await expect(page.locator('#status-main')).toHaveText(t('status.saved'));
  await page.getByRole('button', { name: t('top.redo') }).click();
  await expect(inspector(page).getByRole('group', { name: t('bg.video') })).toContainText('ferias.mp4');
  expect(errors).toEqual([]);
});

test('files dropped on the editing area become the background; a GIF is a video', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const stage = page.locator('#stage');

  await dropFile(page, stage, 'Chuva Forte.mp4', 6_000_000);
  const details = `${formatDuration(12_400)} · ${formatBytes(6_000_000, lang)}`;
  await expect(toast(page)).toHaveText(t('toast.videoAdded', { name: 'chuva-forte.mp4', details }));
  await expect(inspector(page).getByRole('group', { name: t('bg.video') })).toContainText('chuva-forte.mp4');

  // An animated GIF: a video background, its poster its first picture.
  await dropFile(page, stage, 'Ondas.gif', 3_000_000);
  const gif = inspector(page).getByRole('group', { name: t('bg.video') });
  await expect(gif).toContainText('ondas.gif');
  await expect(gif).toContainText(formatDuration(2_400));
  await expectAccessible(page);
  // One undo per drop.
  await page.getByRole('button', { name: t('top.undo') }).click();
  await expect(inspector(page).getByRole('group', { name: t('bg.video') })).toContainText('chuva-forte.mp4');

  // A GIF of one picture is a picture background.
  await dropFile(page, stage, 'parado.gif', 98_304);
  await expect(toast(page)).toHaveText(t('toast.imageAdded', { name: 'parado.gif' }));
  await expect(inspector(page)).toContainText(t('bg.image'));
  await expect(inspector(page).getByRole('group', { name: t('bg.video') })).toHaveCount(0);

  // Something else is refused with what to drop.
  await dropFile(page, stage, 'notes.txt', 10);
  await expect(toast(page)).toHaveText(t('media.dropUnsupported'));

  // Dropped on the Media panel, a file is only added.
  await page.getByRole('tab', { name: t('library.media') }).click();
  await dropFile(page, page.locator('#panel-media'), 'Mar.gif', 1_000_000);
  const sea = page.locator('.media-item[data-ref="assets/mar.gif"]');
  await expect(sea).toContainText(t('media.animatedGif'));
  await expect(inspector(page)).toContainText(t('bg.image'), { timeout: 1000 });
  await sea.getByRole('button', { name: t('media.useBackground') }).click();
  await expect(inspector(page).getByRole('group', { name: t('bg.video') })).toContainText('mar.gif');
  expect(errors).toEqual([]);
});

test('without ffmpeg the video comes without a poster and the inspector says why', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=noffmpeg');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await inspector(page).getByRole('button', { name: t('inspector.useVideo') }).click();
  await expect(toast(page)).toHaveText(t('toast.videoAddedNoFfmpeg', { name: 'ferias.mp4', details: feriasDetails(lang) }));
  const shown = inspector(page).getByRole('group', { name: t('bg.video') });
  await expect(shown).toContainText('ferias.mp4');
  await expect(shown.locator('.thumb svg')).toHaveCount(1);
  await expect(inspector(page)).toContainText(t('inspector.video.noPoster'));
  await expect(inspector(page).locator('.hints code').first()).toHaveText('sudo dnf install ffmpeg');
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

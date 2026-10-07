import { test, expect, watchErrors } from './helpers.mjs';

test('Media offers a local preview without changing the background', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await page.getByRole('tab', { name: t('library.media') }).click();
  await page.getByRole('button', { name: t('media.addVideo') }).click();
  const item = page.locator('.media-item[data-ref="assets/ferias.mp4"]');
  await item.getByRole('button', { name: t('videoPreview.open'), exact: true }).click();
  const dialog = page.locator('.video-preview-dialog');
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText(t('videoPreview.unavailable'));
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(item.getByRole('button', { name: t('videoPreview.open'), exact: true })).toBeFocused();
  expect(errors).toEqual([]);
});

test('closing while preview loads discards the late response', async ({ page }) => {
  await page.goto('/index.html?demo=turing88');
  await page.evaluate(async () => {
    const { openVideoPreview } = await import('/ui/video-preview.js');
    const { translator } = await import('/i18n/index.js');
    let resolve;
    const pending = new Promise(r => { resolve = r; });
    const dialog = openVideoPreview({ t: translator('en'), name: 'clip.mp4', load: () => pending });
    const closed = new Promise(r => dialog.addEventListener('close', r, { once: true }));
    dialog.close();
    await closed;
    resolve('data:video/mp4;base64,Y2xpcA==');
    await new Promise(r => setTimeout(r, 0));
    if (document.querySelector('.video-preview-dialog')) throw Error('Preview not removed');
  });
});

test('Storage offers association when a video has no local copy', async ({ page, t }) => {
  await page.goto('/index.html?demo=vendorCard');
  await page.getByRole('tab', { name: t('library.screen'), exact: true }).click();
  await page.getByRole('tab', { name: t('screen.storageTab') }).click();
  await page.locator('[data-path="sd/video/AMD.mp4"]').first().click();
  await page.locator('[data-focus="act-preview-sd"]').click();
  const dialog = page.locator('.video-preview-dialog');
  await expect(dialog).toContainText(t('videoPreview.unavailable'));
  await expect(dialog.getByRole('button', { name: t('videoPreview.associate') })).toBeVisible();
  await page.keyboard.press('Escape');
});

test('the local player can play a video and releases it on close', async ({ page }) => {
  await page.goto('/index.html?demo=turing88');
  await page.evaluate(async () => {
    const canvas = document.createElement('canvas'); canvas.width = 32; canvas.height = 32;
    const stream = canvas.captureStream(10);
    const recorder = new MediaRecorder(stream, { mimeType: 'video/webm' });
    const chunks = [];
    recorder.ondataavailable = e => chunks.push(e.data);
    const finished = new Promise(r => recorder.onstop = r);
    recorder.start();
    canvas.getContext('2d').fillRect(0, 0, 32, 32);
    await new Promise(r => setTimeout(r, 250)); recorder.stop(); await finished;
    stream.getTracks().forEach(track => track.stop());
    const reader = new FileReader();
    const loaded = new Promise(r => reader.onload = r);
    reader.readAsDataURL(new Blob(chunks, { type: 'video/webm' })); await loaded;
    const { openVideoPreview } = await import('/ui/video-preview.js');
    const { translator } = await import('/i18n/index.js');
    const dialog = openVideoPreview({ t: translator('en'), name: 'clip.webm', load: () => reader.result });
    const player = dialog.querySelector('video'); player.muted = true;
    await new Promise((resolve, reject) => { player.addEventListener('loadeddata', resolve, {once:true}); player.addEventListener('error', reject, {once:true}); });
    await player.play();
    if (player.paused || !player.controls || !player.loop) throw Error('Player did not start');
    const closed = new Promise(r => dialog.addEventListener('close', r, {once:true})); dialog.close(); await closed;
    if (player.hasAttribute('src') || !player.paused) throw Error('Player not released');
  });
});

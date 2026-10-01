// The storage tab of the Screen panel in demo mode, in pt-BR and en, light
// and dark: usage and files per medium, sending with a summary, a progress
// bar and Cancel, Delete and the boot media behind a dialog that names the
// file, a missing ffmpeg and a missing theme video explained inline, a file
// stored with the wrong size, and what a TUR_USB screen does not offer. No
// console errors, no serious or critical accessibility violations.
import { test, expect, watchErrors, expectAccessible, prefixOf, suffixOf } from './helpers.mjs';
import { formatBytes } from '../../src/ui/storage.js';
import { DEMO_LET_GO_EVENT } from '../../src/bridge.js';

async function openStorage(page, t) {
  await page.getByRole('tab', { name: t('library.screen') }).click();
  await page.getByRole('tab', { name: t('screen.storageTab') }).click();
  await expect(page.getByRole('region', { name: t('storage.medium.internal') })).toBeVisible();
}

async function dragTo(page, source, target) {
  const from = await source.boundingBox();
  const to = await target.boundingBox();
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + from.width / 2 + 20, from.y + from.height / 2 - 20, { steps: 4 });
  await page.mouse.move(to.x + to.width / 2, to.y + to.height / 2, { steps: 8 });
  await page.mouse.up();
}

/** Drops a file called `name` on `zone`, like one dragged from the desktop. */
async function dropFile(page, zone, name, type) {
  const dataTransfer = await page.evaluateHandle(({ fileName, fileType }) => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array(2048)], fileName, { type: fileType }));
    return dt;
  }, { fileName: name, fileType: type });
  await zone.locator('.drop-zone').dispatchEvent('drop', { dataTransfer });
}

/** Where the summary says a file goes: "in the internal memory (videos)". */
const placeIn = (t, medium, kind) => `${t(`storage.in.${medium}`)} (${t(`storage.kind.${kind}`).toLowerCase()})`;

const toast = (page) => page.locator('#toast');
const UPLOAD = { timeout: 15_000 };

/**
 * Lets `count` held upload phases go on (a page opened with `&hold` holds
 * each phase after its first step): one now, the others when they come.
 */
async function letGo(page, count = 1) {
  await page.evaluate(({ name, times }) => {
    for (let i = 0; i < times; i += 1) window.dispatchEvent(new Event(name));
  }, { name: DEMO_LET_GO_EVENT, times: count });
}

test('storage tab upload progress and confirmed delete', async ({ page, t, lang }) => {
  const errors = watchErrors(page);
  // Each phase of an upload waits in the middle until the test lets it go.
  await page.goto('/index.html?demo=turing88&hold');
  await openStorage(page, t);
  const internal = page.getByRole('region', { name: t('storage.medium.internal') });
  const card = page.getByRole('region', { name: t('storage.medium.sd') });
  await expect(internal).toContainText('logo.png');
  await expect(internal).toContainText(formatBytes(7_516_192_768, lang));
  await expect(card).toContainText('chuva.mp4');
  await expect(card).toContainText(t('storage.cardHelp'));
  await expectAccessible(page);

  // Sending shows a summary first, then a progress bar with Cancel.
  await internal.getByRole('button', { name: t('storage.choose') }).click();
  const summary = page.getByRole('dialog', { name: t('storage.confirmUploadTitle', { name: 'ferias.mp4' }) });
  await expect(summary).toContainText(placeIn(t, 'internal', 'video'));
  await expect(summary).toContainText(t('storage.summary.converted', { width: 480, height: 1920 }));
  await expectAccessible(page);
  await summary.getByRole('button', { name: t('storage.confirmUploadAction'), exact: true }).click();
  const bar = page.getByRole('progressbar', { name: t('storage.progressLabel') });
  await expect(bar).toBeVisible();
  await expect(page.getByText(t('storage.phase.convert'), { exact: true })).toBeVisible();
  await letGo(page);
  await expect(page.getByText(t('storage.phase.upload'), { exact: true })).toBeVisible();
  await expect.poll(() => bar.evaluate((b) => b.value)).toBeGreaterThan(0);
  await expect(internal.getByRole('button', { name: t('storage.choose') })).toBeDisabled();
  await expect(internal.getByRole('button', { name: t('storage.delete', { name: 'logo.png' }) })).toBeDisabled();
  await expectAccessible(page);
  await page.getByRole('button', { name: t('storage.cancel') }).click();
  const cancelled = page.getByRole('region', { name: t('storage.cancelled') });
  await expect(cancelled).toContainText(prefixOf(t, 'storage.partial', ['size', 'name']));
  await expect(bar).toBeHidden();
  await expect(internal.getByRole('listitem').filter({ hasText: 'ferias.mp4' })).toHaveCount(1);

  // Deleting asks first and names the file; Esc keeps it.
  await cancelled.getByRole('button', { name: t('storage.deletePartial') }).click();
  let confirm = page.getByRole('dialog', { name: t('storage.confirmDeleteTitle', { name: 'ferias.mp4' }) });
  await expect(confirm).toContainText(t('storage.from.internal'));
  await expect(confirm.getByRole('button', { name: t('dialog.cancel') })).toBeFocused();
  await expectAccessible(page);
  // The editor's shortcuts do not act behind the dialog.
  await page.keyboard.press('Tab');
  await expect(confirm.getByRole('button', { name: t('storage.deleteAction'), exact: true })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(confirm).toHaveCount(0);
  await expect(cancelled.getByRole('button', { name: t('storage.deletePartial') })).toBeFocused();
  await expect(internal.getByRole('listitem').filter({ hasText: 'ferias.mp4' })).toHaveCount(1);

  await internal.getByRole('button', { name: t('storage.delete', { name: 'ferias.mp4' }) }).click();
  confirm = page.getByRole('dialog', { name: t('storage.confirmDeleteTitle', { name: 'ferias.mp4' }) });
  await confirm.getByRole('button', { name: t('storage.deleteAction'), exact: true }).click();
  await expect(toast(page)).toHaveText(t('storage.deleted', { name: 'ferias.mp4' }));
  await expect(internal.getByRole('listitem').filter({ hasText: 'ferias.mp4' })).toHaveCount(0);

  // A complete upload lists the stored file.
  await internal.getByRole('button', { name: t('storage.choose') }).click();
  await page.getByRole('dialog').getByRole('button', { name: t('storage.confirmUploadAction'), exact: true }).click();
  await letGo(page, 2);
  await expect(toast(page)).toHaveText(t('storage.uploaded', { name: 'ferias.mp4' }), UPLOAD);
  await expect(internal.getByRole('listitem').filter({ hasText: 'ferias.mp4' })).toContainText('MB');

  // The same file again replaces it, and the summary says so.
  await internal.getByRole('button', { name: t('storage.choose') }).click();
  const replace = page.getByRole('dialog', { name: t('storage.confirmUploadTitle', { name: 'ferias.mp4' }) });
  await expect(replace).toContainText(suffixOf(t, 'storage.summary.replaces', ['name', 'size', 'place']));
  await replace.getByRole('button', { name: t('dialog.cancel') }).click();
  await expect(replace).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('storage tab explains a missing ffmpeg and locates it', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=noffmpeg');
  await openStorage(page, t);
  const ffmpeg = page.getByRole('region', { name: t('storage.ffmpegMissingTitle') });
  await expect(ffmpeg).toContainText('sudo dnf install ffmpeg');
  const card = page.getByRole('region', { name: t('storage.medium.sd') });
  await expect(card).toContainText(t('storage.noCard'));
  await expect(card).toContainText(t('storage.cardHelp'));
  await expectAccessible(page);

  const internal = page.getByRole('region', { name: t('storage.medium.internal') });
  await internal.getByRole('button', { name: t('storage.choose') }).click();
  const refused = page.getByRole('region', { name: t('storage.refusedTitle', { name: 'ferias.mp4' }) });
  const details = [t('storage.mismatch.audio'), t('storage.mismatch.resolution', { found: '1920x1080', expected: '480x1920' })].join('; ');
  await expect(refused).toContainText(t('storage.refused.needsConverter', { details }));
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expectAccessible(page);
  await refused.getByRole('button', { name: t('storage.ffmpegLocate') }).click();
  await expect(toast(page)).toHaveText(t('storage.ffmpegReady', { version: '7.1' }));
  await expect(ffmpeg).toHaveCount(0);

  await internal.getByRole('button', { name: t('storage.choose') }).click();
  await expect(page.getByRole('dialog', { name: t('storage.confirmUploadTitle', { name: 'ferias.mp4' }) })).toBeVisible();
  expect(errors).toEqual([]);
});

test('a live theme video missing on the screen is sent on request', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=video');
  await expect(page.locator('#theme-name')).toHaveValue('Vídeo');
  await page.getByRole('switch').click({ force: true });
  await openStorage(page, t);
  const cta = page.getByRole('region', { name: t('storage.videoMissingTitle') });
  await expect(cta).toContainText(t('storage.videoMissing'));
  await expect(page.locator('#status-device')).toHaveText(t('status.liveVideoMissing'));
  // While live, the theme covers what the screen plays.
  const internal = page.getByRole('region', { name: t('storage.medium.internal') });
  await expect(internal.getByRole('button', { name: t('storage.play', { name: 'logo.png' }) })).toBeDisabled();
  await expect(page.getByText(t('storage.liveHint'))).toBeVisible();
  await expectAccessible(page);

  await cta.getByRole('button', { name: t('storage.sendVideo') }).click();
  const summary = page.getByRole('dialog', { name: t('storage.confirmUploadTitle', { name: 'nebula_90.mp4' }) });
  await expect(summary).toContainText(t('storage.summary.turned', { degrees: 90 }));
  await expect(summary).toContainText(placeIn(t, 'sd', 'video'));
  await summary.getByRole('button', { name: t('storage.confirmUploadAction'), exact: true }).click();
  await expect(toast(page)).toHaveText(t('storage.uploaded', { name: 'nebula_90.mp4' }), UPLOAD);
  await expect(cta).toHaveCount(0);
  await expect(page.locator('#status-device')).toHaveText(t('status.live'));
  expect(errors).toEqual([]);
});

test('the boot media asks first, files drop on a medium and TUR_USB keeps its own', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await openStorage(page, t);
  const internal = page.getByRole('region', { name: t('storage.medium.internal') });
  const card = page.getByRole('region', { name: t('storage.medium.sd') });
  const slot = page.getByRole('region', { name: t('storage.bootTitle') });

  await card.getByRole('button', { name: t('storage.boot', { name: 'chuva.mp4' }) }).click();
  const boot = page.getByRole('dialog', { name: t('storage.confirmBootTitle', { name: 'chuva.mp4' }) });
  await expect(boot).toContainText(t('storage.confirmBoot', { name: 'chuva.mp4' }));
  // No brightness set in this session: the screen keeps its own default.
  await expect(boot).toContainText(t('storage.bootKeepsDefault'));
  await boot.getByRole('button', { name: t('storage.bootAction') }).click();
  await expect(toast(page)).toHaveText(t('storage.bootSet', { name: 'chuva.mp4' }));

  // The brightness set under Settings is the one the screen starts with.
  await page.getByRole('tab', { name: t('screen.settingsTab') }).click();
  await page.getByRole('slider', { name: t('screen.brightness') }).fill('40');
  await page.getByRole('tab', { name: t('screen.storageTab') }).click();
  await slot.getByRole('button', { name: t('storage.defaultAction') }).click();
  const reset = page.getByRole('dialog', { name: t('storage.confirmDefaultTitle') });
  await expect(reset).toContainText(t('storage.bootKeeps', { percent: 40 }));
  await reset.getByRole('button', { name: t('dialog.cancel') }).click();

  // A stored file dragged onto the slot asks the same.
  await dragTo(page, internal.getByText('logo.png', { exact: true }), slot);
  const dragged = page.getByRole('dialog', { name: t('storage.confirmBootTitle', { name: 'logo.png' }) });
  await expect(dragged).toBeVisible();
  await dragged.getByRole('button', { name: t('dialog.cancel') }).click();
  await expect(dragged).toHaveCount(0);

  await internal.getByRole('button', { name: t('storage.play', { name: 'logo.png' }) }).click();
  await expect(toast(page)).toHaveText(t('storage.playing', { name: 'logo.png' }));

  // A file dropped on a medium goes to its folder of that kind.
  await dropFile(page, card, 'Mapa Novo.png', 'image/png');
  const summary = page.getByRole('dialog', { name: t('storage.confirmUploadTitle', { name: 'mapa_novo.png' }) });
  await expect(summary).toContainText(placeIn(t, 'sd', 'image'));
  await expect(summary).toContainText(t('storage.summary.asIs'));
  await summary.getByRole('button', { name: t('storage.confirmUploadAction'), exact: true }).click();
  await expect(toast(page)).toHaveText(t('storage.uploaded', { name: 'mapa_novo.png' }), UPLOAD);
  await expect(card).toContainText('mapa_novo.png');

  // A TUR_USB screen takes files and plays them, but Bezel neither deletes
  // them nor sets its boot media.
  await page.goto('/index.html?demo=turzx');
  await openStorage(page, t);
  await expect(page.getByText(t('storage.limitedHint'))).toBeVisible();
  await expect(page.getByRole('button', { name: new RegExp(`^${t('storage.deleteAction')}`) })).toHaveCount(0);
  await expect(page.getByRole('region', { name: t('storage.bootTitle') })).toHaveCount(0);
  await expect(page.locator('#storage-panel')).not.toContainText('null');
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('a file stored with the wrong size is explained and deleted on request', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await openStorage(page, t);
  const internal = page.getByRole('region', { name: t('storage.medium.internal') });
  await dropFile(page, internal, 'torto.png', 'image/png');
  const summary = page.getByRole('dialog', { name: t('storage.confirmUploadTitle', { name: 'torto.png' }) });
  await summary.getByRole('button', { name: t('storage.confirmUploadAction'), exact: true }).click();
  const failed = page.getByRole('region', { name: t('storage.errorTitle') });
  const sizes = { file: 'internal/image/torto.png', stored: '255990', expected: '256000' };
  await expect(failed).toContainText(t('error.sizeMismatch', sizes), UPLOAD);
  await expectAccessible(page);

  // Nothing is deleted on its own: the button asks first.
  await expect(internal.getByRole('listitem').filter({ hasText: 'torto.png' })).toHaveCount(1);
  await failed.getByRole('button', { name: t('storage.delete', { name: 'torto.png' }) }).click();
  await page.getByRole('dialog', { name: t('storage.confirmDeleteTitle', { name: 'torto.png' }) })
    .getByRole('button', { name: t('storage.deleteAction'), exact: true }).click();
  await expect(toast(page)).toHaveText(t('storage.deleted', { name: 'torto.png' }));
  await expect(internal.getByRole('listitem').filter({ hasText: 'torto.png' })).toHaveCount(0);
  expect(errors).toEqual([]);
});

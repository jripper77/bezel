// The storage tab of the "Tela" panel in demo mode (light and dark
// projects): usage and files per medium, sending with a summary, a progress
// bar and Cancel, Delete and the boot media behind a dialog that names the
// file, a missing ffmpeg and a missing theme video explained inline, and what
// a TUR_USB screen does not offer. No console errors, no serious or critical
// accessibility violations.
import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

function watchErrors(page) {
  const errors = [];
  page.on('console', (msg) => msg.type() === 'error' && errors.push(msg.text()));
  page.on('pageerror', (err) => errors.push(err.message));
  return errors;
}

async function expectAccessible(page) {
  const { violations } = await new AxeBuilder({ page }).analyze();
  const serious = violations.filter((v) => ['critical', 'serious'].includes(v.impact));
  expect(serious.map((v) => `${v.id}: ${v.help} @ ${v.nodes.map((n) => n.target.join(' ')).join(', ')}`)).toEqual([]);
}

async function openStorage(page) {
  await page.getByRole('tab', { name: 'Tela' }).click();
  await page.getByRole('tab', { name: 'Armazenamento' }).click();
  await expect(page.getByRole('region', { name: 'Memória interna' })).toBeVisible();
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

const toast = (page) => page.locator('#toast');
const UPLOAD = { timeout: 15_000 };

test('storage tab upload progress and confirmed delete', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await openStorage(page);
  const internal = page.getByRole('region', { name: 'Memória interna' });
  const card = page.getByRole('region', { name: 'Cartão SD' });
  await expect(internal).toContainText('logo.png');
  await expect(internal).toContainText(/usados de 7,5 GB/);
  await expect(card).toContainText('chuva.mp4');
  await expect(card).toContainText('FAT32');
  await expectAccessible(page);

  // Sending shows a summary first, then a progress bar with Cancel.
  await internal.getByRole('button', { name: 'Enviar arquivo…' }).click();
  const summary = page.getByRole('dialog', { name: 'Enviar “ferias.mp4” para a tela?' });
  await expect(summary).toContainText('na memória interna (vídeos)');
  await expect(summary).toContainText('Vídeo H.264 de 480×1920, sem áudio');
  await expectAccessible(page);
  await summary.getByRole('button', { name: 'Enviar', exact: true }).click();
  const bar = page.getByRole('progressbar', { name: 'Progresso do envio' });
  await expect(bar).toBeVisible();
  await expect(page.getByText('Convertendo', { exact: true })).toBeVisible();
  await expect(page.getByText('Enviando', { exact: true })).toBeVisible();
  await expect.poll(() => bar.evaluate((b) => b.value)).toBeGreaterThan(0);
  await expect(internal.getByRole('button', { name: 'Enviar arquivo…' })).toBeDisabled();
  await expect(internal.getByRole('button', { name: 'Apagar “logo.png”' })).toBeDisabled();
  await expectAccessible(page);
  await page.getByRole('button', { name: 'Cancelar envio' }).click();
  const cancelled = page.getByRole('region', { name: 'Envio cancelado' });
  await expect(cancelled).toContainText('arquivo incompleto');
  await expect(bar).toBeHidden();
  await expect(internal.getByRole('listitem').filter({ hasText: 'ferias.mp4' })).toHaveCount(1);

  // Deleting asks first and names the file; Esc keeps it.
  await cancelled.getByRole('button', { name: 'Apagar o arquivo incompleto' }).click();
  let confirm = page.getByRole('dialog', { name: 'Apagar “ferias.mp4”?' });
  await expect(confirm).toContainText('da memória interna da tela');
  await expect(confirm.getByRole('button', { name: 'Cancelar' })).toBeFocused();
  await expectAccessible(page);
  // The editor's shortcuts do not act behind the dialog.
  await page.keyboard.press('Tab');
  await expect(confirm.getByRole('button', { name: 'Apagar', exact: true })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(confirm).toHaveCount(0);
  await expect(cancelled.getByRole('button', { name: 'Apagar o arquivo incompleto' })).toBeFocused();
  await expect(internal.getByRole('listitem').filter({ hasText: 'ferias.mp4' })).toHaveCount(1);

  await internal.getByRole('button', { name: 'Apagar “ferias.mp4”' }).click();
  confirm = page.getByRole('dialog', { name: 'Apagar “ferias.mp4”?' });
  await confirm.getByRole('button', { name: 'Apagar', exact: true }).click();
  await expect(toast(page)).toHaveText('“ferias.mp4” foi apagado.');
  await expect(internal.getByRole('listitem').filter({ hasText: 'ferias.mp4' })).toHaveCount(0);

  // A complete upload lists the stored file.
  await internal.getByRole('button', { name: 'Enviar arquivo…' }).click();
  await page.getByRole('dialog').getByRole('button', { name: 'Enviar', exact: true }).click();
  await expect(toast(page)).toHaveText('“ferias.mp4” está na tela.', UPLOAD);
  await expect(internal.getByRole('listitem').filter({ hasText: 'ferias.mp4' })).toContainText('MB');

  // The same file again replaces it, and the summary says so.
  await internal.getByRole('button', { name: 'Enviar arquivo…' }).click();
  const replace = page.getByRole('dialog', { name: 'Enviar “ferias.mp4” para a tela?' });
  await expect(replace).toContainText('ele será substituído');
  await replace.getByRole('button', { name: 'Cancelar' }).click();
  await expect(replace).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('storage tab explains a missing ffmpeg and locates it', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=noffmpeg');
  await openStorage(page);
  const ffmpeg = page.getByRole('region', { name: 'ffmpeg não encontrado' });
  await expect(ffmpeg).toContainText('sudo dnf install ffmpeg');
  const card = page.getByRole('region', { name: 'Cartão SD' });
  await expect(card).toContainText('Nenhum cartão SD na tela.');
  await expect(card).toContainText('não formata');
  await expectAccessible(page);

  const internal = page.getByRole('region', { name: 'Memória interna' });
  await internal.getByRole('button', { name: 'Enviar arquivo…' }).click();
  const refused = page.getByRole('region', { name: 'Não dá para enviar “ferias.mp4”' });
  await expect(refused).toContainText('precisa ser convertido (tem áudio; tem 1920x1080 em vez de 480x1920)');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expectAccessible(page);
  await refused.getByRole('button', { name: 'Localizar ffmpeg…' }).click();
  await expect(toast(page)).toHaveText('ffmpeg 7.1 pronto para converter vídeos.');
  await expect(ffmpeg).toHaveCount(0);

  await internal.getByRole('button', { name: 'Enviar arquivo…' }).click();
  await expect(page.getByRole('dialog', { name: 'Enviar “ferias.mp4” para a tela?' })).toBeVisible();
  expect(errors).toEqual([]);
});

test('a live theme video missing on the screen is sent on request', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=video');
  await expect(page.locator('#theme-name')).toHaveValue('Vídeo');
  await page.getByRole('switch').click({ force: true });
  await openStorage(page);
  const cta = page.getByRole('region', { name: 'Vídeo de fundo fora da tela' });
  await expect(cta).toContainText('imagem de capa');
  await expect(page.locator('#status-device')).toHaveText('ao vivo · vídeo de fundo fora da tela');
  // While live, the theme covers what the screen plays.
  const internal = page.getByRole('region', { name: 'Memória interna' });
  await expect(internal.getByRole('button', { name: 'Tocar “logo.png” na tela' })).toBeDisabled();
  await expect(page.getByText('o tema cobre o que a tela toca')).toBeVisible();
  await expectAccessible(page);

  await cta.getByRole('button', { name: 'Enviar para a tela' }).click();
  const summary = page.getByRole('dialog', { name: 'Enviar “nebula_90.mp4” para a tela?' });
  await expect(summary).toContainText('girado 90°');
  await expect(summary).toContainText('no cartão SD (vídeos)');
  await summary.getByRole('button', { name: 'Enviar', exact: true }).click();
  await expect(toast(page)).toHaveText('“nebula_90.mp4” está na tela.', UPLOAD);
  await expect(cta).toHaveCount(0);
  await expect(page.locator('#status-device')).toHaveText('ao vivo');
  expect(errors).toEqual([]);
});

test('the boot media asks first, files drop on a medium and TUR_USB keeps its own', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await openStorage(page);
  const internal = page.getByRole('region', { name: 'Memória interna' });
  const card = page.getByRole('region', { name: 'Cartão SD' });
  const slot = page.getByRole('region', { name: 'Ao ligar' });

  await card.getByRole('button', { name: 'Mostrar “chuva.mp4” ao ligar a tela' }).click();
  const boot = page.getByRole('dialog', { name: 'Mostrar “chuva.mp4” ao ligar?' });
  await expect(boot).toContainText('fica gravada na tela');
  // No brightness set in this session: the screen keeps its own default.
  await expect(boot).toContainText('liga com o brilho padrão, cerca de 67%, e nunca entra em repouso sozinha');
  await boot.getByRole('button', { name: 'Mostrar ao ligar' }).click();
  await expect(toast(page)).toHaveText('A tela vai mostrar “chuva.mp4” ao ligar.');

  // The brightness set under Ajustes is the one the screen starts with.
  await page.getByRole('tab', { name: 'Ajustes' }).click();
  await page.getByRole('slider', { name: 'Brilho' }).fill('40');
  await page.getByRole('tab', { name: 'Armazenamento' }).click();
  await slot.getByRole('button', { name: /relógio padrão/ }).click();
  const reset = page.getByRole('dialog', { name: 'Voltar ao relógio padrão?' });
  await expect(reset).toContainText('Ela liga com brilho de 40%, o nível que você ajustou no Bezel');
  await reset.getByRole('button', { name: 'Cancelar' }).click();

  // A stored file dragged onto the slot asks the same.
  await dragTo(page, internal.getByText('logo.png', { exact: true }), slot);
  const dragged = page.getByRole('dialog', { name: 'Mostrar “logo.png” ao ligar?' });
  await expect(dragged).toBeVisible();
  await dragged.getByRole('button', { name: 'Cancelar' }).click();
  await expect(dragged).toHaveCount(0);

  await internal.getByRole('button', { name: 'Tocar “logo.png” na tela' }).click();
  await expect(toast(page)).toHaveText('Tocando “logo.png” na tela.');

  // A file dropped on a medium goes to its folder of that kind.
  const dataTransfer = await page.evaluateHandle(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array(2048)], 'Mapa Novo.png', { type: 'image/png' }));
    return dt;
  });
  await card.locator('.drop-zone').dispatchEvent('drop', { dataTransfer });
  const summary = page.getByRole('dialog', { name: 'Enviar “mapa_novo.png” para a tela?' });
  await expect(summary).toContainText('no cartão SD (imagens)');
  await expect(summary).toContainText('Nenhuma: vai como está');
  await summary.getByRole('button', { name: 'Enviar', exact: true }).click();
  await expect(toast(page)).toHaveText('“mapa_novo.png” está na tela.', UPLOAD);
  await expect(card).toContainText('mapa_novo.png');

  // A TUR_USB screen takes files and plays them, but Bezel neither deletes
  // them nor sets its boot media.
  await page.goto('/index.html?demo=turzx');
  await openStorage(page);
  await expect(page.getByText('não apaga nem escolhe o que ela mostra ao ligar')).toBeVisible();
  await expect(page.getByRole('button', { name: /^Apagar/ })).toHaveCount(0);
  await expect(page.getByRole('region', { name: 'Ao ligar' })).toHaveCount(0);
  await expect(page.locator('#storage-panel')).not.toContainText('null');
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('a file stored with the wrong size is explained and deleted on request', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await openStorage(page);
  const internal = page.getByRole('region', { name: 'Memória interna' });
  const dataTransfer = await page.evaluateHandle(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array(16)], 'torto.png', { type: 'image/png' }));
    return dt;
  });
  await internal.locator('.drop-zone').dispatchEvent('drop', { dataTransfer });
  await page.getByRole('dialog', { name: 'Enviar “torto.png” para a tela?' }).getByRole('button', { name: 'Enviar', exact: true }).click();
  const failed = page.getByRole('region', { name: 'Não deu certo' });
  await expect(failed).toContainText('“internal/image/torto.png” foi gravado com 255990 bytes em vez de 256000. Apague e envie de novo.', UPLOAD);
  await expectAccessible(page);

  // Nothing is deleted on its own: the button asks first.
  await expect(internal.getByRole('listitem').filter({ hasText: 'torto.png' })).toHaveCount(1);
  await failed.getByRole('button', { name: 'Apagar “torto.png”' }).click();
  await page.getByRole('dialog', { name: 'Apagar “torto.png”?' }).getByRole('button', { name: 'Apagar', exact: true }).click();
  await expect(toast(page)).toHaveText('“torto.png” foi apagado.');
  await expect(internal.getByRole('listitem').filter({ hasText: 'torto.png' })).toHaveCount(0);
  expect(errors).toEqual([]);
});

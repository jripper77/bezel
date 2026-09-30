// The editor in demo mode: it loads without console errors or serious
// accessibility violations (light and dark projects), widgets and sensors
// drag onto the canvas, keyboard edits undo, the screen turns between
// vertical and horizontal, and imports list what had no equivalent.
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

async function dragTo(page, source, target, at = { x: 0.5, y: 0.5 }) {
  const from = await source.boundingBox();
  const to = await target.boundingBox();
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x + from.width / 2 + 20, from.y + from.height / 2 + 20, { steps: 4 });
  await page.mouse.move(to.x + to.width * at.x, to.y + to.height * at.y, { steps: 8 });
  await page.mouse.up();
}

const layers = (page) => page.locator('#layer-list .layer-row');

test('the editor opens the demo theme', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await expect(page.locator('#screen-select')).toContainText('Turing Smart Screen 8.8"');
  await expect(page.locator('#status-main')).toHaveText('Tudo salvo');
  await expect(page.locator('#status-render')).toContainText('Render');
  await expect(page.getByRole('heading', { name: 'Tema' })).toBeVisible();
  const box = await page.locator('#canvas-box').boundingBox();
  expect(Math.round(box.height / box.width)).toBe(4);
  await expectAccessible(page);
  for (const tab of ['Sensores', 'Camadas', 'Temas', 'Mídia', 'Tela']) {
    await page.getByRole('tab', { name: tab }).click();
    await expectAccessible(page);
  }
  expect(errors).toEqual([]);
});

test('drag a widget onto the canvas', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await dragTo(page, page.getByRole('button', { name: 'Adicionar Barra' }), page.locator('#canvas-box'), { x: 0.5, y: 0.6 });
  await expect(page.locator('.sel-box')).toHaveCount(1);
  await expect(page.locator('#status-main')).toHaveText('Alterações não salvas');
  await expect(page.locator('#inspector')).toContainText('Posição e tamanho');
  await page.getByRole('tab', { name: 'Camadas' }).click();
  await expect(layers(page)).toHaveCount(4);
  await expectAccessible(page);

  await page.getByRole('tab', { name: 'Sensores' }).click();
  await page.getByRole('searchbox', { name: 'Buscar sensor' }).fill('gpu temp');
  await dragTo(page, page.getByRole('button', { name: /GPU temperature/ }), page.locator('#canvas-box'), { x: 0.3, y: 0.8 });
  await expect(page.locator('#inspector').getByRole('combobox', { name: 'Sensor', exact: true })).toHaveValue('gpu.temperature');
  await page.getByRole('tab', { name: 'Camadas' }).click();
  await expect(layers(page)).toHaveCount(5);

  // A drop outside the canvas adds nothing; a plain click adds at the center.
  await page.getByRole('tab', { name: 'Widgets' }).click();
  await dragTo(page, page.getByRole('button', { name: 'Adicionar Texto' }), page.locator('#inspector'));
  await page.getByRole('button', { name: 'Adicionar Anel' }).click();
  await page.getByRole('tab', { name: 'Camadas' }).click();
  await expect(layers(page)).toHaveCount(6);
  expect(errors).toEqual([]);
});

test('keyboard move and undo', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await page.getByRole('tab', { name: 'Camadas' }).click();
  await page.getByRole('button', { name: /^CPU/ }).click();
  const x = page.getByRole('spinbutton', { name: 'X', exact: true });
  await expect(x).toHaveValue('90');

  await page.keyboard.press('Shift+ArrowRight');
  await page.keyboard.press('ArrowRight');
  await expect(x).toHaveValue('101');
  await page.keyboard.press('Control+z');
  await expect(x).toHaveValue('100');
  await page.keyboard.press('Control+z');
  await expect(x).toHaveValue('90');
  await expect(page.locator('#status-main')).toHaveText('Tudo salvo');
  await page.keyboard.press('Control+Shift+z');
  await expect(x).toHaveValue('100');

  // Typing in a field does not nudge; the field edit is one undo step.
  await x.fill('120');
  await x.press('Enter');
  await x.press('ArrowLeft');
  await expect(x).toHaveValue('120');
  await page.getByRole('button', { name: 'Desfazer (Ctrl+Z)' }).click();
  await expect(x).toHaveValue('100');

  // Duplicate, then delete, then save.
  await page.getByRole('button', { name: /^CPU/ }).click();
  await page.keyboard.press('Control+d');
  await expect(layers(page)).toHaveCount(4);
  await page.keyboard.press('Delete');
  await expect(layers(page)).toHaveCount(3);
  await page.keyboard.press('Control+s');
  await expect(page.locator('#status-main')).toHaveText('Tudo salvo');
  await expect(page.locator('#toast')).toHaveText('Tema salvo.');
  expect(errors).toEqual([]);
});

test('without a screen, live mode explains itself', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=empty');
  await expect(page.locator('#screen-select')).toContainText('Nenhuma tela conectada');
  await page.getByRole('switch').click({ force: true });
  await expect(page.locator('#toast')).toHaveText('Conecte uma tela para usar o modo ao vivo.');
  await expect(page.getByRole('switch')).not.toBeChecked();
  await page.getByRole('tab', { name: 'Tela' }).click();
  await expect(page.locator('#screen-panel')).toContainText('Confira o cabo USB');
  await expectAccessible(page);
  await page.goto('/index.html?demo=error');
  await expect(page.locator('#status-device')).toContainText('permission denied');
  expect(errors).toEqual([]);
});

test('live mode and start at login', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('switch').click({ force: true });
  await expect(page.getByRole('switch')).toBeChecked();
  await expect(page.locator('#status-device')).toHaveText('ao vivo');
  await page.getByRole('tab', { name: 'Tela' }).click();
  const autostart = page.getByRole('checkbox', { name: /Iniciar com o computador/ });
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

test('switch between vertical and horizontal', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  const group = page.getByRole('group', { name: 'Orientação da tela' });
  const vertical = group.getByRole('button', { name: 'Vertical', exact: true });
  const horizontal = group.getByRole('button', { name: 'Horizontal', exact: true });
  const turn = group.getByRole('button', { name: 'Girar 180°' });
  const orientation = page.locator('#inspector').getByRole('combobox', { name: 'Orientação' });
  await expect(vertical).toHaveAttribute('aria-pressed', 'true');
  await expect(horizontal).toHaveAttribute('aria-pressed', 'false');
  await expect(turn).toHaveAttribute('aria-pressed', 'true');
  await expect(orientation).toHaveValue('reverse-portrait');

  await horizontal.click();
  await expect(horizontal).toHaveAttribute('aria-pressed', 'true');
  await expect(vertical).toHaveAttribute('aria-pressed', 'false');
  await expect(turn).toHaveAttribute('aria-pressed', 'true');
  await expect(orientation).toHaveValue('reverse-landscape');
  await expect(page.locator('#inspector')).toContainText('Tela de 1920×480 pixels');
  const wide = await page.locator('#canvas-box').boundingBox();
  expect(wide.width).toBeGreaterThan(wide.height);
  await page.keyboard.press('Control+a');
  await expect(page.locator('.sel-box')).toHaveCount(3);
  await expectInside(page, '.sel-box', '#canvas-box');
  await page.keyboard.press('Escape');
  await expect(page.locator('#status-main')).toHaveText('Alterações não salvas');
  await expectAccessible(page);

  // One undo step brings the vertical layout back.
  await page.keyboard.press('Control+z');
  await expect(vertical).toHaveAttribute('aria-pressed', 'true');
  await expect(orientation).toHaveValue('reverse-portrait');
  const tall = await page.locator('#canvas-box').boundingBox();
  expect(tall.height).toBeGreaterThan(tall.width);
  await expect(page.locator('#status-main')).toHaveText('Tudo salvo');
  await expect(page.getByRole('button', { name: 'Desfazer (Ctrl+Z)' })).toBeDisabled();

  // The 180° turn toggles, by mouse and by keyboard, and keeps the layout.
  await turn.click();
  await expect(turn).toHaveAttribute('aria-pressed', 'false');
  await expect(orientation).toHaveValue('portrait');
  await expect(page.locator('#inspector')).toContainText('Tela de 480×1920 pixels');
  await turn.focus();
  await page.keyboard.press('Space');
  await expect(turn).toHaveAttribute('aria-pressed', 'true');
  await expect(orientation).toHaveValue('reverse-portrait');

  // The inspector's select is the same command.
  await orientation.selectOption('landscape');
  await expect(horizontal).toHaveAttribute('aria-pressed', 'true');
  await expect(turn).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('#inspector')).toContainText('Tela de 1920×480 pixels');
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('new vertical and horizontal themes', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: 'Temas' }).click();
  const cards = page.locator('#theme-grid .theme-card');
  await expect(cards).toHaveCount(1);
  await expect(cards.first()).toContainText('Vertical');

  await page.getByRole('button', { name: 'Novo horizontal' }).click();
  await expect(page.locator('#theme-name')).toHaveValue('Sem título');
  await expect(page.getByRole('button', { name: 'Horizontal', exact: true })).toHaveAttribute('aria-pressed', 'true');
  const box = await page.locator('#canvas-box').boundingBox();
  expect(box.width).toBeGreaterThan(box.height);
  await page.keyboard.press('Control+s');
  await expect(cards).toHaveCount(2);
  await expect(cards.nth(1)).toContainText('Horizontal');
  await expect(cards.nth(1)).toContainText('1920×480');

  await page.getByRole('button', { name: 'Novo vertical' }).click();
  await expect(page.getByRole('button', { name: 'Vertical', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('#inspector')).toContainText('Tela de 480×1920 pixels');
  await expectAccessible(page);
  expect(errors).toEqual([]);
});

test('an import lists what had no equivalent', async ({ page }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await page.getByRole('tab', { name: 'Temas' }).click();
  await page.getByRole('button', { name: 'Importar…' }).click();
  await expect(page.locator('#theme-name')).toHaveValue('Imported');
  await expect(page.locator('#toast')).toHaveText('Tema importado com 2 avisos. A lista está na aba Temas.');
  const report = page.getByRole('region', { name: 'Avisos da importação' });
  await expect(report).toBeVisible();
  await expect(report.getByRole('listitem')).toHaveCount(2);
  await expect(report).toContainText('backplate LED');
  await expectAccessible(page);
  await report.getByRole('button', { name: 'Fechar avisos' }).click();
  await expect(report).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Importar…' })).toBeFocused();
  expect(errors).toEqual([]);
});

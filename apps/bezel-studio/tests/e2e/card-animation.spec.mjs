import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';

test('card built in the editor previews fade, slide and flip with changing frames and reduced motion', async ({ page, t }) => {
  const errors = watchErrors(page);
  await page.goto('/index.html?demo=turing88');
  await expect(page.locator('#theme-name')).toHaveValue('Demo');
  await expect(page.getByRole('button', { name: t('card.demo'), exact: true })).toHaveCount(0);
  const detail = page.locator('#inspector');
  const add = async widget => {
    await page.locator('#tab-widgets').click();
    await page.getByRole('button', { name: t('library.addWidget', { name: t(`widget.${widget}`) }), exact: true }).click();
  };
  await add('card');
  const cardName = await detail.getByLabel(t('inspector.name'), { exact: true }).inputValue();
  const selectCard = async () => {
    await page.locator('#tab-layers').click();
    await page.locator('.layer-name').filter({ hasText: cardName }).first().click();
  };
  await add('text');
  await selectCard();
  await detail.getByRole('button', { name: t('card.addFace'), exact: true }).click();
  await add('ring');
  await selectCard();
  await detail.getByRole('combobox', { name: t('card.effect'), exact: true }).selectOption('flip');
  await expect(detail.getByRole('combobox', { name: t('card.effect'), exact: true })).toHaveValue('flip');
  const duration = detail.getByRole('slider', { name: new RegExp(t('card.duration')) });
  await duration.evaluate(e => { e.value = '1000'; e.dispatchEvent(new Event('change', { bubbles: true })); });
  await detail.getByRole('combobox', { name: t('card.direction'), exact: true }).selectOption('right');
  for (const effect of ['fade', 'slide', 'flip']) {
    await detail.getByRole('combobox', { name: t('card.effect'), exact: true }).selectOption(effect);
    await page.waitForTimeout(150);
    const sampling = page.evaluate(async () => {
      const source = document.querySelector('#preview');
      const sample = document.createElement('canvas'); sample.width = 48; sample.height = 192;
      const ctx = sample.getContext('2d', { willReadFrequently: true }), hashes = [];
      for (let i = 0; i < 10; i++) {
        ctx.drawImage(source, 0, 0, 48, 192);
        const rgba = ctx.getImageData(0, 0, 48, 192).data;
        let hash = 2166136261; for (let j = 0; j < rgba.length; j++) hash = Math.imul(hash ^ rgba[j], 16777619);
        hashes.push(hash); await new Promise(resolve => setTimeout(resolve, 140));
      }
      return hashes;
    });
    await detail.getByRole('button', { name: t('card.animateNext'), exact: true }).click();
    const hashes = await sampling;
    expect(new Set(hashes).size, effect).toBeGreaterThanOrEqual(4);
    await expect(detail.getByRole('slider', { name: new RegExp(t('card.duration')) })).toHaveValue('1000');
  }
  await detail.getByRole('checkbox', { name: t('card.includeBase'), exact: true }).uncheck();
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await detail.getByRole('button', { name: t('card.animateNext'), exact: true }).click();
  await expectAccessible(page);
  expect(errors).toEqual([]);
  await page.screenshot({ path: test.info().outputPath('card-animation.png') });
});

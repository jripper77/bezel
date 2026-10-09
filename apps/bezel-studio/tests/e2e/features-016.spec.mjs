import { test, expect, watchErrors, expectAccessible } from './helpers.mjs';
const tab = (page,t,key) => page.locator('#inspector').getByRole('tab',{name:t(key),exact:true}).click();
const add = (page,t,widget) => page.getByRole('button',{name:t('library.addWidget',{name:t(`widget.${widget}`)}),exact:true}).click();
test('image and shape share radial controls; player and weather have their own appearance',async ({page,t})=>{
 const errors=watchErrors(page);await page.goto('/index.html?demo=turing88');
 for(const widget of ['shape','image']) {
  await add(page,t,widget);
  if(widget==='image') await tab(page,t,'inspector.tab.look');
  const mode=page.locator('#inspector').getByRole('combobox',{name:t('fade.mode'),exact:true});
  await mode.selectOption('radial');
  const center=page.locator('#inspector').getByRole('spinbutton',{name:t('fade.centerX'),exact:true});
  await expect(center).toHaveValue('50');await center.fill('25');await center.press('Tab');await expect(center).toHaveValue('25');
  await mode.selectOption('linear');await expect(page.locator('#inspector').getByRole('spinbutton',{name:t('shape.fadeAngle'),exact:true})).toBeVisible();
 }
 await add(page,t,'player');await expect(page.locator('#inspector').getByRole('textbox',{name:t('player.source'),exact:true})).toBeVisible();
 await tab(page,t,'inspector.tab.look');await page.locator('#inspector').getByRole('checkbox',{name:t('player.showCover'),exact:true}).uncheck();
 await add(page,t,'weather');await tab(page,t,'inspector.tab.look');const style=page.locator('#inspector').getByRole('combobox',{name:t('weather.iconStyle'),exact:true});
 await style.selectOption('colored');await expect(style).toHaveValue('colored');await style.selectOption('dimensional');await expect(style).toHaveValue('dimensional');
 await expectAccessible(page);expect(errors).toEqual([]);
});
test('card trigger edits, priorities and target faces survive Undo',async ({page,t})=>{
 const errors=watchErrors(page);await page.goto('/index.html?demo=turing88');await add(page,t,'card');
 const inspector=page.locator('#inspector');await tab(page,t,'card.tab.triggers');await expect(inspector.getByRole('button',{name:t('trigger.add'),exact:true})).toBeDisabled();
 await tab(page,t,'card.tab.faces');await inspector.getByRole('button',{name:t('card.addFace'),exact:true}).click();
 await tab(page,t,'card.tab.triggers');await inspector.getByRole('button',{name:t('trigger.add'),exact:true}).click();
 const condition=inspector.getByRole('combobox',{name:t('trigger.condition'),exact:true});await expect(condition).toHaveValue('mediaPlaying');
 await condition.selectOption('foreground');await expect(condition).toHaveValue('foreground');
 const priority=inspector.getByRole('spinbutton',{name:t('trigger.priority'),exact:true});await priority.fill('7');await priority.press('Tab');await expect(priority).toHaveValue('7');
 await page.locator('#undo').click();await expect(priority).toHaveValue('0');
 await inspector.getByRole('button',{name:t('trigger.remove'),exact:true}).click();await expect(condition).toHaveCount(0);
 await expectAccessible(page);expect(errors).toEqual([]);
});

test('sleep timer is discoverable, defaults to five and remembers edits',async ({page,t})=>{
 const errors=watchErrors(page);await page.goto('/index.html?demo=turing88');await expect(page.locator('#theme-name')).toHaveValue('Demo');
 await page.locator('#tab-screen').click();await page.locator('#configure-sleep-timer').click();
 const dialog=page.getByRole('dialog',{name:t('standby.off.title'),exact:true});const minutes=dialog.getByRole('combobox',{name:t('standby.off.minutes'),exact:true});
 await expect(minutes).toHaveValue('5');await minutes.selectOption('7');await dialog.getByRole('button',{name:t('standby.write'),exact:true}).click();
 await expect(dialog).toHaveCount(0);await page.locator('#configure-sleep-timer').click();await expect(minutes).toHaveValue('7');await dialog.getByRole('button',{name:t('dialog.cancel'),exact:true}).click();
 await expectAccessible(page);expect(errors).toEqual([]);
});
test('property navigation fits small and large windows',async ({page,t})=>{
 const errors=watchErrors(page);await page.goto('/index.html?demo=turing88');
 for(const size of [{width:1000,height:700},{width:1600,height:1000}]) {
  await page.setViewportSize(size);await page.locator('#tab-widgets').click();await add(page,t,'player');
  for(const pane of ['.sidebar','.rail','.library']) expect(await page.locator(pane).evaluate(n=>n.scrollWidth<=n.clientWidth+1),pane).toBe(true);
  expect(await page.locator('#inspector').evaluate(n=>n.scrollWidth<=n.clientWidth+1)).toBe(true);
 }
 await page.screenshot({path:'../../target/features-016-ui.png'});await expectAccessible(page);expect(errors).toEqual([]);
});

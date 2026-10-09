import {test,expect,watchErrors,expectAccessible} from './helpers.mjs';
const add=(page,t,widget)=>page.getByRole('button',{name:t('library.addWidget',{name:t(`widget.${widget}`)}),exact:true}).click();
test('canvas focus restores nudge and cut-paste, context menu groups objects',async({page,t})=>{
 const errors=watchErrors(page);await page.goto('/index.html?demo=turing88');await add(page,t,'shape');
 const input=page.locator('#inspector').getByRole('spinbutton',{name:t('inspector.x'),exact:true});const original=Number(await input.inputValue());await input.focus();
 const selected=page.locator('.sel-box');const b=await selected.boundingBox();await page.mouse.click(b.x+b.width/2,b.y+b.height/2);await page.keyboard.press('ArrowRight');await expect(input).toHaveValue(String(original+1));
 await page.keyboard.press('Control+x');await expect(selected).toHaveCount(0);await page.keyboard.press('Control+v');await expect(selected).toHaveCount(1);await expect(input).toHaveValue(String(original+1));
 await page.locator('#tab-widgets').click();await add(page,t,'text');await page.locator('#tab-layers').click();const rows=page.locator('#layer-list .layer-row');await rows.first().locator('.layer-name').click();await rows.nth(1).locator('.layer-name').click({modifiers:['Control']});
 await rows.first().click({button:'right'});const menu=page.getByRole('menu',{name:t('menu.title')});await expect(menu).toBeVisible();await menu.getByRole('menuitem',{name:t('group.create'),exact:true}).click();await expect(page.locator('[data-group-id]')).toHaveCount(1);
 await page.screenshot({path:'../../target/editing-017-layers.png'});
 await expectAccessible(page);expect(errors).toEqual([]);
});
test('layers allow dropping into another face and back to root, including Undo',async({page,t})=>{
 const errors=watchErrors(page);await page.goto('/index.html?demo=turing88');await add(page,t,'card');await page.locator('#inspector').getByRole('button',{name:t('card.addFace'),exact:true}).click();await add(page,t,'shape');await page.locator('#tab-layers').click();
 const card=page.locator('[data-card-id]');const row=card.locator('[data-face="1"] .layer-row');const source=await row.boundingBox(),target=await card.locator('[data-face="0"] .layer-group-heading').boundingBox();
 await page.mouse.move(source.x+30,source.y+source.height/2);await page.mouse.down();await page.mouse.move(target.x+30,target.y+target.height/2,{steps:10});await expect(page.locator('.layer-drop-inside')).toHaveCount(1);await page.mouse.up();await expect(card.locator('[data-face="0"] .layer-row')).toHaveCount(1);
 await page.locator('#undo').click();await expect(card.locator('[data-face="1"] .layer-row')).toHaveCount(1);
 await card.locator(':scope > .layer-list > .layer-row').click({button:'right'});await page.keyboard.press('Escape');
 await expectAccessible(page);expect(errors).toEqual([]);
});
test('corner controls, player spacing and explicit trigger return are editable',async({page,t})=>{
 const errors=watchErrors(page);await page.goto('/index.html?demo=turing88');await add(page,t,'shape');const inspector=page.locator('#inspector');await inspector.getByRole('checkbox',{name:t('inspector.individualCorners'),exact:true}).check();const tl=inspector.getByRole('spinbutton',{name:t('inspector.topLeft'),exact:true});await tl.fill('30');await tl.press('Tab');await expect(tl).toHaveValue('30');
 await add(page,t,'player');await inspector.getByRole('tab',{name:t('inspector.tab.look'),exact:true}).click();const gap=inspector.getByRole('spinbutton',{name:t('player.coverGap'),exact:true});await gap.fill('25');await gap.press('Tab');await expect(gap).toHaveValue('25');await expect(inspector.getByRole('spinbutton',{name:t('player.coverRadius'),exact:true})).toBeVisible();
 await inspector.getByRole('checkbox',{name:t('inspector.individualCorners'),exact:true}).check();const coverTL=inspector.getByRole('spinbutton',{name:t('inspector.topLeft'),exact:true});await coverTL.fill('30');await coverTL.press('Tab');await expect(coverTL).toHaveValue('30');await expect(inspector.getByRole('spinbutton',{name:t('inspector.topRight'),exact:true})).toHaveValue('0');
 await page.locator('#undo').click();await expect(coverTL).toHaveValue('0');await inspector.getByRole('checkbox',{name:t('inspector.individualCorners'),exact:true}).uncheck();await expect(inspector.getByRole('spinbutton',{name:t('player.coverRadius'),exact:true})).toBeVisible();
 await add(page,t,'card');await inspector.getByRole('button',{name:t('card.addFace'),exact:true}).click();await inspector.getByRole('tab',{name:t('card.tab.triggers'),exact:true}).click();await inspector.getByRole('button',{name:t('trigger.add'),exact:true}).click();const ret=inspector.getByRole('combobox',{name:t('trigger.returnFace'),exact:true});await ret.selectOption('0');await expect(ret).toHaveValue('0');await page.locator('#undo').click();await expect(ret).toHaveValue('previous');
 await expectAccessible(page);expect(errors).toEqual([]);
});

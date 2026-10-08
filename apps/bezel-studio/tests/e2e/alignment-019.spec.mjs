import {test,expect,watchErrors,expectAccessible} from './helpers.mjs';
test('alignment references the first clicked object rather than paint order',async({page,t})=>{
 const errors=watchErrors(page);await page.goto('/index.html?demo=turing88');await expect(page.locator('#theme-name')).toHaveValue('Demo');
 const field=(key)=>page.locator('#inspector').getByRole('spinbutton',{name:t(`inspector.${key}`),exact:true});
 for(const values of [{x:100,y:1000,width:100,height:100},{x:300,y:1200,width:60,height:80}]){
  await page.locator('#tab-widgets').click();await page.getByRole('button',{name:t('library.addWidget',{name:t('widget.shape')}),exact:true}).click();
  for(const [key,value]of Object.entries(values)){await field(key).fill(String(value));await field(key).press('Tab');}
 }
 await page.locator('#tab-layers').click();const first=page.locator('.layer-row[data-element-id="5"] .layer-name'),second=page.locator('.layer-row[data-element-id="4"] .layer-name');
 await first.click();await second.click({modifiers:['Control']});await expect(page.locator('.sel-box')).toHaveCount(2);
 await page.getByRole('button',{name:t('align.centerX'),exact:true}).click();await first.click();await expect(field('x')).toHaveValue('300');await second.click();await expect(field('x')).toHaveValue('280');await page.locator('#undo').click();await expect(field('x')).toHaveValue('100');
 await first.click();await second.click({modifiers:['Control']});await page.getByRole('button',{name:t('align.centerY'),exact:true}).click();await first.click();await expect(field('y')).toHaveValue('1200');await second.click();await expect(field('y')).toHaveValue('1190');
 await expectAccessible(page);expect(errors).toEqual([]);
});

import test from 'node:test';
import assert from 'node:assert/strict';
import { filterIcons, iconSvg, iconUrl, readIcon, loadIconCatalog } from '../../src/icon-catalog.js';

import catalog from '../../src/assets/tabler/icons.js';

test('offline catalog is complete, unique and searchable in Italian', () => {
  assert.equal(catalog.icons.length, 6220);
  assert.equal(new Set(catalog.icons.map((item) => item.id)).size, 6220);
  for (const query of ['cpu', 'ventola', 'temperatura', 'processore', 'rete', 'memoria', 'disco', 'schermo', 'gpu', 'energia', 'potenza', 'luce', 'led', 'orologio', 'gradi', 'fan', 'ram']) {
    assert.ok(filterIcons(catalog.icons, query).length > 0, query);
  }
  assert.ok(filterIcons(catalog.icons, ' CPU ', 'outline').every((item) => item.style === 'outline'));
  assert.ok(filterIcons(catalog.icons, '', 'filled').length > 0);
  assert.equal(filterIcons(catalog.icons, 'does-not-exist').length, 0);
  assert.ok(filterIcons(catalog.icons, 'cpu 2').every((item) => item.id.includes('cpu') && item.id.includes('2')));
});

test('selected icon stays SVG with explicit paint and validated options', () => {
  const item = catalog.icons.find((item) => item.id === 'cpu');
  const svg = iconSvg(item, '#38bdf8', 1.5);
  assert.match(svg, /stroke="#38bdf8"/);
  assert.match(svg, /stroke-width="1.5"/);
  assert.ok(!svg.includes('currentColor'));
  assert.match(iconSvg(item), /#ffffff/);
  assert.equal(decodeURIComponent(iconUrl(svg).split(',', 2)[1]), svg);
  for (const color of ['red', '#ff', '#fff<script>']) assert.throws(() => iconSvg(item, color));
  for (const width of [0, 5, NaN, Infinity]) assert.throws(() => iconSvg(item, '#ffffff', width));
});


test('icon paint and shadow persist in SVG and old icons remain editable', () => {
  const item = catalog.icons.find((item) => item.id === 'cpu');
  const svg = iconSvg(item, '#38bdf880', 3, { shadow: 2, shadowColor: '#ff000080' });
  assert.deepEqual(readIcon(svg), { id: 'cpu', style: 'outline', color: '#38bdf880', stroke: 3, shadow: 2, shadowColor: '#ff000080' });
  assert.match(svg, /feDropShadow/);
  assert.match(svg, /viewBox="-8 -8 40 40"/);
  assert.equal(readIcon(item.svg.replaceAll('currentColor', '#ffffff')).id, 'cpu');
  const filled = catalog.icons.find((item) => item.style === 'filled');
  assert.equal(readIcon(filled.svg.replaceAll('currentColor', '#ffffff')).id, filled.id);
  assert.equal(readIcon(null), null);
  assert.equal(readIcon('<svg/>'), null);
  assert.equal(readIcon(svg.replace('data-bezel-shadow="2"', 'data-bezel-shadow="NaN"')), null);
  for (const shadow of [-1, 5, NaN]) assert.throws(() => iconSvg(item, '#ffffff', 2, { shadow }));
  assert.throws(() => iconSvg(item, '#ffffff', 2, { shadowColor: 'red' }));
  assert.ok(!iconSvg(item).includes('feDropShadow'));
});


test('hardware collection adds pumps and Italian electrical searches find primary symbols', async () => {
  const all = await loadIconCatalog();
  assert.equal(all.icons.length, 6595);
  assert.equal(new Set(all.icons.map(i => i.id)).size, all.icons.length);
  assert.equal((await loadIconCatalog()), all);
  assert.ok(filterIcons(all.icons, 'fulmine').some(i => i.id === 'bolt'));
  assert.ok(filterIcons(all.icons, 'fulmine per elettricit\u00e0').some(i => i.id === 'bolt'));
  assert.ok(['bolt', 'bolt-filled', 'mdi-lightning-bolt', 'mdi-flash'].includes(filterIcons(all.icons, 'fulmine')[0].id));
  assert.ok(filterIcons(all.icons, 'pompa di raffreddamento').some(i => i.id === 'mdi-water-pump'));
  for (const q of ['pompa', 'radiatore', 'valvola', 'tubi', 'flusso', 'liquido', 'acqua']) assert.ok(filterIcons(all.icons, q).length, q);
  assert.equal(filterIcons(all.icons, '', 'all', 'mdi').length, 375);
  assert.equal(filterIcons(all.icons, '', 'outline', 'mdi').length, 0);
  assert.equal(filterIcons(all.icons, '', 'all', 'tabler').length, 6220);
  const pump = all.icons.find(i => i.id === 'mdi-water-pump');
  const svg = iconSvg(pump, '#38bdf880', 2, { shadow: 1.5, shadowColor: '#00000080' });
  assert.deepEqual(readIcon(svg), { id: pump.id, style: 'filled', color: '#38bdf880', stroke: 2, shadow: 1.5, shadowColor: '#00000080' });
});

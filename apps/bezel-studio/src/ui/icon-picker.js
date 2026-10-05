import { el } from './dom.js';
import { makeDraggable } from './dragdrop.js';
import { filterIcons, iconSvg, iconUrl, loadIconCatalog } from '../icon-catalog.js';

/** Searchable offline catalog under Media > Icons. */
export function createIconPicker({ store, canvas, stage, t, add }) {
  const host = document.getElementById('media-icons');
  const PAGE = 120;
  let catalog = null;
  let loading = null;
  let limit = PAGE;
  let query = '';
  let color = '#ffffff';
  let style = 'all';
  let source = 'all';
  let size = 64;
  let stroke = 2;
  const search = el('input', { type: 'search', id: 'icon-search', oninput: () => { query = search.value; limit = PAGE; render(); } });
  const colorInput = el('input', { type: 'color', value: color, oninput: () => { color = colorInput.value; render(); } });
  const styleInput = el('select', { onchange: () => { style = styleInput.value; limit = PAGE; render(); } });
  const sizeInput = el('input', { type: 'number', min: 8, max: 1024, step: 1, value: size,
    onchange: () => { size = Math.round(Math.min(1024, Math.max(8, Number(sizeInput.value) || 64))); sizeInput.value = size; } });
  const strokeInput = el('input', { type: 'number', min: 0.5, max: 4, step: 0.5, value: stroke,
    onchange: () => { stroke = Math.min(4, Math.max(0.5, Number(strokeInput.value) || 2)); strokeInput.value = stroke; render(); } });
  const sourceInput = el('select', { id: 'icon-source', onchange: () => { source = sourceInput.value; limit = PAGE; render(); } });
  const count = el('p', { class: 'hint', role: 'status' });
  const grid = el('div', { class: 'icon-grid', id: 'icon-grid' });
  const more = el('button', { type: 'button', class: 'text-button', onclick: () => { limit += PAGE; render(); } });
  const notice = el('p', { class: 'hint' });
  const field = (key, input) => el('label', { class: 'icon-option' }, [el('span', { text: t(key) }), input]);

  function render() {
    if (!catalog) return;
    const found = filterIcons(catalog.icons, query, style, source);
    count.textContent = t('icons.count', { shown: Math.min(limit, found.length), count: found.length });
    grid.replaceChildren(...found.slice(0, limit).map((item) => {
      const label = item.name.replaceAll('-', ' ') + (item.provider === 'mdi' ? ' (MDI)' : '');
      const svg = iconSvg(item, color, stroke);
      const button = el('button', { type: 'button', class: 'icon-choice', title: `${label} (${item.style})`,
        'aria-label': `${label} (${item.style})`, dataset: { icon: item.id } }, [
        el('img', { src: iconUrl(svg), alt: '', width: 32, height: 32, draggable: 'false' }),
        el('span', { text: label }),
      ]);
      const insert = (at) => add(item, svg, at, size);
      makeDraggable(button, { label, canvas, stage, onDrop: insert, onClick: () => {
        const c = store.getState().theme.canvas;
        insert({ x: c.width / 2, y: c.height / 2 });
      } });
      return button;
    }));
    more.hidden = found.length <= limit;
  }

  function retranslate() {
    search.placeholder = t('icons.search');
    search.setAttribute('aria-label', t('icons.search'));
    styleInput.replaceChildren(...['all', 'outline', 'filled'].map((value) => el('option', { value, text: t(`icons.${value}`) })));
    styleInput.value = style;
    more.textContent = t('icons.more');
    notice.textContent = t('icons.notice');
    sourceInput.replaceChildren(...[['all', t('icons.all')], ['tabler', t('icons.tabler')], ['mdi', t('icons.mdi')]].map(([value, text]) => el('option', { value, text })));
    sourceInput.value = source;
    host.replaceChildren(search,
      el('div', { class: 'icon-options' }, [field('icons.source', sourceInput), field('icons.style', styleInput), field('icons.color', colorInput),
        field('icons.size', sizeInput), field('icons.stroke', strokeInput)]), count, grid, more, notice);
    render();
  }

  async function show() {
    if (catalog) return;
    if (loading) return loading;
    count.textContent = t('icons.loading');
    loading = (async () => {
      try {
        // Embedded modules are allowed by Studio's script policy; local fetch is not.
        catalog = await loadIconCatalog();
        render();
      } catch {
        count.textContent = t('icons.error');
      } finally { loading = null; }
    })();
    return loading;
  }
  retranslate();
  return { show, retranslate };
}

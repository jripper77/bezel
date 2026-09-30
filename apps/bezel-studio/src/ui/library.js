// The left library: widgets, sensors, layers, themes, media and screen, each a
// tab panel. Widgets and sensors drag onto the canvas.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { makeDraggable } from './dragdrop.js';
import { checkField } from './fields.js';
import { WIDGETS, widgetOf } from '../editor/widgets.js';

const CATEGORY_ORDER = ['cpu', 'gpu', 'memory', 'disk', 'network', 'board', 'system'];

/** Groups catalog entries by category, in display order, filtered by text. */
export function groupSensors(catalog, filter = '') {
  const q = filter.trim().toLowerCase();
  const groups = new Map();
  for (const s of catalog) {
    if (q && !`${s.label} ${s.key}`.toLowerCase().includes(q)) continue;
    if (!groups.has(s.category)) groups.set(s.category, []);
    groups.get(s.category).push(s);
  }
  const rank = (c) => (CATEGORY_ORDER.includes(c) ? CATEGORY_ORDER.indexOf(c) : CATEGORY_ORDER.length);
  return [...groups.entries()].sort((a, b) => rank(a[0]) - rank(b[0]) || a[0].localeCompare(b[0]));
}

/**
 * @param {object} deps
 */
export function createLibrary({ store, canvas, stage, t, actions }) {
  const $ = (id) => document.getElementById(id);
  let catalog = [];
  let readings = {};
  let editing = null;

  // ---------------------------------------------------------------- tabs --
  const tabs = [...document.querySelectorAll('.tabs [role="tab"]')];
  function selectTab(tab) {
    for (const t2 of tabs) {
      const on = t2 === tab;
      t2.setAttribute('aria-selected', String(on));
      t2.tabIndex = on ? 0 : -1;
      $(t2.getAttribute('aria-controls')).hidden = !on;
    }
  }
  tabs.forEach((tab, i) => {
    tab.addEventListener('click', () => selectTab(tab));
    tab.addEventListener('keydown', (evt) => {
      const step = evt.key === 'ArrowRight' ? 1 : evt.key === 'ArrowLeft' ? -1 : 0;
      if (!step) return;
      const next = tabs[(i + step + tabs.length) % tabs.length];
      selectTab(next);
      next.focus();
    });
  });

  const center = () => {
    const c = store.getState().theme.canvas;
    return { x: c.width / 2, y: c.height / 2 };
  };

  // ------------------------------------------------------------- widgets --
  function renderWidgets() {
    const grid = $('widget-grid');
    grid.replaceChildren(
      ...WIDGETS.map((w) => {
        const label = t(`widget.${w}`);
        const tile = el('button', { type: 'button', class: 'widget-tile', dataset: { widget: w }, 'aria-label': t('library.addWidget', { name: label }) }, [icon(ICONS[w], 26), el('span', { text: label })]);
        makeDraggable(tile, {
          label,
          canvas,
          stage,
          onDrop: (p) => store.dispatch('add', { widget: w, x: p.x, y: p.y }),
          onClick: () => store.dispatch('add', { widget: w, ...center() }),
        });
        return el('li', {}, [tile]);
      }),
    );
  }

  // ------------------------------------------------------------- sensors --
  function sensorRow(s) {
    const r = readings[s.key];
    const value = el('span', { class: `value${r?.unavailable ? ' unavailable' : ''}`, dataset: { valueFor: s.key }, text: r?.display ?? '…' });
    const row = el('button', { type: 'button', class: 'sensor-row', dataset: { key: s.key }, 'aria-label': t('library.addSensor', { name: s.label }) }, [
      el('span', { class: 'label', text: s.label }),
      value,
      el('span', { class: 'key', text: s.key }),
    ]);
    const sensor = { key: s.key, quantity: s.quantity, label: s.label };
    makeDraggable(row, {
      label: s.label,
      canvas,
      stage,
      onDrop: (p) => store.dispatch('add', { widget: 'value', x: p.x, y: p.y, sensor }),
      onClick: () => store.dispatch('add', { widget: 'value', ...center(), sensor }),
    });
    return row;
  }

  function renderSensors() {
    const list = $('sensor-list');
    const groups = groupSensors(catalog, $('sensor-search').value);
    if (groups.length === 0) {
      list.replaceChildren(el('p', { class: 'empty-note', text: t('library.noSensors') }));
      return;
    }
    list.replaceChildren(...groups.flatMap(([cat, items]) => [el('h3', { text: t(`category.${cat}`) }), ...items.map(sensorRow)]));
  }

  function updateReadings(next) {
    readings = next;
    for (const span of document.querySelectorAll('[data-value-for]')) {
      const r = readings[span.dataset.valueFor];
      span.textContent = r?.display ?? '…';
      span.classList.toggle('unavailable', Boolean(r?.unavailable));
    }
  }

  $('sensor-search').addEventListener('input', renderSensors);

  // -------------------------------------------------------------- layers --
  function layerRow(e, index, total) {
    const selected = store.getState().selection.includes(e.id);
    const name = editing === e.id
      ? el('input', {
        type: 'text', value: e.name, 'aria-label': t('inspector.name'), class: 'layer-name',
        onkeydown: (evt) => {
          if (evt.key === 'Enter') evt.target.blur();
          if (evt.key === 'Escape') { editing = null; renderLayers(); }
        },
        onblur: (evt) => {
          editing = null;
          if (evt.target.value.trim()) store.dispatch('update', { id: e.id, patch: { name: evt.target.value.trim() } });
          else renderLayers();
        },
      })
      : el('button', {
        type: 'button', class: 'layer-name', 'aria-pressed': String(selected),
        onclick: (evt) => store.select(evt.shiftKey ? [...new Set([...store.getState().selection, e.id])] : [e.id]),
        ondblclick: () => { editing = e.id; renderLayers(); document.querySelector('.layer-list input')?.focus(); },
      }, [e.name, el('small', { text: t(`widget.${widgetOf(e)}`) })]);
    const btn = (label, paths, onclick, disabled = false, pressed = null) => el('button', { type: 'button', class: 'icon-button', title: label, 'aria-label': label, disabled, 'aria-pressed': pressed === null ? null : String(pressed), onclick }, [icon(paths)]);
    return el('li', { class: `layer-row${selected ? ' selected' : ''}${e.visible === false ? ' hidden-el' : ''}` }, [
      name,
      btn(e.visible === false ? t('layers.show') : t('layers.hide'), e.visible === false ? ICONS.eyeOff : ICONS.eye, () => store.dispatch('update', { id: e.id, patch: { visible: e.visible === false } }), false, e.visible !== false),
      btn(e.locked ? t('layers.unlock') : t('layers.lock'), e.locked ? ICONS.lock : ICONS.unlock, () => store.dispatch('update', { id: e.id, patch: { locked: !e.locked } }), false, Boolean(e.locked)),
      btn(t('layers.up'), ICONS.up, () => store.dispatch('reorder', { id: e.id, index: index + 1 }), index === total - 1),
      btn(t('layers.down'), ICONS.down, () => store.dispatch('reorder', { id: e.id, index: index - 1 }), index === 0),
    ]);
  }

  function renderLayers() {
    const els = store.getState().theme.elements;
    const list = $('layer-list');
    if (els.length === 0) {
      list.replaceChildren(el('li', { class: 'empty-note', text: t('layers.empty') }));
      return;
    }
    // Topmost first, like every design tool.
    list.replaceChildren(...els.map((e, i) => layerRow(e, i, els.length)).reverse());
  }

  // -------------------------------------------------------------- themes --
  function renderThemes(list) {
    const grid = $('theme-grid');
    if (!list.length) {
      grid.replaceChildren(el('li', { class: 'empty-note', text: t('themes.empty') }));
      return;
    }
    grid.replaceChildren(...list.map((th) => el('li', {}, [
      el('button', { type: 'button', class: 'theme-card', onclick: () => actions.openTheme(th.location) }, [
        el('span', { class: 'thumb', style: th.thumbnail ? { backgroundImage: `url(${th.thumbnail})` } : {} }),
        el('strong', { text: th.name }),
        el('small', { text: `${th.canvas.width}×${th.canvas.height}${th.bundled ? ` · ${t('themes.bundled')}` : ''}` }),
      ]),
    ])));
  }
  $('theme-new').addEventListener('click', () => actions.newTheme());
  $('theme-open').addEventListener('click', () => actions.refreshThemes());
  $('theme-import').addEventListener('click', () => actions.importTheme());

  // --------------------------------------------------------------- media --
  function renderMedia(assets) {
    const list = $('media-list');
    const images = assets.filter((a) => a.kind === 'image');
    if (!images.length) {
      list.replaceChildren(el('li', { class: 'empty-note', text: t('media.empty') }));
      return;
    }
    list.replaceChildren(...images.map((a) => el('li', { class: 'media-item' }, [
      el('span', { class: 'thumb', style: a.dataUrl ? { backgroundImage: `url(${a.dataUrl})` } : {} }),
      el('span', { text: a.ref.split('/').pop() }),
      el('div', { class: 'actions' }, [
        el('button', { type: 'button', class: 'text-button', text: t('media.useBackground'), onclick: () => store.dispatch('setTheme', { patch: { background: { type: 'image', asset: a.ref, fit: 'cover' } } }) }),
        el('button', { type: 'button', class: 'text-button', text: t('media.addImage'), onclick: () => {
          const { x, y } = center();
          store.dispatch('add', { widget: 'image', x, y });
          const id = store.getState().selection[0];
          store.dispatch('update', { id, patch: { kind: { asset: a.ref } } });
        } }),
      ]),
    ])));
  }
  $('media-add').addEventListener('click', () => actions.addImage());

  // -------------------------------------------------------------- screen --
  const autostartField = () => checkField(t('screen.autostart'), actions.autostart(), (on) => actions.setAutostart(on));

  function renderScreen(screens, current, live) {
    const root = $('screen-panel');
    if (!screens.length) {
      root.replaceChildren(el('p', { class: 'empty-note', text: t('screen.none') }), autostartField());
      return;
    }
    root.replaceChildren(autostartField(), ...screens.map((s) => {
      const model = s.models.length === 1 ? s.models[0] : null;
      const slider = el('input', { type: 'range', min: 0, max: 100, step: 1, value: '70', 'aria-label': t('screen.brightness') });
      slider.addEventListener('change', () => actions.setBrightness(s.key, Number(slider.value)));
      return el('section', { class: 'screen-card', 'aria-label': model ? model.name : s.key }, [
        el('strong', { text: model ? model.name : s.models.map((m) => m.name).join(' / ') }),
        el('span', { class: 'meta', text: `${s.key} · ${t(`screen.state.${s.state}`)}${s.key === current && live ? ` · ${t('status.live')}` : ''}` }),
        el('label', { class: 'field' }, [el('span', { text: t('screen.brightness') }), slider]),
        el('div', { class: 'button-row' }, [
          el('button', { type: 'button', class: 'text-button', text: t('screen.release'), onclick: () => actions.release(s.key) }),
        ]),
      ]);
    }));
  }

  return {
    renderWidgets,
    setCatalog(next) {
      catalog = next;
      renderSensors();
    },
    updateReadings,
    renderLayers,
    renderThemes,
    renderMedia,
    renderScreen,
    selectTab: (name) => selectTab(tabs.find((x) => x.dataset.tab === name)),
  };
}

// The left library: widgets, sensors, layers, themes, media and screen, each a
// tab panel. Widgets and sensors drag onto the canvas.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { makeDraggable } from './dragdrop.js';
import { checkField } from './fields.js';
import { WIDGETS, widgetOf } from '../editor/widgets.js';
import { isHorizontal } from '../editor/geometry.js';
import { backgroundOf, fileNameOf, mediaItems, moves, videoFacts } from '../editor/background.js';
import { warningText } from '../messages.js';
import { formatBytes } from './storage.js';

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
 * The keys of the sensors the list shows: those the filter keeps while the
 * list is open (its tab selected, the window visible), none otherwise. The
 * app measures them with the theme's; `net.ping` sends packets only while
 * something shown uses it (D-2026-09-30-release-polish-11).
 */
export function shownSensorKeys(catalog, filter, open) {
  if (!open) return [];
  return groupSensors(catalog, filter).flatMap(([, items]) => items.map((s) => s.key));
}

/**
 * The miniature screen drawn in a theme card's 4:3 thumbnail: the canvas
 * shape scaled into 80% of the box, as percentages of its width and height.
 * @param {{width:number, height:number}} canvas
 */
export function thumbScreen(canvas) {
  const scale = Math.min(3.2 / canvas.width, 2.4 / canvas.height);
  const pct = (v) => Math.round(v * 1000) / 10;
  return { width: pct((canvas.width * scale) / 4), height: pct((canvas.height * scale) / 3) };
}

/** `vertical` or `horizontal` for a theme entry (by orientation, else by shape). */
export function axisOf(entry) {
  if (entry.orientation) return isHorizontal(entry.orientation) ? 'horizontal' : 'vertical';
  return entry.canvas.width > entry.canvas.height ? 'horizontal' : 'vertical';
}

/**
 * @param {object} deps
 */
export function createLibrary({ store, canvas, stage, t, locale = () => 'en', actions }) {
  const $ = (id) => document.getElementById(id);
  let catalog = [];
  let readings = {};
  let editing = null;
  // What each panel shows, to draw it again in another language.
  let themes = [];
  let media = [];
  let importReport = null;
  let screenArgs = [[], null, false, {}, [], {}];

  // ---------------------------------------------------------------- tabs --
  const tabs = [...document.querySelectorAll('.tabs [role="tab"]')];
  function selectTab(tab) {
    for (const t2 of tabs) {
      const on = t2 === tab;
      t2.setAttribute('aria-selected', String(on));
      t2.tabIndex = on ? 0 : -1;
      $(t2.getAttribute('aria-controls')).hidden = !on;
    }
    reportShown();
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

  // What the list shows, told to the app when it changes.
  let reported = null;
  function reportShown() {
    const open = !$('panel-sensors').hidden && document.visibilityState !== 'hidden';
    const keys = shownSensorKeys(catalog, $('sensor-search').value, open);
    if (keys.join(' ') === reported) return;
    reported = keys.join(' ');
    actions.showSensors(keys);
  }
  document.addEventListener('visibilitychange', reportShown);

  function renderSensors() {
    reportShown();
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
  function themeCard(th) {
    const axis = axisOf(th);
    const mini = thumbScreen(th.canvas);
    const screen = { width: `${mini.width}%`, height: `${mini.height}%` };
    if (th.thumbnail) screen.backgroundImage = `url(${th.thumbnail})`;
    return el('li', {}, [
      el('button', { type: 'button', class: 'theme-card', onclick: () => actions.openTheme(th.location) }, [
        el('span', { class: 'thumb', 'aria-hidden': 'true' }, [el('span', { class: 'thumb-screen', style: screen })]),
        el('strong', { text: th.name }),
        el('span', { class: 'card-meta' }, [
          el('span', { class: `badge ${axis}` }, [icon(ICONS[axis], 14), t(`axis.${axis}`)]),
          el('small', { text: `${th.canvas.width}×${th.canvas.height}${th.bundled ? ` · ${t('themes.bundled')}` : ''}` }),
        ]),
      ]),
    ]);
  }

  function renderThemes(list) {
    themes = list;
    const grid = $('theme-grid');
    if (!list.length) {
      grid.replaceChildren(el('li', { class: 'empty-note', text: t('themes.empty') }));
      return;
    }
    grid.replaceChildren(...list.map(themeCard));
  }

  /** Lists what an import could not map exactly, until dismissed; `null` clears it. */
  function showImportReport(report) {
    importReport = report;
    const root = $('import-report');
    if (!report?.warnings?.length) {
      root.replaceChildren();
      return;
    }
    const count = report.warnings.length;
    const dismiss = el('button', {
      type: 'button', class: 'icon-button', title: t('import.dismiss'), 'aria-label': t('import.dismiss'),
      onclick: () => { showImportReport(null); $('theme-import').focus(); },
    }, [icon(ICONS.close, 16)]);
    root.replaceChildren(el('section', { class: 'notice', 'aria-labelledby': 'import-report-title' }, [
      el('div', { class: 'notice-head' }, [icon(ICONS.warning, 18), el('h2', { id: 'import-report-title', text: t('import.title') }), dismiss]),
      el('p', { text: count === 1 ? t('import.summaryOne', { name: report.name }) : t('import.summary', { name: report.name, count }) }),
      el('ul', {}, report.warnings.map((w) => el('li', { text: warningText(t, w) }))),
    ]));
  }

  $('theme-new-vertical').addEventListener('click', () => actions.newTheme('vertical'));
  $('theme-new-horizontal').addEventListener('click', () => actions.newTheme('horizontal'));
  $('theme-open').addEventListener('click', () => actions.refreshThemes());
  $('theme-import').addEventListener('click', () => actions.importTheme());

  // --------------------------------------------------------------- media --
  /**
   * One picture or video of the theme. A video, or an animated GIF, shows
   * its poster, its kind, play time and size, and becomes a video
   * background; a picture becomes a picture background or an image element.
   */
  function mediaItem(a, assets) {
    const moving = moves(a);
    const poster = moving && a.poster ? assets.find((p) => p.ref === a.poster)?.dataUrl : null;
    const preview = poster ?? a.dataUrl ?? null;
    const label = a.kind === 'video' ? t('media.video') : t('media.animatedGif');
    const details = moving ? [label, ...videoFacts(a, (n) => formatBytes(n, locale()))].join(' · ') : null;
    return el('li', { class: 'media-item', dataset: { ref: a.ref } }, [
      el('span', { class: 'thumb', style: preview ? { backgroundImage: `url(${preview})` } : {} }, preview ? [] : [icon(ICONS.film, 22)]),
      el('span', { class: 'media-name' }, [
        el('span', { text: fileNameOf(a.ref) }),
        details ? el('small', { text: details }) : null,
      ]),
      el('div', { class: 'actions' }, [
        el('button', { type: 'button', class: 'text-button', text: t('media.useBackground'), onclick: () => store.dispatch('setTheme', { patch: { background: backgroundOf(a) } }) }),
        a.kind === 'image' && el('button', { type: 'button', class: 'text-button', text: t('media.addImage'), onclick: () => {
          const { x, y } = center();
          store.dispatch('add', { widget: 'image', x, y });
          const id = store.getState().selection[0];
          store.dispatch('update', { id, patch: { kind: { asset: a.ref } } });
        } }),
      ]),
    ]);
  }

  function renderMedia(assets) {
    media = assets;
    const list = $('media-list');
    const items = mediaItems(assets);
    if (!items.length) {
      list.replaceChildren(el('li', { class: 'empty-note', text: t('media.empty') }));
      return;
    }
    list.replaceChildren(...items.map((a) => mediaItem(a, assets)));
  }

  /** While a file is added (a poster may take seconds), the add buttons wait. */
  function setAdding(on) {
    $('media-add').disabled = on;
    $('media-add-video').disabled = on;
    $('media-status').hidden = !on;
    $('panel-media').setAttribute('aria-busy', String(on));
  }

  $('media-add').addEventListener('click', () => actions.addImage());
  $('media-add-video').addEventListener('click', () => actions.addVideo());

  // -------------------------------------------------------------- screen --
  const autostartField = () => checkField(t('screen.autostart'), actions.autostart(), (on) => actions.setAutostart(on));

  /**
   * The panels the vendor app left in desktop mode: listed, labelled "not
   * validated on hardware", and switched back only after a confirmation
   * (D-2026-09-30-release-polish-8).
   */
  function desktopSection(panels) {
    if (!panels.length) return null;
    return el('section', { class: 'desktop-mode', 'aria-labelledby': 'desktop-mode-title' }, [
      el('h3', { id: 'desktop-mode-title', text: t('desktop.title') }),
      ...panels.map((p) => el('div', { class: 'screen-card', role: 'group', 'aria-label': t('desktop.cardLabel', { address: p.key }) }, [
        el('strong', { text: t('desktop.name') }),
        el('span', { class: 'meta', text: `${p.key} · ${p.usb}` }),
        el('span', { class: 'badge not-validated' }, [icon(ICONS.warning, 14), t('desktop.notValidated')]),
        el('p', { class: 'hint', text: t('desktop.hint') }),
        el('p', { class: 'hint', text: t('desktop.models', { models: p.models.map((m) => m.name).join(', ') }) }),
        el('div', { class: 'button-row' }, [
          el('button', { type: 'button', class: 'text-button', text: t('desktop.leave'), onclick: () => actions.leaveDesktopMode(p) }),
        ]),
      ])),
    ]);
  }

  /**
   * What a screen that can be restarted without a replug offers: the
   * Restart button, and why to use it when the screen hung
   * (D-2026-09-30-release-polish-13).
   * @param {{key: string, restartable?: boolean}} s
   * @param {{restarting?: string|null, hung?: string|null}} restart
   */
  function restartParts(s, { restarting = null, hung = null }) {
    if (!s.restartable) return { note: null, button: null };
    const running = restarting === s.key;
    let note = null;
    if (running) note = el('p', { class: 'hint', role: 'status', text: t('restart.running') });
    else if (hung === s.key) note = el('p', { class: 'dialog-warning', role: 'status' }, [icon(ICONS.warning, 18), el('span', { text: t('restart.hung') })]);
    const button = el('button', {
      type: 'button',
      class: hung === s.key && !running ? 'primary-button' : 'text-button',
      text: t('screen.restart'),
      disabled: Boolean(restarting),
      onclick: () => actions.restart(s.key),
    });
    return { note, button };
  }

  /**
   * @param {Record<string, number>} brightness the level set on each screen in this session
   * @param {object[]} desktopMode the panels in desktop mode
   * @param {{restarting?: string|null, hung?: string|null}} restart the screen restarting, and the one that hung
   */
  function renderScreen(screens, current, live, brightness = {}, desktopMode = [], restart = {}) {
    screenArgs = [screens, current, live, brightness, desktopMode, restart];
    const root = $('screen-panel');
    if (!screens.length) {
      root.replaceChildren(el('p', { class: 'empty-note', text: t('screen.none') }), autostartField(), ...[desktopSection(desktopMode)].filter(Boolean));
      return;
    }
    root.replaceChildren(autostartField(), ...[desktopSection(desktopMode)].filter(Boolean), ...screens.map((s) => {
      const model = s.models.length === 1 ? s.models[0] : null;
      const slider = el('input', { type: 'range', min: 0, max: 100, step: 1, value: String(brightness[s.key] ?? 70), 'aria-label': t('screen.brightness') });
      slider.addEventListener('change', () => actions.setBrightness(s.key, Number(slider.value)));
      const { note, button } = restartParts(s, restart);
      return el('section', { class: 'screen-card', 'aria-label': model ? model.name : s.key }, [
        el('strong', { text: model ? model.name : s.models.map((m) => m.name).join(' / ') }),
        el('span', { class: 'meta', text: `${s.key} · ${t(`screen.state.${s.state}`)}${s.key === current && live ? ` · ${t('status.live')}` : ''}` }),
        el('label', { class: 'field' }, [el('span', { text: t('screen.brightness') }), slider]),
        note,
        el('div', { class: 'button-row' }, [
          el('button', { type: 'button', class: 'text-button', text: t('screen.release'), onclick: () => actions.release(s.key) }),
          button,
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
    showImportReport,
    renderMedia,
    setAdding,
    renderScreen,
    selectTab: (name) => selectTab(tabs.find((x) => x.dataset.tab === name)),
    /** The UI's language changed: every panel is drawn again. */
    retranslate() {
      renderWidgets();
      renderSensors();
      renderLayers();
      renderThemes(themes);
      showImportReport(importReport);
      renderMedia(media);
      renderScreen(...screenArgs);
    },
  };
}

// The left library: widgets, sensors, layers, themes, media (the theme's and
// the GIF collection) and screen, each a tab panel. Widgets and sensors drag
// onto the canvas.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { makeDraggable } from './dragdrop.js';
import { checkField } from './fields.js';
import { WIDGETS, widgetOf } from '../editor/widgets.js';
import { backgroundOf, fileNameOf, mediaItems, moves, videoFacts } from '../editor/background.js';
import { warningText } from '../messages.js';
import { formatBytes, wireSubtabs } from './storage.js';
import { SHOW_ALL, axisOf, countText, emptyState, filterThemes, rememberedFilter, scopeIn, screenLabel, thumbnailKey } from '../theme-filter.js';
import { createIconPicker } from './icon-picker.js';
import { layerGroups } from '../editor/layers.js';
import { wireLayerDrag } from './layer-drag.js';

export { axisOf };

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

/**
 * @param {object} deps
 * @param {{scope?: string|null, axis?: string}|null} [deps.themeFilter] the Themes tab's filter as remembered
 */
export function createLibrary({ store, canvas, stage, t, locale = () => 'en', themeFilter = null, actions }) {
  const $ = (id) => document.getElementById(id);
  let catalog = [];
  let readings = {};
  let editing = null;
  const collapsedLayers=new Set();
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
    fetchThumbnails();
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
    const { theme, selection } = store.getState();
    const chosen = selection.length === 1 && theme.elements.find(e => e.id === selection[0]);
    const card = chosen?.card ? chosen : theme.elements.find(e => e.id === chosen?.cardMember?.parent);
    if (card) return { x: card.frame.x + card.frame.width / 2, y: card.frame.y + card.frame.height / 2 };
    const c = theme.canvas;
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
  const layerDrag = wireLayerDrag($('layer-list'), store);
  function layerRow(e, index, total, neighbors = null) {
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
        onclick: (evt) => {
          const previous=store.getState().selection;
          if (e.cardMember?.face != null) store.dispatch('cardFace', { id: e.cardMember.parent, face: e.cardMember.face });
          store.select(evt.ctrlKey || evt.metaKey ? previous.includes(e.id)?previous.filter(id=>id!==e.id):[...previous,e.id] : evt.shiftKey ? [...new Set([...previous,e.id])] : [e.id]);
        },
        ondblclick: () => { editing = e.id; renderLayers(); document.querySelector('.layer-list input')?.focus(); },
      }, [e.name, el('small', { text: t(`widget.${widgetOf(e)}`) })]);
    const btn = (label, paths, onclick, disabled = false, pressed = null) => el('button', { type: 'button', class: 'icon-button', title: label, 'aria-label': label, disabled, 'aria-pressed': pressed === null ? null : String(pressed), onclick }, [icon(paths)]);
    return el('li', { dataset: { elementId: e.id }, class: `layer-row${e.card || e.isGroup ? ' layer-container' : ''}${selected ? ' selected' : ''}${e.visible === false ? ' hidden-el' : ''}` }, [
      ...(e.card || e.isGroup ? [el('button',{type:'button',class:'icon-button layer-fold','aria-label':t(collapsedLayers.has(e.id)?'layers.expand':'layers.collapse'),'aria-expanded':String(!collapsedLayers.has(e.id)),onclick:()=>{if(collapsedLayers.has(e.id))collapsedLayers.delete(e.id);else collapsedLayers.add(e.id);renderLayers();}},[icon(collapsedLayers.has(e.id)?['M9 6l6 6-6 6']:['M6 9l6 6 6-6'],14)])] : []),
      name,
      btn(e.visible === false ? t('layers.show') : t('layers.hide'), e.visible === false ? ICONS.eyeOff : ICONS.eye, () => store.dispatch('update', { id: e.id, patch: { visible: e.visible === false } }), false, e.visible !== false),
      btn(e.locked ? t('layers.unlock') : t('layers.lock'), e.locked ? ICONS.lock : ICONS.unlock, () => store.dispatch('update', { id: e.id, patch: { locked: !e.locked } }), false, Boolean(e.locked)),
      btn(t('layers.up'), ICONS.up, () => store.dispatch('reorder', { id: e.id, index: neighbors ? neighbors.up : index + 1 }), neighbors ? neighbors.up == null : index === total - 1),
      btn(t('layers.down'), ICONS.down, () => store.dispatch('reorder', { id: e.id, index: neighbors ? neighbors.down : index - 1 }), neighbors ? neighbors.down == null : index === 0),
    ]);
  }

  function renderLayers() {
    const els = store.getState().theme.elements;
    const list = $('layer-list');
    if (els.length === 0) {
      list.replaceChildren(el('li', { class: 'empty-note', text: t('layers.empty') }));
      return;
    }
    const indexOf = e => els.findIndex(item => item.id === e.id);
    const renderNodes = nodes => nodes.map((node, i) => {
      const { element: card, groups, children } = node;
      const row = layerRow(card, indexOf(card), els.length, {
        up: i > 0 ? indexOf(nodes[i - 1].element) : null,
        down: i + 1 < nodes.length ? indexOf(nodes[i + 1].element) : null,
      });
      if (card.isGroup) return el('li', { class: 'layer-card', dataset: { groupId: card.id } }, [
        el('ul', { class: 'layer-list' }, [row]),
        el('ul', { class: 'layer-list layer-children',hidden:collapsedLayers.has(card.id) }, renderNodes(children)),
      ]);
      if (!groups.length) return row;
      return el('li', { class: 'layer-card', dataset: { cardId: card.id } }, [
        el('ul', { class: 'layer-list' }, [row]),
        el('ul', { class: 'layer-groups',hidden:collapsedLayers.has(card.id) }, groups.map(({ face, elements, nodes: members }) => {
          const active = face !== null && card.card.activeFace === face;
          const label = face === null ? t('card.base') : card.card.faces[face];
          const heading = face === null
            ? el('span', { class: 'layer-group-title', text: label })
            : el('button', {
              type: 'button', class: 'layer-group-title', text: label,
              'aria-pressed': String(active),
              onclick: () => {
                if (!active) store.dispatch('cardFace', { id: card.id, face });
                else store.select([card.id]);
              },
            });
          return el('li', { class: `layer-group${active ? ' active-face' : ''}`, dataset: { face: face === null ? 'base' : face } }, [
            el('div', { class: 'layer-group-heading',dataset:{dropCard:card.id,dropFace:face===null?'base':face} }, [icon(face===null?ICONS.shape:ICONS.card,14),heading, el('small', { text: String(elements.length) })]),
            el('ul', { class: 'layer-list layer-children' }, members.length?renderNodes(members):[el('li',{class:'layer-empty',dataset:{dropCard:card.id,dropFace:face===null?'base':face},text:t('layers.dropHere')})]),
          ]);
        })),
      ]);
    });
    list.replaceChildren(el('li',{class:'layer-root-drop',dataset:{rootDrop:'true'},text:t('layers.rootDrop')}),...renderNodes(layerGroups(els)));
    layerDrag.refresh();
  }

  // -------------------------------------------------------------- themes --
  // Which themes the gallery lists (remembered), the thumbnails it got (by
  // location and revision; `null`: the theme cannot be drawn) and the mini
  // screens of the cards shown, to fill in as thumbnails arrive.
  let filter = rememberedFilter(themeFilter);
  const thumbnails = new Map();
  const cardScreens = new Map();
  const waiting = [];
  let fetching = false;
  // The screen the gallery filtered for (once listed), to list again when it changes.
  let filteredFor = null;

  /** The screen in use (the one chosen at the top), or `null`. */
  function screenInUse() {
    const [screens, current] = screenArgs;
    return screens.find((s) => s.key === current) ?? null;
  }

  /** What the gallery's filter depends on of `screen`: its key and models. */
  const signature = (screen) => (screen ? `${screen.key} ${screen.models.map((m) => m.id).join(' ')}` : '');

  /** Fills a card's mini screen with its thumbnail, or marks it without one. */
  function paintThumbnail(screen, url) {
    screen.dataset.state = url ? 'ready' : url === null ? 'none' : 'loading';
    screen.style.backgroundImage = url ? `url("${url}")` : '';
    const thumb = screen.parentElement;
    thumb?.querySelector('.no-preview')?.remove();
    if (url === null) thumb?.append(el('span', { class: 'no-preview' }, [icon(ICONS.eyeOff, 14), el('span', { text: t('themes.noPreview') })]));
  }

  /** Asks for the missing thumbnails of the cards shown, one at a time, while the tab is open. */
  async function fetchThumbnails() {
    if (fetching) return;
    fetching = true;
    while (waiting.length && !$('panel-themes').hidden) {
      const entry = waiting.shift();
      const key = thumbnailKey(entry);
      if (thumbnails.has(key)) continue;
      const url = await Promise.resolve(actions.themeThumbnail?.(entry.location)).catch(() => null);
      thumbnails.set(key, url ?? null);
      const screen = cardScreens.get(key);
      if (screen) paintThumbnail(screen, url ?? null);
    }
    fetching = false;
  }

  function themeCard(th) {
    const axis = axisOf(th);
    const mini = thumbScreen(th.canvas);
    const key = thumbnailKey(th);
    const screen = el('span', { class: 'thumb-screen', style: { width: `${mini.width}%`, height: `${mini.height}%` } });
    cardScreens.set(key, screen);
    const thumb = el('span', { class: 'thumb', 'aria-hidden': 'true' }, [screen]);
    paintThumbnail(screen, thumbnails.has(key) ? thumbnails.get(key) : th.thumbnail);
    return el('li', {}, [
      el('button', { type: 'button', class: 'theme-card', dataset: { location: th.location }, onclick: () => actions.openTheme(th.location) }, [
        thumb,
        el('strong', { text: th.name }),
        el('span', { class: 'card-meta' }, [
          el('span', { class: `badge ${axis}` }, [icon(ICONS[axis], 14), t(`axis.${axis}`)]),
          el('small', { text: `${screenLabel(th, locale())}${th.bundled ? ` · ${t('themes.bundled')}` : ''}` }),
        ]),
      ]),
    ]);
  }

  /** What the gallery shows when the filter keeps no theme: why, new themes, and "Show all". */
  function emptyItem({ key, showAll }) {
    const newButton = (axis) => el('button', { type: 'button', class: 'text-button', onclick: () => actions.newTheme(axis) }, [
      icon(ICONS[axis], 14),
      el('span', { text: t(axis === 'vertical' ? 'themes.newVertical' : 'themes.newHorizontal') }),
    ]);
    return el('li', { class: 'empty-state' }, [
      el('p', { class: 'empty-note', text: t(key) }),
      el('div', { class: 'button-row' }, [
        newButton('vertical'),
        newButton('horizontal'),
        showAll && el('button', { type: 'button', class: 'text-button show-all', text: t('themes.showAll'), onclick: () => {
          chooseFilter(SHOW_ALL);
          document.querySelector('#theme-scope [data-scope="all"]')?.focus();
        } }),
      ]),
    ]);
  }

  /** The filter buttons say what is chosen; "For this screen" needs a screen. */
  function syncFilterButtons(screen) {
    const scope = scopeIn(filter, screen);
    for (const button of document.querySelectorAll('#theme-scope [data-scope]')) {
      button.setAttribute('aria-pressed', String(button.dataset.scope === scope));
      if (button.dataset.scope !== 'screen') continue;
      button.disabled = !screen;
      const model = screen?.models.length === 1 ? screen.models[0].name : screen?.key;
      button.title = screen ? t('themes.forScreenHint', { name: model }) : t('themes.forScreenNone');
    }
    for (const button of document.querySelectorAll('#theme-axis [data-axis]')) {
      button.setAttribute('aria-pressed', String(button.dataset.axis === filter.axis));
    }
  }

  function renderThemes(list) {
    themes = list;
    const screen = screenInUse();
    filteredFor = signature(screen);
    syncFilterButtons(screen);
    const shown = filterThemes(list, filter, screen);
    const count = countText(shown.length, list.length);
    $('theme-count').textContent = count ? t(count.key, count.params) : '';
    const grid = $('theme-grid');
    cardScreens.clear();
    if (!shown.length) {
      grid.replaceChildren(emptyItem(emptyState(list, filter, screen)));
      return;
    }
    grid.replaceChildren(...shown.map(themeCard));
    waiting.splice(0, waiting.length, ...shown.filter((entry) => !thumbnails.has(thumbnailKey(entry))));
    fetchThumbnails();
  }

  /** Changes the filter (what is not given stays), remembers it and lists again. */
  function chooseFilter(next) {
    filter = { ...filter, ...next };
    actions.rememberThemeFilter?.(filter);
    renderThemes(themes);
  }

  /** Lists again when the screen in use changed (another model, or none). */
  function screenChanged() {
    if (filteredFor !== null && signature(screenInUse()) !== filteredFor) renderThemes(themes);
  }

  for (const button of document.querySelectorAll('#theme-scope [data-scope]')) {
    button.addEventListener('click', () => chooseFilter({ scope: button.dataset.scope }));
  }
  for (const button of document.querySelectorAll('#theme-axis [data-axis]')) {
    button.addEventListener('click', () => chooseFilter({ axis: button.dataset.axis }));
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
  $('theme-save-as').addEventListener('click', () => actions.saveThemeAs());

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
        moving && el('button', { type: 'button', class: 'text-button', text: t('videoPreview.open'), onclick: () => actions.previewVideo(a) }),
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

  // "This theme" | "Collection" (D-2026-10-01-gif-sticker-search-5): the
  // collection is where GIFs and stickers are searched on KLIPY.
  const iconPicker = createIconPicker({ store, canvas, stage, t, add: actions.addIcon });
  wireSubtabs($('panel-media').querySelector('.subtabs'), (name) => {
    if (name === 'icons') void iconPicker.show();
    actions.mediaSubtab?.(name);
  });
  $('gif-search-open').addEventListener('click', () => actions.searchGifs());

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
    screenChanged();
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
      iconPicker.retranslate();
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

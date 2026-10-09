import { isShown, canGroup } from '../editor/cards.js';
import { CLOCK_PATTERNS, formatClock } from '../clock-format.js';
// The right inspector: the theme when nothing is selected, one element's
// properties, or align/distribute tools for several. Every edit is one
// store command (one undo step).
import { readIcon } from '../icon-catalog.js';
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { checkField, colorField as colorInputs, numberField, rangeField, segmented, selectField, textField } from './fields.js';
import { createWidget, widgetOf } from '../editor/widgets.js';
import { ORIENTATIONS } from '../editor/geometry.js';
import { fileNameOf, videoFacts, videoStatus } from '../editor/background.js';
import { ROTATIONS, ZOOM_MAX, ZOOM_MIN, ZOOM_STEP, framingOf, framingPercents, reframed, resolvedRotation, withoutFraming } from '../editor/video-framing.js';
import { formatBytes } from './storage.js';
import { createTabMemory, previewCrop, tabAfterKey, tabsOf } from '../inspector-tabs.js';

const BOUND = ['value', 'bar', 'ring', 'needle', 'graph'];
/** "Frame on canvas": a frame's corners around a move cross. */
const FRAME_ICON = ['M4 9V4h5', 'M15 4h5v5', 'M20 15v5h-5', 'M9 20H4v-5', 'M12 8v8', 'M8 12h8'];
/** A group: two boxes in a dashed frame. */
const GROUP_ICON = ['M4 4h4', 'M16 4h4v4', 'M20 16v4h-4', 'M8 20H4v-4', 'M8 8h5v5H8z', 'M11 11h5v5h-5z'];
const CHEVRON_LEFT = ['M15 18l-6-6 6-6'];
const CHEVRON_RIGHT = ['M9 18l6-6-6-6'];
const PLUS = ['M12 5v14', 'M5 12h14'];
/** The tallest the live preview is drawn, CSS pixels. */
const PREVIEW_HEIGHT = 120;

/** The video-background actions and what they depend on, when none are given. */
const NO_VIDEO_ACTIONS = Object.freeze({
  context: () => ({ screen: null, live: false, liveVideo: null, tools: null, locale: 'en', auto: null, framing: false, motion: true }),
  useVideo: () => {},
  useImage: () => {},
  openStorage: () => {},
  setFraming: () => {},
  openGuide: () => {},
});

/**
 * @param {object} deps
 * @param {HTMLElement} deps.root
 * @param {() => number} deps.minRefresh the fastest refresh a theme may ask for, seconds (from the backend)
 * @param {{context: () => {screen: object|null, live: boolean, liveVideo: object|null, tools: object|null, locale: string, auto: {rotation: number, size: object|null}|null, framing: boolean, motion: boolean}, useVideo: () => void, useImage: () => void, openStorage: () => void, setFraming: (on: boolean) => void, openGuide: () => void}} [deps.video]
 *   the screen and ffmpeg the video background is told against, what Auto
 *   turns it (`video_auto`), whether the canvas frames it now and whether
 *   the preview may move, and its actions: pick a video (or GIF) for the
 *   background, pick a picture, show the storage tab (where the missing
 *   video is sent), frame it on the canvas, and open the ffmpeg guide
 * @param {() => HTMLCanvasElement|null} [deps.frame] the canvas the rendered
 *   frame is drawn on; the live preview copies the selected object from it
 */
/** Buttons of one choice (`aria-pressed`), each with a stable id so focus stays on it. */
function choice(id, label, value, options, onChange) {
  const group = el('div', { class: `segmented framing-choice ${id}`, role: 'group', 'aria-label': label });
  for (const [v, text] of options) {
    group.append(el('button', { type: 'button', id: `${id}-${v}`, text, 'aria-pressed': String(v === value), onclick: () => v !== value && onChange(v) }));
  }
  return el('div', { class: 'field' }, [el('span', { text: label }), group]);
}

export function createInspector({ root, store, t, sensors, minRefresh, editIcon = () => {}, searchCities = async () => [], video = NO_VIDEO_ACTIONS, frame: frameCanvas = () => document.getElementById('preview') }) {
  const update = (id, patch) => store.dispatch('update', { id, patch });
  // A framing slider being dragged: its moves are one gesture (one undo
  // step), and the form is not drawn again under the pointer meanwhile.
  let sliding = false;
  const colorField = (label, hex, onChange) => colorInputs(label, hex, onChange, { alphaLabel: t('inspector.opacityOf', { name: label }) });

  function sensorOptions() {
    const groups = {};
    for (const s of sensors.catalog()) {
      const group = t(`category.${s.category}`);
      (groups[group] ??= []).push([s.key, s.label]);
    }
    return groups;
  }

  function sensorPicker(id, key, patchFor) {
    const options = sensorOptions();
    const known = Object.values(options).flat().some(([k]) => k === key);
    if (!known) options[t('inspector.otherSensor')] = [[key, key]];
    return selectField(t('inspector.sensor'), key, options, (k) => update(id, patchFor(k)));
  }

  function rangeFields(id, binding) {
    return el('div', { class: 'field-row' }, [
      numberField(t('inspector.min'), binding.min, (v) => update(id, { kind: { binding: { min: v } } }), { step: 'any' }),
      numberField(t('inspector.max'), binding.max, (v) => update(id, { kind: { binding: { max: v } } }), { step: 'any' }),
    ]);
  }

  // What the form was last drawn with: a video background is drawn again
  // when the screen, live mode, its video or ffmpeg change.
  let shown = { assets: [], signature: '' };

  // --------------------------------------------------------------- theme --
  const button = (text, onclick) => el('button', { type: 'button', class: 'text-button', text, onclick });
  const useColor = () => button(t('inspector.useColor'), () => store.dispatch('setTheme', { patch: { background: { type: 'color', color: '#0c0e16ff' } } }));

  // ------------------------------------------------------------ framing --
  /** Applies a framing patch to the video background: one store command, none when it changes nothing. */
  function frame(patch) {
    const framed = reframed(store.getState().theme.background, patch);
    if (framed) store.dispatch('setTheme', { patch: { background: framed } });
  }

  function endSlide() {
    if (!sliding) return;
    sliding = false;
    store.endGesture();
  }

  /**
   * A framing slider with its readout: dragging it frames the preview live
   * and is one undo step; each arrow key on it is one too.
   */
  function framingRange({ id, label, value, min, max, step, format, onInput }) {
    const out = el('output', { for: id, text: format(value) });
    const input = el('input', { id, type: 'range', min, max, step, value: String(value) });
    input.addEventListener('input', () => {
      const v = Number(input.value);
      out.textContent = format(v);
      if (!sliding) {
        sliding = true;
        store.beginGesture();
      }
      onInput(v);
    });
    for (const done of ['change', 'pointerup', 'keyup', 'blur']) input.addEventListener(done, endSlide);
    return el('label', { class: 'field', for: id }, [el('span', {}, [label, ' ', out]), input]);
  }

  /** The pad color: a picker and its hex, opaque. */
  function padField(color) {
    const rgb = color.slice(0, 7);
    const picker = el('input', { type: 'color', id: 'framing-pad', value: rgb, 'aria-label': t('framing.padColor') });
    const text = el('input', { type: 'text', id: 'framing-pad-hex', value: rgb, 'aria-label': t('framing.padHex'), spellcheck: false });
    picker.addEventListener('input', () => { text.value = picker.value; });
    picker.addEventListener('change', () => frame({ padColor: picker.value }));
    text.addEventListener('change', () => {
      if (/^#[0-9a-f]{6}$/i.test(text.value)) frame({ padColor: text.value });
    });
    return el('div', { class: 'field' }, [el('span', { text: t('framing.padColor') }), el('div', { class: 'color-field opaque' }, [picker, text])]);
  }

  /** The zoom: a slider and a number, 100–400 %, step 5 %. */
  function zoomField(zoom) {
    const percent = Math.round(zoom * 100);
    const number = el('input', { type: 'number', id: 'framing-zoom-number', min: ZOOM_MIN * 100, max: ZOOM_MAX * 100, step: ZOOM_STEP * 100, value: String(percent), 'aria-label': t('framing.zoomNumber') });
    number.addEventListener('change', () => {
      const v = Number(number.value);
      if (Number.isFinite(v)) frame({ zoom: v / 100 });
    });
    const range = framingRange({
      id: 'framing-zoom', label: t('framing.zoom'), value: percent, min: ZOOM_MIN * 100, max: ZOOM_MAX * 100, step: ZOOM_STEP * 100,
      format: (v) => t('framing.percent', { value: v }),
      onInput: (v) => {
        number.value = String(v);
        frame({ zoom: v / 100 });
      },
    });
    return el('div', { class: 'framing-zoom' }, [range, number]);
  }

  /**
   * The Framing group of a video background (D-2026-10-01-video-background-framing-6):
   * "Frame on canvas", Rotation (Auto names the angle it detected), Fill or
   * Fit, Zoom, Position X and Y with Center, the pad color (Fit only) and
   * Reset; why the preview shows the poster, when it does.
   */
  function framingGroup(bg, context) {
    const f = framingOf(bg);
    const percents = framingPercents(f);
    const auto = context.auto;
    const autoLabel = auto ? t('framing.auto', { degrees: auto.rotation }) : t('framing.autoUnknown');
    const rotation = resolvedRotation(f, auto);
    const nodes = [
      el('button', {
        type: 'button', id: 'framing-canvas', class: 'text-button framing-toggle', 'aria-pressed': String(Boolean(context.framing)), onclick: () => video.setFraming(!context.framing),
      }, [icon(FRAME_ICON, 16), t('framing.onCanvas')]),
      el('p', { class: 'hint', text: t('framing.onCanvasHint') }),
      choice('framing-rotation', t('framing.rotation'), f.rotation === null ? 'auto' : String(f.rotation), [
        ['auto', autoLabel],
        ...ROTATIONS.map((r) => [String(r), t('framing.degrees', { degrees: r })]),
      ], (v) => frame({ rotation: v === 'auto' ? null : Number(v) })),
      f.rotation === null && rotation !== 0 ? el('p', { class: 'hint', text: t('framing.autoTurned', { degrees: rotation }) }) : null,
      choice('framing-fit', t('framing.fit'), f.fit, [['cover', t('framing.cover')], ['contain', t('framing.contain')]], (v) => frame({ fit: v })),
      el('p', { class: 'hint', text: t('framing.fitHint') }),
      zoomField(f.zoom),
      framingRange({ id: 'framing-x', label: t('framing.positionX'), value: percents.x, min: 0, max: 100, step: 1, format: (v) => t('framing.percent', { value: v }), onInput: (v) => frame({ position: { x: v / 100 } }) }),
      framingRange({ id: 'framing-y', label: t('framing.positionY'), value: percents.y, min: 0, max: 100, step: 1, format: (v) => t('framing.percent', { value: v }), onInput: (v) => frame({ position: { y: v / 100 } }) }),
      f.fit === 'contain' ? padField(f.padColor) : null,
      el('div', { class: 'button-row' }, [
        el('button', { type: 'button', id: 'framing-center', class: 'text-button', text: t('framing.center'), disabled: f.position.x === 0.5 && f.position.y === 0.5, onclick: () => frame({ position: { x: 0.5, y: 0.5 } }) }),
        el('button', { type: 'button', id: 'framing-reset', class: 'text-button', text: t('framing.reset'), disabled: !bg.framing, onclick: () => store.dispatch('setTheme', { patch: { background: withoutFraming(bg) } }) }),
      ]),
    ];
    if (context.tools && !context.tools.ready) {
      nodes.push(
        el('p', { class: 'hint framing-note', text: t('framing.noFfmpeg') }),
        el('div', { class: 'button-row' }, [el('button', { type: 'button', id: 'framing-guide', class: 'text-button', text: t('framing.guide'), onclick: () => video.openGuide() })]),
      );
    } else if (context.motion === false) {
      nodes.push(el('p', { class: 'hint framing-note', text: t('framing.reducedMotion') }));
    }
    return el('div', { class: 'framing', role: 'group', 'aria-labelledby': 'framing-title' }, [
      el('h3', { id: 'framing-title', text: t('framing.title') }),
      ...nodes.filter(Boolean),
    ]);
  }

  /**
   * A video background (a video or an animated GIF): its poster and name,
   * its framing, what the connected screen does with it (the storage tab
   * sends a missing one), what ffmpeg is for when there is no poster, and
   * the other backgrounds.
   */
  function videoBackground(bg, assets) {
    const context = video.context();
    const asset = assets.find((a) => a.ref === bg.asset) ?? { ref: bg.asset };
    const posterUrl = bg.poster ? assets.find((a) => a.ref === bg.poster)?.dataUrl : null;
    const facts = videoFacts(asset, (n) => formatBytes(n, context.locale));
    const status = videoStatus(context);
    const nodes = [
      el('div', { class: 'bg-video', role: 'group', 'aria-label': t('bg.video') }, [
        el('span', { class: 'thumb', style: posterUrl ? { backgroundImage: `url(${posterUrl})` } : {} }, posterUrl ? [] : [icon(ICONS.film, 22)]),
        el('span', { class: 'media-name' }, [
          el('strong', { text: fileNameOf(bg.asset) }),
          facts.length ? el('small', { text: facts.join(' · ') }) : null,
        ]),
      ]),
      framingGroup(bg, context),
      el('p', { class: 'hint video-status', dataset: { status }, text: t(`inspector.video.${status}`) }),
    ];
    if (status === 'missing') nodes.push(el('div', { class: 'button-row' }, [button(t('inspector.video.openStorage'), () => video.openStorage())]));
    if (!bg.poster && context.tools && !context.tools.ready) {
      nodes.push(el('p', { class: 'hint', text: t('inspector.video.noPoster') }));
      nodes.push(el('ul', { class: 'hints' }, (context.tools.installHints ?? []).map((h) => el('li', {}, [el('code', { text: h })]))));
    } else if (!bg.poster) {
      nodes.push(el('p', { class: 'hint', text: t('inspector.video.noPosterReady') }));
    }
    nodes.push(el('div', { class: 'button-row' }, [
      button(t('inspector.replaceVideo'), () => video.useVideo()),
      button(t('deviceVideo.choose'), () => video.openStorage()),
      button(t('inspector.useImage'), () => video.useImage()),
      useColor(),
    ]));
    return nodes;
  }

  function backgroundFields(bg, assets) {
    if (bg.type === 'deviceVideo') {
      const context = video.context();
      const windows = store.getState().theme.elements.some((e) => isShown(store.getState().theme, e) && e.kind.type === 'shape' && e.kind.videoWindow);
      const status = context.live ? context.liveVideo?.state : null;
      return [
        el('strong', { text: fileNameOf(bg.path) }),
        el('p', { class: 'hint', text: t('deviceVideo.fromScreen') }),
        checkField(t('deviceVideo.loop'), bg.looping !== false, (looping) => store.dispatch('setTheme', { patch: { background: { ...bg, looping } } })),
        windows && colorField(t('deviceVideo.outsideColor'), bg.color ?? '#000000ff', (color) => store.dispatch('setTheme', { patch: { background: { ...bg, color } } })),
        el('p', { class: 'hint', text: t('deviceVideo.preview') }),
        status === 'storedMissing' && el('p', { class: 'hint', text: t('deviceVideo.missing') }),
        status === 'noPlayback' && el('p', { class: 'hint', text: t('deviceVideo.unsupported') }),
        el('div', { class: 'button-row' }, [button(t('deviceVideo.choose'), () => video.openStorage()), useColor()]),
      ].filter(Boolean);
    }
    if (bg.type === 'video') return videoBackground(bg, assets);
    return [
      bg.type === 'color'
        ? colorField(t('inspector.color'), bg.color, (c) => store.dispatch('setTheme', { patch: { background: { type: 'color', color: c } } }))
        : el('p', { class: 'hint', text: t(`bg.${bg.type}`) }),
      el('div', { class: 'button-row' }, [
        bg.type !== 'color' && useColor(),
        button(t('inspector.useVideo'), () => video.useVideo()),
        button(t('deviceVideo.choose'), () => video.openStorage()),
      ]),
    ];
  }

  function themeForm(theme, assets) {
    return [
      el('h2', { text: t('inspector.theme') }),
      textField(t('inspector.name'), theme.name, (v) => store.dispatch('setTheme', { patch: { name: v } })),
      el('p', { class: 'hint', text: t('inspector.canvas', { width: theme.canvas.width, height: theme.canvas.height }) }),
      selectField(t('inspector.orientation'), theme.orientation, ORIENTATIONS.map((o) => [o, t(`orientation.${o}`)]), (o) => store.dispatch('setOrientation', { orientation: o })),
      el('div', { class: 'background-heading' }, [
        el('h3', { text: t('inspector.background') }),
        theme.background.type !== 'color' && button(t('inspector.removeBackground'), () => store.dispatch('setTheme', {
          patch: { background: { type: 'color', color: theme.background.color ?? '#0c0e16ff' } },
        })),
      ]),
      ...backgroundFields(theme.background, assets),
      el('h3', { text: t('inspector.refresh') }),
      rangeField(t('inspector.refreshSeconds'), theme.refreshSeconds, (v) => store.dispatch('setTheme', { patch: { refreshSeconds: v } }), { min: minRefresh(), max: 5, step: minRefresh(), format: (v) => t('unit.seconds', { value: v }) }),
    ];
  }

  // ---------------------------------------------------------- per kind ----
  function textForm(e) {
    const k = e.kind;
    const s = k.style;
    const c = k.content;
    const nodes = [], weatherAppearance = [];
    if (c.type === 'static') nodes.push(textField(t('inspector.text'), c.text, (v) => update(e.id, { kind: { content: { text: v } } })));
    if (c.type === 'clock') {
      nodes.push(selectField(t('clock.language'), c.language ?? '', [['', t('clock.system')], ['it', t('language.it')], ['en', t('language.en')], ['pt-BR', t('language.pt-BR')]], (language) => update(e.id, { kind: { content: { language: language || null } } })));
      nodes.push(selectField(t('clock.format'), CLOCK_PATTERNS.includes(c.pattern) ? c.pattern : 'custom', [...CLOCK_PATTERNS.map((pattern) => [pattern, formatClock(pattern, new Date(), c.language, c.casing)]), ['custom', t('clock.custom')]], (pattern) => {
        if (pattern === 'custom') root.querySelector('[list="clock-patterns"]')?.focus();
        else update(e.id, { kind: { content: { pattern } } });
      }));
      nodes.push(selectField(t('clock.casing'), c.casing ?? 'normal', ['normal', 'upper', 'title'].map((value) => [value, t(`clock.${value}`)]), (casing) => update(e.id, { kind: { content: { casing } } })));
      nodes.push(textField(t('inspector.pattern'), c.pattern, (v) => update(e.id, { kind: { content: { pattern: v } } }), { list: 'clock-patterns' }));
      nodes.push(el('datalist', { id: 'clock-patterns' }, CLOCK_PATTERNS.map((p) => el('option', { value: p }))));
      nodes.push(el('p', { id: 'clock-preview', class: 'hint', text: t('clock.preview', { value: formatClock(c.pattern, new Date(), c.language, c.casing) }) }));
      nodes.push(el('p', { class: 'hint', text: t('inspector.patternHelp') }));
    }
    if (c.type === 'player') {
      const set = patch => update(e.id, {kind:{content:patch}});
      nodes.push(textField(t('player.source'), c.source ?? '', source=>set({source:source.slice(0,160)})),
        textField(t('player.empty'), c.emptyText ?? '', emptyText=>set({emptyText:emptyText.slice(0,512)})),
        checkField(t('player.hide'),c.hideWhenStopped,hideWhenStopped=>set({hideWhenStopped})),
        el('p',{class:'hint',text:t('player.help')}));
      for (const field of ['showCover','showProgress','showSource']) weatherAppearance.push(checkField(t(`player.${field}`),c[field],value=>set({[field]:value})));
      if(c.showCover) weatherAppearance.push(
        checkField(t('inspector.individualCorners'),Boolean(c.coverCorners),enabled=>set({coverCorners:enabled?Array(4).fill(c.coverRadius ?? 0):null,coverRadius:c.coverCorners?.[0] ?? c.coverRadius ?? 0})),
        ...(c.coverCorners ? ['topLeft','topRight','bottomRight','bottomLeft'].map((name,i)=>numberField(t(`inspector.${name}`),c.coverCorners[i],v=>set({coverCorners:c.coverCorners.map((r,n)=>n===i?Math.max(0,v):r)}),{min:0})) : [numberField(t('player.coverRadius'),c.coverRadius ?? 0,v=>set({coverRadius:Math.max(0,v)}),{min:0})]),
        numberField(t('player.coverGap'),c.coverGap ?? e.kind.style.size*.3,v=>set({coverGap:Math.max(0,v)}),{min:0}));
    }
    if (c.type === 'weather') {
      let query = c.city;
      const results = el('div', { id: 'weather-results', role: 'status', 'aria-live': 'polite' });
      const search = el('button', { type: 'button', class: 'text-button', text: t('weather.search'), onclick: async () => {
        search.disabled = true;
        results.textContent = t('weather.loading');
        try {
          const cities = await searchCities(query);
          if (!root.contains(results)) return;
          results.replaceChildren();
          if (!cities.length) results.textContent = t('weather.empty');
          for (const city of cities) results.append(el('button', { type: 'button', class: 'text-button', text: [city.name, city.region, city.country].filter(Boolean).join(', '), onclick: () => update(e.id, { kind: { content: { city: city.name, latitude: city.latitude, longitude: city.longitude } } }) }));
        } catch { if (root.contains(results)) results.textContent = t('weather.error'); }
        finally { search.disabled = false; }
      } });
      nodes.push(el('p', { class: 'hint', text: t('weather.location', { city: c.city }) }));
      nodes.push(textField(t('weather.city'), query, (value) => { query = value; }));
      nodes.push(search, results);
      nodes.push(selectField(t('weather.language'), c.language ?? '', [['', t('clock.system')], ['it', t('language.it')], ['en', t('language.en')], ['pt-BR', t('language.pt-BR')]], (language) => update(e.id, { kind: { content: { language: language || null } } })));
      nodes.push(checkField(t('inspector.fahrenheit'), c.fahrenheit, (fahrenheit) => update(e.id, { kind: { content: { fahrenheit } } })));
      weatherAppearance.push(checkField(t('weather.icon'), c.showIcon !== false, (showIcon) => update(e.id, { kind: { content: { showIcon } } })));
      if (c.showIcon !== false) {
        weatherAppearance.push(selectField(t('weather.iconStyle'), c.iconStyle ?? 'outline', [['outline', t('icons.outline')], ['filled', t('icons.filled')], ['colored',t('weather.colored')], ['dimensional',t('weather.dimensional')]], (iconStyle) => update(e.id, { kind: { content: { iconStyle } } })));
        weatherAppearance.push(numberField(t('weather.iconGap'), c.iconGap ?? s.size * 0.3, (iconGap) => update(e.id, { kind: { content: { iconGap: Math.max(0, Math.min(256, iconGap)) } } }), { min: 0, max: 256, step: 'any' }));
        weatherAppearance.push(numberField(t('weather.iconSize'), c.iconSize ?? 0, (size) => update(e.id, { kind: { content: { iconSize: size <= 0 ? null : Math.min(512, size) } } }), { min: 0, max: 512 }));
      }
      nodes.push(el('p', { class: 'hint', text: t('weather.source') }));
    }
    if (c.type === 'sensor') {
      nodes.push(sensorPicker(e.id, c.key, (key) => ({ kind: { content: { key } } })));
      nodes.push(el('div', { class: 'field-row' }, [
        textField(t('inspector.prefix'), c.prefix, (v) => update(e.id, { kind: { content: { prefix: v } } })),
        textField(t('inspector.suffix'), c.suffix, (v) => update(e.id, { kind: { content: { suffix: v } } })),
      ]));
      nodes.push(selectField(t('inspector.decimals'), c.format.decimals === undefined || c.format.decimals === null ? 'auto' : String(c.format.decimals),
        [['auto', t('inspector.decimalsAuto')], ['0', '0'], ['1', '1'], ['2', '2']],
        (v) => update(e.id, { kind: { content: { format: { decimals: v === 'auto' ? null : Number(v) } } } })));
      nodes.push(checkField(t('inspector.showUnit'), c.format.showUnit !== false, (v) => update(e.id, { kind: { content: { format: { showUnit: v } } } })));
      nodes.push(checkField(t('inspector.fahrenheit'), c.format.fahrenheit, (v) => update(e.id, { kind: { content: { format: { fahrenheit: v } } } })));
    }
    const content = nodes.splice(0);
    nodes.push(...weatherAppearance);
    nodes.push(el('h4', { text: t('inspector.font') }));
    nodes.push(textField(t('inspector.fontFamily'), s.font.family, (v) => update(e.id, { kind: { style: { font: { family: v } } } }), { list: 'font-families' }));
    nodes.push(el('datalist', { id: 'font-families' }, sensors.fonts().map((f) => el('option', { value: f }))));
    nodes.push(el('div', { class: 'field-row' }, [
      numberField(t('inspector.size'), s.size, (v) => update(e.id, { kind: { style: { size: Math.max(4, v) } } }), { min: 4, max: 800 }),
      selectField(t('inspector.weight'), String(s.font.weight), [300, 400, 500, 600, 700, 800, 900].map((w) => [String(w), t(`weight.${w}`)]), (v) => update(e.id, { kind: { style: { font: { weight: Number(v) } } } })),
    ]));
    nodes.push(colorField(t('inspector.color'), typeof s.paint === 'string' ? s.paint : '#ffffffff', (v) => update(e.id, { kind: { style: { paint: v } } })));
    nodes.push(segmented(t('inspector.align'), s.align, [['left', t('inspector.alignLeft')], ['center', t('inspector.alignCenter')], ['right', t('inspector.alignRight')]], (v) => update(e.id, { kind: { style: { align: v } } })));
    nodes.push(segmented(t('inspector.valign'), s.valign, [['top', t('inspector.valignTop')], ['middle', t('inspector.valignMiddle')], ['bottom', t('inspector.valignBottom')]], (v) => update(e.id, { kind: { style: { valign: v } } })));
    nodes.push(numberField(t('inspector.letterSpacing'), s.letterSpacing ?? 0, (v) => update(e.id, { kind: { style: { letterSpacing: v } } }), { step: 'any' }));
    return { content, appearance: nodes };
  }

  const trackField = (e) => el('div', {}, [
    checkField(t('inspector.showTrack'), Boolean(e.kind.track), (on) => update(e.id, { kind: { track: on ? '#ffffff26' : null } })),
    e.kind.track && colorField(t('inspector.track'), typeof e.kind.track === 'string' ? e.kind.track : '#ffffff26', (v) => update(e.id, { kind: { track: v } })),
  ]);

  function barForm(e) {
    const k = e.kind;
    const content = [
      sensorPicker(e.id, k.binding.key, (key) => ({ kind: { binding: { key } } })),
      rangeFields(e.id, k.binding),
    ];
    const appearance = [
      selectField(t('inspector.direction'), k.direction, ['leftToRight', 'rightToLeft', 'bottomToTop', 'topToBottom'].map((d) => [d, t(`dir.${d}`)]), (v) => update(e.id, { kind: { direction: v } })),
      colorField(t('inspector.fill'), typeof k.fill === 'string' ? k.fill : '#38bdf8ff', (v) => update(e.id, { kind: { fill: v } })),
      trackField(e),
      numberField(t('inspector.radius'), k.radius ?? 0, (v) => update(e.id, { kind: { radius: Math.max(0, v) } }), { min: 0 }),
      segmentsFields(e),
    ];
    return { content, appearance };
  }

  function segmentsFields(e) {
    const seg = e.kind.segments;
    return el('div', {}, [
      checkField(t('inspector.segments'), Boolean(seg), (on) => update(e.id, { kind: { segments: on ? { count: 10, gap: e.kind.type === 'ring' ? 4 : 2 } : null } })),
      seg && el('div', { class: 'field-row' }, [
        numberField(t('inspector.count'), seg.count, (v) => update(e.id, { kind: { segments: { count: Math.max(1, v) } } }), { min: 1, max: 200 }),
        numberField(t('inspector.gap'), seg.gap, (v) => update(e.id, { kind: { segments: { gap: Math.max(0, v) } } }), { min: 0, step: 'any' }),
      ]),
    ]);
  }

  function ringFillFields(e) {
    const p = e.kind.fill;
    const arc = p?.scale === 'arc';
    const linear = typeof p === 'object' && Boolean(p?.stops);
    const start = arc ? p.start : typeof p === 'string' ? p : p?.stops?.[0]?.[1] ?? '#38bdf8ff';
    const end = arc ? p.end : p?.stops?.at(-1)?.[1] ?? '#f97316ff';
    const modes = [['solid', t('ring.solid')], ['arc', t('ring.arc')]];
    if (linear) modes.push(['linear', t('ring.importedLinear')]);
    const setArc = (patch) => update(e.id, { kind: { fill: { scale: 'arc', start, end, transition: p?.transition ?? 50, ...patch } } });
    return el('div', {}, [
      selectField(t('ring.fillMode'), arc ? 'arc' : linear ? 'linear' : 'solid', modes, (v) => {
        if (v === 'arc') setArc({});
        else if (v === 'solid') update(e.id, { kind: { fill: start } });
      }),
      arc ? el('div', {}, [
        colorField(t('ring.startColor'), start, (v) => setArc({ start: v })),
        colorField(t('ring.endColor'), end, (v) => setArc({ end: v })),
        rangeField(t('ring.transition'), p.transition ?? 50, (v) => setArc({ transition: v }), { min: 0, max: 100, step: 1, format: (v) => `${v}%` }),
        el('p', { class: 'hint', text: t('ring.transitionHint') }),
      ]) : !linear && colorField(t('inspector.fill'), start, (v) => update(e.id, { kind: { fill: v } })),
    ]);
  }

  function ringForm(e) {
    const k = e.kind;
    const content = [
      sensorPicker(e.id, k.binding.key, (key) => ({ kind: { binding: { key } } })),
      rangeFields(e.id, k.binding),
    ];
    const appearance = [
      checkField(t('ring.testFull'), Boolean(k.testFull), (v) => update(e.id, { kind: { testFull: v } })),
      el('div', { class: 'field-row' }, [
        numberField(t('inspector.startAngle'), k.startAngle, (v) => update(e.id, { kind: { startAngle: v } }), { min: -360, max: 360 }),
        numberField(t('inspector.sweep'), k.sweep, (v) => update(e.id, { kind: { sweep: v } }), { min: 1, max: 360 }),
      ]),
      numberField(t('inspector.thickness'), k.thickness, (v) => update(e.id, { kind: { thickness: Math.max(1, v) } }), { min: 1 }),
      checkField(t('inspector.clockwise'), k.clockwise !== false, (v) => update(e.id, { kind: { clockwise: v } })),
      checkField(t('inspector.roundCaps'), Boolean(k.roundCaps), (v) => update(e.id, { kind: { roundCaps: v } })),
      ringFillFields(e),
      trackField(e),
      segmentsFields(e),
    ];
    return { content, appearance };
  }

  function needleForm(e) {
    const k = e.kind;
    const content = [
      sensorPicker(e.id, k.binding.key, (key) => ({ kind: { binding: { key } } })),
      rangeFields(e.id, k.binding),
    ];
    const appearance = [
      el('div', { class: 'field-row' }, [
        numberField(t('inspector.startAngle'), k.startAngle, (v) => update(e.id, { kind: { startAngle: v } }), { min: -360, max: 360 }),
        numberField(t('inspector.sweep'), k.sweep, (v) => update(e.id, { kind: { sweep: v } }), { min: 1, max: 360 }),
      ]),
      el('div', { class: 'field-row' }, [
        numberField(t('inspector.pivotX'), Math.round((k.pivot?.[0] ?? 0.5) * 100), (v) => update(e.id, { kind: { pivot: [v / 100, k.pivot?.[1] ?? 0.5] } }), { min: 0, max: 100 }),
        numberField(t('inspector.pivotY'), Math.round((k.pivot?.[1] ?? 0.5) * 100), (v) => update(e.id, { kind: { pivot: [k.pivot?.[0] ?? 0.5, v / 100] } }), { min: 0, max: 100 }),
      ]),
      colorField(t('inspector.color'), k.color, (v) => update(e.id, { kind: { color: v } })),
      numberField(t('inspector.lineWidth'), k.width, (v) => update(e.id, { kind: { width: Math.max(1, v) } }), { min: 1 }),
    ];
    return { content, appearance };
  }

  function graphForm(e) {
    const k = e.kind;
    // Data: what it plots, its vertical scale and how far back it looks.
    const content = [
      sensorPicker(e.id, k.binding.key, (key) => ({ kind: { binding: { key } } })),
      checkField(t('inspector.autoscale'), Boolean(k.autoscale), (v) => update(e.id, { kind: { autoscale: v } })),
      rangeFields(e.id, k.binding),
      numberField(t('inspector.history'), k.history, (v) => update(e.id, { kind: { history: Math.min(Math.max(2, v), 3600) } }), { min: 2, max: 3600 }),
    ];
    const appearance = [
      segmented(t('inspector.style'), k.style, [['line', t('graph.line')], ['area', t('graph.area')], ['bars', t('graph.bars')]], (v) => update(e.id, { kind: { style: v } })),
      colorField(t('inspector.color'), k.color, (v) => update(e.id, { kind: { color: v } })),
      k.style === 'area' && colorField(t('inspector.fill'), typeof k.fill === 'string' ? k.fill : '#38bdf840', (v) => update(e.id, { kind: { fill: v } })),
      numberField(t('inspector.lineWidth'), k.lineWidth, (v) => update(e.id, { kind: { lineWidth: Math.max(1, v) } }), { min: 1, step: 'any' }),
    ];
    return { content, appearance };
  }

  function imageForm(e, assets) {
    const k = e.kind;
    const paint = readIcon(assets.find((a) => a.ref === k.asset)?.iconSvg);
    const change = (patch) => editIcon(e.id, k.asset, { ...paint, ...patch });
    const options = assets.filter((a) => a.kind === 'image').map((a) => [a.ref, a.ref.split('/').pop()]);
    if (k.asset && !options.some(([r]) => r === k.asset)) options.push([k.asset, k.asset]);
    const content = [
      options.length
        ? selectField(t('inspector.asset'), k.asset, [['', t('inspector.noImage')], ...options], (v) => update(e.id, { kind: { asset: v } }))
        : el('p', { class: 'hint', text: t('inspector.addMediaFirst') }),
    ];
    const appearance = [
      ...fadeFields(e),
      paint && colorField(t('inspector.color'), paint.color, (color) => change({ color })),
      paint?.style === 'outline' && numberField(t('inspector.strokeWidth'), paint.stroke, (stroke) => change({ stroke: Math.min(4, Math.max(0.5, stroke)) }), { min: 0.5, max: 4, step: 0.5 }),
      paint && numberField(t('icons.shadow'), paint.shadow, (shadow) => change({ shadow: Math.min(4, Math.max(0, shadow)) }), { min: 0, max: 4, step: 0.5 }),
      paint && paint.shadow > 0 && colorField(t('icons.shadowColor'), paint.shadowColor, (shadowColor) => change({ shadowColor })),
      selectField(t('inspector.fit'), k.fit, ['fill', 'contain', 'cover', 'none'].map((f) => [f, t(`fit.${f}`)]), (v) => update(e.id, { kind: { fit: v } })),
    ];
    return { content, appearance };
  }

  function fadeFields(e) {
    const f = e.kind.fade;
    const set = patch => update(e.id, { kind: { fade: patch } });
    return [
      selectField(t('fade.mode'), !f ? 'none' : f.radial ? 'radial' : 'linear',
        ['none','linear','radial'].map(mode => [mode, t(`fade.${mode}`)]), mode => set(mode === 'none' ? null : {
          angle: f?.angle ?? 0, start: f?.start ?? 1, end: f?.end ?? 0,
          radial: mode === 'radial' ? f?.radial ?? [0.5,0.5,0.5] : null,
        })),
      ...(f ? [
        ...(f.radial ? [
          numberField(t('fade.centerX'), Math.round(f.radial[0]*100), x => set({ radial: [Math.max(0,Math.min(100,x))/100,f.radial[1],f.radial[2]] }), {min:0,max:100}),
          numberField(t('fade.centerY'), Math.round(f.radial[1]*100), y => set({ radial: [f.radial[0],Math.max(0,Math.min(100,y))/100,f.radial[2]] }), {min:0,max:100}),
          numberField(t('fade.extent'), Math.round(f.radial[2]*100), r => set({ radial: [f.radial[0],f.radial[1],Math.max(1,Math.min(200,r))/100] }), {min:1,max:200}),
        ] : [numberField(t('shape.fadeAngle'), f.angle, angle => set({angle:Math.max(-360,Math.min(360,angle))}), {min:-360,max:360})]),
        rangeField(t('shape.fadeStart'), Math.round(f.start*100), v => set({start:v/100}), {format:v=>`${v}%`}),
        rangeField(t('shape.fadeEnd'), Math.round(f.end*100), v => set({end:v/100}), {format:v=>`${v}%`}),
      ] : []),
    ];
  }

  function shapeForm(e) {
    const k = e.kind;
    return [
      segmented(t('inspector.shape'), k.shape, [['rect', t('shape.rect')], ['ellipse', t('shape.ellipse')]], (v) => update(e.id, { kind: { shape: v } })),
      ...(k.shape === 'rect' ? [
        checkField(t('inspector.individualCorners'),Boolean(k.corners),enabled=>update(e.id,{kind:{corners:enabled?Array(4).fill(k.radius ?? 0):null,radius:k.corners?.[0] ?? k.radius ?? 0}})),
        ...(k.corners ? ['topLeft','topRight','bottomRight','bottomLeft'].map((name,i)=>numberField(t(`inspector.${name}`),k.corners[i],v=>update(e.id,{kind:{corners:k.corners.map((r,n)=>n===i?Math.max(0,v):r)}}),{min:0})) : [numberField(t('inspector.radius'),k.radius ?? 0,v=>update(e.id,{kind:{radius:Math.max(0,v)}}),{min:0})]),
      ] : []),
      checkField(t('shape.videoWindow'), Boolean(k.videoWindow), (videoWindow) => update(e.id, { kind: { videoWindow } })),
      k.videoWindow ? el('div', {}, [el('p', { class: 'hint', text: t('shape.videoWindowHint') }), button(t('deviceVideo.choose'), () => video.openStorage())]) : colorField(t('inspector.fill'), typeof k.fill === 'string' ? k.fill : '#1e293bff', (v) => update(e.id, { kind: { fill: v } })),
      ...fadeFields(e),
      el('div', { class: 'field-row' }, [
        numberField(t('inspector.strokeWidth'), k.strokeWidth ?? 0, (v) => update(e.id, { kind: { strokeWidth: Math.max(0, v), stroke: v > 0 ? (k.stroke ?? '#ffffffff') : null } }), { min: 0 }),
      ]),
      (k.strokeWidth ?? 0) > 0 && colorField(t('inspector.stroke'), k.stroke ?? '#ffffffff', (v) => update(e.id, { kind: { stroke: v } })),
    ];
  }

  function showAs(e, theme) {
    const current = widgetOf(e);
    if (!BOUND.includes(current)) return null;
    const key = e.kind.content?.key ?? e.kind.binding?.key;
    const quantity = sensors.catalog().find((s) => s.key === key)?.quantity;
    return selectField(t('inspector.showAs'), current, BOUND.map((w) => [w, t(`widget.${w}`)]), (w) => {
      const made = createWidget(w, theme.canvas, { key, quantity });
      if (made.kind.binding && e.kind.binding) made.kind.binding = { ...made.kind.binding, min: e.kind.binding.min, max: e.kind.binding.max };
      store.dispatch('setKind', { id: e.id, kind: made.kind });
    });
  }

  /** The faces of a card as chips: the one in view is pressed and says so; "+" adds one. */
  function faceChips(e) {
    const c = e.card;
    const chips = c.faces.map((name, i) => {
      const active = i === c.activeFace;
      return el('button', {
        type: 'button', class: 'face-chip', dataset: { face: String(i) }, 'aria-pressed': String(active), 'aria-current': active ? 'true' : null,
        title: active ? t('layers.inViewHint') : null, onclick: () => !active && store.dispatch('cardFace', { id: e.id, face: i }),
      }, [
        el('span', { class: 'face-chip-name', text: name }),
        active && el('span', { class: 'face-badge', 'aria-hidden': 'true', text: t('layers.inView') }),
      ]);
    });
    const add = el('button', {
      type: 'button', class: 'face-chip face-add', 'aria-label': t('card.addFace'), title: t('card.addFace'), disabled: c.faces.length >= 16,
      onclick: () => store.dispatch('addCardFace', { id: e.id, name: t('card.faceName', { number: c.faces.length + 1 }) }),
    }, [icon(PLUS, 18)]);
    return el('div', { class: 'face-chips', role: 'group', 'aria-label': t('card.activeFace') }, [...chips, add]);
  }

  /** A card's rule list: when an app runs, plays or comes forward, show a face (no "active now": the Studio cannot know). */
  function triggerForm(e) {
    const c = e.card;
    const rules = c.triggers ?? [];
    const change = (i, patch) => update(e.id, { card: { triggers: rules.map((r, n) => n === i ? { ...r, ...patch } : r) } });
    const faceOptions = c.faces.map((name, n) => [String(n), name]);
    const rule = (r, i) => el('div', { class: 'trigger-rule', role: 'group', 'aria-label': t('trigger.rule', { value: i + 1 }) }, [
      el('div', { class: 'trigger-rule-head' }, [
        el('h4', { text: t('trigger.rule', { value: i + 1 }) }),
        el('button', { type: 'button', class: 'icon-button danger', 'aria-label': t('trigger.remove'), title: t('trigger.remove'), onclick: () => update(e.id, { card: { triggers: rules.filter((_, n) => n !== i) } }) }, [icon(ICONS.trash, 16)]),
      ]),
      selectField(t('trigger.condition'), r.source, ['process', 'processClosed', 'foreground', 'mediaPlaying'].map(v => [v, t(`trigger.${v}`)]), source => change(i, { source, app: r.app || (source === 'mediaPlaying' ? '' : 'Spotify') })),
      textField(t('trigger.app'), r.app, app => { if (app.trim() || r.source === 'mediaPlaying') change(i, { app: app.slice(0, 160) }); }),
      selectField(t('trigger.face'), String(r.face), faceOptions, face => change(i, { face: Number(face) })),
      el('div', { class: 'field-row' }, [
        numberField(t('trigger.priority'), r.priority ?? 0, priority => change(i, { priority: Math.max(0, Math.min(100, Math.round(priority))) }), { min: 0, max: 100 }),
        numberField(t('trigger.return'), r.returnSeconds ?? 0, returnSeconds => change(i, { returnSeconds: Math.max(0, Math.min(300, Math.round(returnSeconds))) }), { min: 0, max: 300 }),
      ]),
      selectField(t('trigger.returnFace'), r.returnFace == null ? 'previous' : String(r.returnFace), [['previous', t('trigger.previous')], ...faceOptions], v => change(i, { returnFace: v === 'previous' ? null : Number(v) })),
    ]);
    const add = () => update(e.id, { card: { triggers: [...rules, { source: 'mediaPlaying', app: 'Spotify', face: c.activeFace, priority: 0, returnSeconds: 3 }] } });
    return [
      section('triggers', t('trigger.title'), [
        el('p', { class: 'hint', text: t('trigger.help') }),
        ...rules.map(rule),
        el('button', { type: 'button', class: 'text-button trigger-add', disabled: rules.length >= 32 || c.faces.length < 2, onclick: add }, [icon(PLUS, 16), t('trigger.add')]),
      ]),
    ];
  }

  /** How a card's faces change: timed rotation and the transition, with a try on the next face. */
  function motionForm(e) {
    const c = e.card;
    const transition = c.transition && c.transition.effect !== 'none' ? c.transition : null;
    const effect = value => update(e.id, { card: { transition: { effect: value, direction: c.transition?.direction ?? 'left', durationMs: c.transition?.durationMs ?? 650, includeBase: c.transition?.includeBase ?? true } } });
    return [
      section('rotation', t('card.rotation'), [
        checkField(t('card.autoRotate'), c.rotationSeconds != null, enabled => update(e.id, { card: { rotationSeconds: enabled ? 10 : null } })),
        c.rotationSeconds != null && numberField(t('card.rotationSeconds'), c.rotationSeconds, value => update(e.id, { card: { rotationSeconds: Math.max(5, Math.min(3600, Math.round(value))) } }), { min: 5, max: 3600, step: 1 }),
        c.rotationSeconds != null && el('p', { class: 'hint', text: t('card.rotationHelp') }),
      ]),
      section('animation', t('card.animation'), [
        selectField(t('card.effect'), c.transition?.effect ?? 'none', ['none', 'fade', 'slide', 'flip'].map(value => [value, t(`card.effect.${value}`)]), effect),
        transition && rangeField(t('card.duration'), transition.durationMs, durationMs => update(e.id, { card: { transition: { durationMs } } }), { min: 150, max: 1500, step: 50, format: value => `${value} ms` }),
        transition && selectField(t('card.direction'), transition.direction, ['left', 'right', 'up', 'down'].map(value => [value, t(`card.direction.${value}`)]), direction => update(e.id, { card: { transition: { direction } } })),
        transition && checkField(t('card.includeBase'), transition.includeBase, includeBase => update(e.id, { card: { transition: { includeBase } } })),
        el('button', { type: 'button', class: 'text-button play-button', disabled: c.faces.length < 2, onclick: () => store.dispatch('cardFace', { id: e.id, face: (c.activeFace + 1) % c.faces.length }) }, [icon(ICONS.play, 16), t('card.animateNext')]),
        el('p', { class: 'hint', text: t('card.animationHelp') }),
      ]),
    ];
  }

  /** The Faces, Motion and Triggers tabs of a card (artboards CardFaces, CardMotion, CardTriggers). */
  function cardForm(e) {
    const c = e.card;
    const command = (name, args = {}) => () => store.dispatch(name, { id: e.id, ...args });
    const tool = (key, paths, onclick, disabled, extra = '') => el('button', { type: 'button', class: `icon-button ${extra}`.trim(), 'aria-label': t(key), title: t(key), disabled, onclick }, [icon(paths, 16)]);
    const faces = [
      section('faces', t('card.faces'), [faceChips(e), el('p', { class: 'hint', text: t('card.facesHint') })]),
      el('div', { class: 'face-editor' }, [
        textField(t('card.faceTitle'), c.faces[c.activeFace], name => {
          if (!name.trim()) return;
          const names = [...c.faces]; names[c.activeFace] = name.trim().slice(0, 32);
          update(e.id, { card: { faces: names } });
        }),
        el('div', { class: 'face-tools' }, [
          tool('card.previous', CHEVRON_LEFT, command('reorderCardFace', { direction: -1 }), c.activeFace === 0),
          tool('card.next', CHEVRON_RIGHT, command('reorderCardFace', { direction: 1 }), c.activeFace === c.faces.length - 1),
          el('button', { type: 'button', class: 'text-button', disabled: c.faces.length >= 16, onclick: command('duplicateCardFace') }, [icon(ICONS.copy, 16), t('card.duplicateFace')]),
          tool('card.removeFace', ICONS.trash, command('removeCardFace'), c.faces.length <= 1, 'danger'),
        ]),
      ]),
      el('p', { class: 'hint', text: t('card.help') }),
    ];
    return { faces, motion: motionForm(e), triggers: triggerForm(e) };
  }
  function membershipForm(e, theme) {
    if (e.card) return [];
    const cards = theme.elements.filter(p => p.card);
    if (!cards.length) return [];
    const parent = cards.find(p => p.id === e.cardMember?.parent);
    return [
      selectField(t('card.container'), parent ? String(parent.id) : '', [['', t('card.none')], ...cards.map(p => [String(p.id), p.name])], value => {
        const target = cards.find(p => String(p.id) === value);
        store.dispatch('attachCard', { ids: [e.id], parent: target?.id, face: target?.card.activeFace ?? null });
      }),
      parent && selectField(t('card.face'), e.cardMember.face === null ? '' : String(e.cardMember.face), [['', t('card.base')], ...parent.card.faces.map((name, i) => [String(i), name])], value => store.dispatch('attachCard', { ids: [e.id], parent: parent.id, face: value === '' ? null : Number(value) })),
    ].filter(Boolean);
  }

  /** A titled group of fields; nothing when it has no fields. */
  function section(key, label, nodes) {
    const contents = nodes.filter(Boolean);
    if (!contents.length) return null;
    return el('section', { class: 'property-section', dataset: { section: key } }, [
      el('h3', { text: label }),
      el('div', { class: 'property-section-body' }, contents),
    ]);
  }

  // The tab open for each kind of object: UI-only, it survives edits,
  // re-renders and Undo.
  const tabMemory = createTabMemory();

  const tabName = (kind, tab) => t(kind === 'card' && tab !== 'look' ? `card.tab.${tab}` : `inspector.tab.${tab}`);

  /**
   * The panels of an object as tabs (ARIA tabs: ←/→, Home, End), only those
   * with something in them; one alone is shown without a tab list.
   */
  function tabbed(kind, panels) {
    const offered = tabsOf(kind).filter(tab => panels[tab]?.some(Boolean));
    if (offered.length < 2) return offered.flatMap(tab => panels[tab]);
    const open = tabMemory.current(kind, offered);
    const list = el('div', { class: 'inspector-tabs', role: 'tablist', 'aria-label': t(kind === 'card' ? 'card.tabs' : 'inspector.tabs') });
    const views = offered.map(tab => el('div', {
      class: 'inspector-panel', role: 'tabpanel', id: `inspector-panel-${tab}`, 'aria-labelledby': `inspector-tab-${tab}`, hidden: tab !== open, dataset: { tab },
    }, panels[tab].filter(Boolean)));
    const show = (tab, focus) => {
      tabMemory.choose(kind, tab);
      for (const button of list.children) {
        const on = button.dataset.tab === tab;
        button.setAttribute('aria-selected', String(on));
        button.tabIndex = on ? 0 : -1;
        if (on && focus) button.focus();
      }
      for (const view of views) view.hidden = view.dataset.tab !== tab;
    };
    for (const tab of offered) {
      list.append(el('button', {
        type: 'button', role: 'tab', id: `inspector-tab-${tab}`, 'aria-controls': `inspector-panel-${tab}`, 'aria-selected': String(tab === open),
        tabindex: tab === open ? '0' : '-1', dataset: { tab }, text: tabName(kind, tab), onclick: () => show(tab, false),
      }));
    }
    list.addEventListener('keydown', evt => {
      const next = tabAfterKey(offered, evt.target.dataset?.tab, evt.key);
      if (!next) return;
      evt.preventDefault();
      show(next, true);
    });
    return [list, ...views];
  }

  /** The live preview: the object as the renderer drew it, copied from the frame (no render of its own). */
  function preview(e) {
    return el('figure', { class: 'inspector-preview', dataset: { elementId: String(e.id) } }, [
      el('figcaption', { class: 'inspector-preview-label', text: t('inspector.previewLive') }),
      el('div', { class: 'inspector-preview-stage' }, [
        el('canvas', { width: 1, height: 1, role: 'img', 'aria-label': t('inspector.previewOf', { name: e.name }) }),
        el('p', { class: 'hint inspector-preview-off', hidden: true, text: t('inspector.previewOff') }),
      ]),
    ]);
  }

  /** Copies the selected object out of the frame on the canvas into the live preview. */
  function paintPreview() {
    const figure = root.querySelector('.inspector-preview');
    const source = frameCanvas();
    if (!figure || !source) return;
    const e = store.getState().theme.elements.find(x => String(x.id) === figure.dataset.elementId);
    if (!e) return;
    const stage = figure.querySelector('.inspector-preview-stage');
    const target = stage.querySelector('canvas');
    const width = stage.clientWidth > 16 ? stage.clientWidth - 16 : 240;
    const crop = previewCrop(e.frame, { width: source.width, height: source.height }, { width, height: PREVIEW_HEIGHT });
    target.hidden = !crop;
    stage.querySelector('.inspector-preview-off').hidden = Boolean(crop);
    if (!crop) return;
    const ratio = window.devicePixelRatio || 1;
    target.width = Math.max(1, Math.round(crop.width * ratio));
    target.height = Math.max(1, Math.round(crop.height * ratio));
    target.style.width = `${crop.width}px`;
    target.style.height = `${crop.height}px`;
    const ctx = target.getContext('2d');
    ctx.clearRect(0, 0, target.width, target.height);
    ctx.drawImage(source, crop.sx, crop.sy, crop.sw, crop.sh, 0, 0, target.width, target.height);
  }

  /** The object's kind icon, type and name. */
  function header(e, widget, group) {
    return [
      el('div', { class: 'inspector-head' }, [
        el('span', { class: 'inspector-tile' }, [icon(e.isGroup ? GROUP_ICON : ICONS[widget] ?? ICONS.shape, 18)]),
        el('div', { class: 'inspector-title' }, [
          el('h2', { text: t(`widget.${widget}`) }),
          group && el('p', { class: 'hint', text: t('inspector.inGroup', { name: group.name }) }),
        ]),
      ]),
      el('div', { class: 'property-identity' }, [
        textField(t('inspector.name'), e.name, v => v.trim() && update(e.id, { name: v.trim() })),
      ]),
    ];
  }

  function elementForm(e, theme, assets) {
    const f = e.frame, widget = widgetOf(e);
    const kind = e.card ? 'card' : e.isGroup ? 'group' : widget;
    const setFrame = patch => store.dispatch('setFrame', { id: e.id, frame: { ...f, ...patch } });
    const kindForms = { text: textForm, bar: barForm, ring: ringForm, needle: needleForm, graph: graphForm };
    const specific = e.isGroup ? {} : e.kind.type === 'image' ? imageForm(e, assets) :
      e.kind.type === 'shape' ? { appearance: shapeForm(e) } : kindForms[e.kind.type]?.(e) ?? {};
    const group = theme.elements.find(p => p.id === e.groupParent && p.isGroup);
    const look = [section('appearance', t('inspector.appearance'), [
      ...(specific.appearance ?? []),
      rangeField(t('inspector.opacity'), Math.round((e.opacity ?? 1) * 100), v => update(e.id, { opacity: v / 100 }), { format: v => `${v}%` }),
      checkField(t('inspector.visible'), e.visible !== false, v => update(e.id, { visible: v })),
    ])];
    const panels = e.card ? { ...cardForm(e), look } : {
      data: [
        section('content', t('inspector.content'), [!e.isGroup && showAs(e, theme), ...(specific.content ?? [])]),
        section('membership', t('inspector.membership'), membershipForm(e, theme)),
      ],
      look,
    };
    return [
      ...header(e, widget, group),
      preview(e),
      ...tabbed(kind, panels),
      section('position', t('inspector.position'), [
        el('div', { class: 'field-row' }, [
          numberField(t('inspector.x'), f.x, v => setFrame({ x: v })),
          numberField(t('inspector.y'), f.y, v => setFrame({ y: v })),
          numberField(t('inspector.width'), f.width, v => setFrame({ width: Math.max(4, v) }), { min: 4 }),
          numberField(t('inspector.height'), f.height, v => setFrame({ height: Math.max(4, v) }), { min: 4 }),
        ]),
        checkField(t('inspector.locked'), Boolean(e.locked), v => update(e.id, { locked: v })),
      ]),
      section('actions', t('inspector.actions'), [el('div', { class: 'actions' }, [
        e.isGroup && el('button', { type: 'button', class: 'text-button', text: t('group.ungroup'), onclick: () => store.dispatch('ungroup', { ids: [e.id] }) }),
        el('button', { type: 'button', class: 'text-button', onclick: () => store.dispatch('reorder', { id: e.id, index: theme.elements.length }) }, [t('inspector.front')]),
        el('button', { type: 'button', class: 'text-button', onclick: () => store.dispatch('reorder', { id: e.id, index: 0 }) }, [t('inspector.back')]),
        el('button', { type: 'button', class: 'text-button', onclick: () => store.dispatch('duplicate', { ids: [e.id] }) }, [icon(ICONS.copy, 16), t('inspector.duplicate')]),
        el('button', { type: 'button', class: 'text-button danger', onclick: () => store.dispatch('remove', { ids: [e.id] }) }, [icon(ICONS.trash, 16), t('inspector.delete')]),
      ])]),
    ];
  }

  function multiForm(ids) {
    const tool = (edge, paths) => el('button', { type: 'button', class: 'icon-button', title: t(`align.${edge}`), 'aria-label': t(`align.${edge}`), onclick: () => store.dispatch('align', { ids, edge }) }, [icon(paths)]);
    return [
      el('h2', { text: t('inspector.multi', { count: ids.length }) }),
      section('position', t('inspector.alignTools'), [
        el('div', { class: 'button-row' }, [
          tool('left', ICONS.alignLeft), tool('centerX', ICONS.alignCenterX), tool('right', ICONS.alignRight),
          tool('top', ICONS.alignTop), tool('centerY', ICONS.alignCenterY), tool('bottom', ICONS.alignBottom),
        ]),
        el('div', { class: 'button-row' }, [
          el('button', { type: 'button', class: 'text-button', disabled: ids.length < 3, onclick: () => store.dispatch('distribute', { ids, axis: 'x' }) }, [icon(ICONS.distributeX, 16), t('align.distributeX')]),
          el('button', { type: 'button', class: 'text-button', disabled: ids.length < 3, onclick: () => store.dispatch('distribute', { ids, axis: 'y' }) }, [icon(ICONS.distributeY, 16), t('align.distributeY')]),
        ]),
      ]),
      section('membership', t('inspector.membership'), [
        el('button', { type: 'button', class: 'text-button', text: t('group.create'), disabled: !canGroup(store.getState().theme, ids), title: t('group.scope'), onclick: () => store.dispatch('groupSelection', { ids }) }),
        ...(store.getState().theme.elements.some(e => ids.includes(e.id) && e.isGroup) ? [el('button', { type: 'button', class: 'text-button', text: t('group.ungroup'), onclick: () => store.dispatch('ungroup', { ids }) })] : []),
        el('button', { type: 'button', class: 'text-button', text: t('card.group'), onclick: () => store.dispatch('cardFromSelection', { ids }) }),
      ]),
      section('actions', t('inspector.actions'), [
        el('div', { class: 'actions' }, [
          el('button', { type: 'button', class: 'text-button', onclick: () => store.dispatch('duplicate', { ids }) }, [icon(ICONS.copy, 16), t('inspector.duplicate')]),
          el('button', { type: 'button', class: 'text-button danger', onclick: () => store.dispatch('remove', { ids }) }, [icon(ICONS.trash, 16), t('inspector.delete')]),
        ]),
      ]),
    ];
  }

  /** Renders for the current store state. */
  function render(assets = []) {
    // A framing slider under the pointer stays; the form follows at its end.
    if (sliding && store.isGesturing()) return;
    sliding = false;
    const { theme, selection } = store.getState();
    const chosen = selection.map(id => theme.elements.find(e => e.id === id)).filter(Boolean);
    let nodes;
    if (chosen.length === 0) nodes = themeForm(theme, assets);
    else if (chosen.length === 1) nodes = elementForm(chosen[0], theme, assets);
    else nodes = multiForm(chosen.map((e) => e.id));
    // Keep focus where the user was: on the same tab, on the field of the
    // same label, or of the same stable id (the framing's).
    const active = document.activeElement;
    const inside = root.contains(active);
    const tab = inside && active.getAttribute('role') === 'tab' ? active.dataset.tab : null;
    const id = inside && !tab && active.id?.startsWith('framing-') ? active.id : null;
    const label = inside && !tab && !id ? active.getAttribute('aria-label') : null;
    root.replaceChildren(...nodes.filter(Boolean));
    if (tab) root.querySelector(`[role="tab"][data-tab="${CSS.escape(tab)}"]`)?.focus();
    else if (id) document.getElementById(id)?.focus();
    else if (label) root.querySelector(`[aria-label="${CSS.escape(label)}"]`)?.focus();
    paintPreview();
    shown = { assets, signature: videoSignature() };
  }

  function videoSignature() {
    const { theme, selection } = store.getState();
    if (selection.length || !['video', 'deviceVideo'].includes(theme.background.type)) return '';
    const c = video.context();
    if (theme.background.type === 'deviceVideo') return JSON.stringify([c.live, c.screen?.key, c.liveVideo?.state, c.liveVideo?.path]);
    return JSON.stringify([videoStatus(c), c.tools?.ready ?? null, c.tools?.installHints?.length ?? 0, c.auto?.rotation ?? null, Boolean(c.framing), c.motion !== false]);
  }

  /** The screen, live mode, the live video or ffmpeg changed. */
  function contextChanged() {
    const signature = videoSignature();
    if (signature !== shown.signature) render(shown.assets);
  }

  /** A new frame is on the canvas: the live preview copies the selected object from it again. */
  const frameShown = () => paintPreview();

  return { render, contextChanged, frameShown };
}

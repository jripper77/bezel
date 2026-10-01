// The right inspector: the theme when nothing is selected, one element's
// properties, or align/distribute tools for several. Every edit is one
// store command (one undo step).
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { checkField, colorField as colorInputs, numberField, rangeField, segmented, selectField, textField } from './fields.js';
import { createWidget, widgetOf } from '../editor/widgets.js';
import { ORIENTATIONS } from '../editor/geometry.js';
import { fileNameOf, videoFacts, videoStatus } from '../editor/background.js';
import { formatBytes } from './storage.js';

const BOUND = ['value', 'bar', 'ring', 'needle', 'graph'];
const CLOCK_PATTERNS = ['%H:%M', '%H:%M:%S', '%I:%M %p', '%d/%m/%Y', '%A', '%a %d %b', '%B %Y'];

/** The video-background actions and what they depend on, when none are given. */
const NO_VIDEO_ACTIONS = Object.freeze({
  context: () => ({ screen: null, live: false, liveVideo: null, tools: null, locale: 'en' }),
  useVideo: () => {},
  useImage: () => {},
  openStorage: () => {},
});

/**
 * @param {object} deps
 * @param {HTMLElement} deps.root
 * @param {() => number} deps.minRefresh the fastest refresh a theme may ask for, seconds (from the backend)
 * @param {{context: () => {screen: object|null, live: boolean, liveVideo: object|null, tools: object|null, locale: string}, useVideo: () => void, useImage: () => void, openStorage: () => void}} [deps.video]
 *   the screen and ffmpeg the video background is told against, and its
 *   actions: pick a video (or GIF) for the background, pick a picture, and
 *   show the storage tab (where the missing video is sent)
 */
export function createInspector({ root, store, t, sensors, minRefresh, video = NO_VIDEO_ACTIONS }) {
  const update = (id, patch) => store.dispatch('update', { id, patch });
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

  /**
   * A video background (a video or an animated GIF): its poster and name,
   * what the connected screen does with it (the storage tab sends a missing
   * one), what ffmpeg is for when there is no poster, and the other
   * backgrounds.
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
      button(t('inspector.useImage'), () => video.useImage()),
      useColor(),
    ]));
    return nodes;
  }

  function backgroundFields(bg, assets) {
    if (bg.type === 'video') return videoBackground(bg, assets);
    return [
      bg.type === 'color'
        ? colorField(t('inspector.color'), bg.color, (c) => store.dispatch('setTheme', { patch: { background: { type: 'color', color: c } } }))
        : el('p', { class: 'hint', text: t(`bg.${bg.type}`) }),
      el('div', { class: 'button-row' }, [
        bg.type !== 'color' && useColor(),
        button(t('inspector.useVideo'), () => video.useVideo()),
      ]),
    ];
  }

  function themeForm(theme, assets) {
    return [
      el('h2', { text: t('inspector.theme') }),
      textField(t('inspector.name'), theme.name, (v) => store.dispatch('setTheme', { patch: { name: v } })),
      el('p', { class: 'hint', text: t('inspector.canvas', { width: theme.canvas.width, height: theme.canvas.height }) }),
      selectField(t('inspector.orientation'), theme.orientation, ORIENTATIONS.map((o) => [o, t(`orientation.${o}`)]), (o) => store.dispatch('setOrientation', { orientation: o })),
      el('h3', { text: t('inspector.background') }),
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
    const nodes = [];
    if (c.type === 'static') nodes.push(textField(t('inspector.text'), c.text, (v) => update(e.id, { kind: { content: { text: v } } })));
    if (c.type === 'clock') {
      nodes.push(textField(t('inspector.pattern'), c.pattern, (v) => update(e.id, { kind: { content: { pattern: v } } }), { list: 'clock-patterns' }));
      nodes.push(el('datalist', { id: 'clock-patterns' }, CLOCK_PATTERNS.map((p) => el('option', { value: p }))));
      nodes.push(el('p', { class: 'hint', text: t('inspector.patternHelp') }));
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
    nodes.push(el('h3', { text: t('inspector.font') }));
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
    return nodes;
  }

  const trackField = (e) => el('div', {}, [
    checkField(t('inspector.showTrack'), Boolean(e.kind.track), (on) => update(e.id, { kind: { track: on ? '#ffffff26' : null } })),
    e.kind.track && colorField(t('inspector.track'), typeof e.kind.track === 'string' ? e.kind.track : '#ffffff26', (v) => update(e.id, { kind: { track: v } })),
  ]);

  function barForm(e) {
    const k = e.kind;
    return [
      sensorPicker(e.id, k.binding.key, (key) => ({ kind: { binding: { key } } })),
      rangeFields(e.id, k.binding),
      selectField(t('inspector.direction'), k.direction, ['leftToRight', 'rightToLeft', 'bottomToTop', 'topToBottom'].map((d) => [d, t(`dir.${d}`)]), (v) => update(e.id, { kind: { direction: v } })),
      colorField(t('inspector.fill'), typeof k.fill === 'string' ? k.fill : '#38bdf8ff', (v) => update(e.id, { kind: { fill: v } })),
      trackField(e),
      numberField(t('inspector.radius'), k.radius ?? 0, (v) => update(e.id, { kind: { radius: Math.max(0, v) } }), { min: 0 }),
      segmentsFields(e),
    ];
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

  function ringForm(e) {
    const k = e.kind;
    return [
      sensorPicker(e.id, k.binding.key, (key) => ({ kind: { binding: { key } } })),
      rangeFields(e.id, k.binding),
      el('div', { class: 'field-row' }, [
        numberField(t('inspector.startAngle'), k.startAngle, (v) => update(e.id, { kind: { startAngle: v } }), { min: -360, max: 360 }),
        numberField(t('inspector.sweep'), k.sweep, (v) => update(e.id, { kind: { sweep: v } }), { min: 1, max: 360 }),
      ]),
      numberField(t('inspector.thickness'), k.thickness, (v) => update(e.id, { kind: { thickness: Math.max(1, v) } }), { min: 1 }),
      checkField(t('inspector.clockwise'), k.clockwise !== false, (v) => update(e.id, { kind: { clockwise: v } })),
      checkField(t('inspector.roundCaps'), Boolean(k.roundCaps), (v) => update(e.id, { kind: { roundCaps: v } })),
      colorField(t('inspector.fill'), typeof k.fill === 'string' ? k.fill : '#38bdf8ff', (v) => update(e.id, { kind: { fill: v } })),
      trackField(e),
      segmentsFields(e),
    ];
  }

  function needleForm(e) {
    const k = e.kind;
    return [
      sensorPicker(e.id, k.binding.key, (key) => ({ kind: { binding: { key } } })),
      rangeFields(e.id, k.binding),
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
  }

  function graphForm(e) {
    const k = e.kind;
    return [
      sensorPicker(e.id, k.binding.key, (key) => ({ kind: { binding: { key } } })),
      rangeFields(e.id, k.binding),
      checkField(t('inspector.autoscale'), Boolean(k.autoscale), (v) => update(e.id, { kind: { autoscale: v } })),
      segmented(t('inspector.style'), k.style, [['line', t('graph.line')], ['area', t('graph.area')], ['bars', t('graph.bars')]], (v) => update(e.id, { kind: { style: v } })),
      numberField(t('inspector.history'), k.history, (v) => update(e.id, { kind: { history: Math.min(Math.max(2, v), 3600) } }), { min: 2, max: 3600 }),
      colorField(t('inspector.color'), k.color, (v) => update(e.id, { kind: { color: v } })),
      k.style === 'area' && colorField(t('inspector.fill'), typeof k.fill === 'string' ? k.fill : '#38bdf840', (v) => update(e.id, { kind: { fill: v } })),
      numberField(t('inspector.lineWidth'), k.lineWidth, (v) => update(e.id, { kind: { lineWidth: Math.max(1, v) } }), { min: 1, step: 'any' }),
    ];
  }

  function imageForm(e, assets) {
    const k = e.kind;
    const options = assets.filter((a) => a.kind === 'image').map((a) => [a.ref, a.ref.split('/').pop()]);
    if (k.asset && !options.some(([r]) => r === k.asset)) options.push([k.asset, k.asset]);
    return [
      options.length
        ? selectField(t('inspector.asset'), k.asset, [['', t('inspector.noImage')], ...options], (v) => update(e.id, { kind: { asset: v } }))
        : el('p', { class: 'hint', text: t('inspector.addMediaFirst') }),
      selectField(t('inspector.fit'), k.fit, ['fill', 'contain', 'cover', 'none'].map((f) => [f, t(`fit.${f}`)]), (v) => update(e.id, { kind: { fit: v } })),
    ];
  }

  function shapeForm(e) {
    const k = e.kind;
    return [
      segmented(t('inspector.shape'), k.shape, [['rect', t('shape.rect')], ['ellipse', t('shape.ellipse')]], (v) => update(e.id, { kind: { shape: v } })),
      k.shape === 'rect' && numberField(t('inspector.radius'), k.radius ?? 0, (v) => update(e.id, { kind: { radius: Math.max(0, v) } }), { min: 0 }),
      colorField(t('inspector.fill'), typeof k.fill === 'string' ? k.fill : '#1e293bff', (v) => update(e.id, { kind: { fill: v } })),
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

  // ------------------------------------------------------------ element --
  function elementForm(e, theme, assets) {
    const f = e.frame;
    const setFrame = (patch) => store.dispatch('setFrame', { id: e.id, frame: { ...f, ...patch } });
    const kindForms = { text: textForm, bar: barForm, ring: ringForm, needle: needleForm, graph: graphForm, shape: shapeForm };
    const specific = e.kind.type === 'image' ? imageForm(e, assets) : (kindForms[e.kind.type]?.(e) ?? []);
    return [
      el('h2', { text: t(`widget.${widgetOf(e)}`) }),
      textField(t('inspector.name'), e.name, (v) => v.trim() && update(e.id, { name: v.trim() })),
      showAs(e, theme),
      el('h3', { text: t('inspector.position') }),
      el('div', { class: 'field-row' }, [
        numberField(t('inspector.x'), f.x, (v) => setFrame({ x: v })),
        numberField(t('inspector.y'), f.y, (v) => setFrame({ y: v })),
        numberField(t('inspector.width'), f.width, (v) => setFrame({ width: Math.max(4, v) }), { min: 4 }),
        numberField(t('inspector.height'), f.height, (v) => setFrame({ height: Math.max(4, v) }), { min: 4 }),
      ]),
      rangeField(t('inspector.opacity'), Math.round((e.opacity ?? 1) * 100), (v) => update(e.id, { opacity: v / 100 }), { format: (v) => `${v}%` }),
      el('div', { class: 'field-row' }, [
        checkField(t('inspector.visible'), e.visible !== false, (v) => update(e.id, { visible: v })),
        checkField(t('inspector.locked'), Boolean(e.locked), (v) => update(e.id, { locked: v })),
      ]),
      el('h3', { text: t('inspector.appearance') }),
      ...specific,
      el('div', { class: 'actions' }, [
        el('button', { type: 'button', class: 'text-button', onclick: () => store.dispatch('reorder', { id: e.id, index: theme.elements.length }) }, [t('inspector.front')]),
        el('button', { type: 'button', class: 'text-button', onclick: () => store.dispatch('reorder', { id: e.id, index: 0 }) }, [t('inspector.back')]),
        el('button', { type: 'button', class: 'text-button', onclick: () => store.dispatch('duplicate', { ids: [e.id] }) }, [icon(ICONS.copy, 16), t('inspector.duplicate')]),
        el('button', { type: 'button', class: 'text-button danger', onclick: () => store.dispatch('remove', { ids: [e.id] }) }, [icon(ICONS.trash, 16), t('inspector.delete')]),
      ]),
    ];
  }

  function multiForm(ids) {
    const tool = (edge, paths) => el('button', { type: 'button', class: 'icon-button', title: t(`align.${edge}`), 'aria-label': t(`align.${edge}`), onclick: () => store.dispatch('align', { ids, edge }) }, [icon(paths)]);
    return [
      el('h2', { text: t('inspector.multi', { count: ids.length }) }),
      el('h3', { text: t('inspector.alignTools') }),
      el('div', { class: 'button-row' }, [
        tool('left', ICONS.alignLeft), tool('centerX', ICONS.alignCenterX), tool('right', ICONS.alignRight),
        tool('top', ICONS.alignTop), tool('centerY', ICONS.alignCenterY), tool('bottom', ICONS.alignBottom),
      ]),
      el('div', { class: 'button-row' }, [
        el('button', { type: 'button', class: 'text-button', disabled: ids.length < 3, onclick: () => store.dispatch('distribute', { ids, axis: 'x' }) }, [icon(ICONS.distributeX, 16), t('align.distributeX')]),
        el('button', { type: 'button', class: 'text-button', disabled: ids.length < 3, onclick: () => store.dispatch('distribute', { ids, axis: 'y' }) }, [icon(ICONS.distributeY, 16), t('align.distributeY')]),
      ]),
      el('div', { class: 'actions' }, [
        el('button', { type: 'button', class: 'text-button', onclick: () => store.dispatch('duplicate', { ids }) }, [icon(ICONS.copy, 16), t('inspector.duplicate')]),
        el('button', { type: 'button', class: 'text-button danger', onclick: () => store.dispatch('remove', { ids }) }, [icon(ICONS.trash, 16), t('inspector.delete')]),
      ]),
    ];
  }

  /** Renders for the current store state. */
  function render(assets = []) {
    const { theme, selection } = store.getState();
    const chosen = theme.elements.filter((e) => selection.includes(e.id));
    let nodes;
    if (chosen.length === 0) nodes = themeForm(theme, assets);
    else if (chosen.length === 1) nodes = elementForm(chosen[0], theme, assets);
    else nodes = multiForm(chosen.map((e) => e.id));
    // Keep focus where the user was typing when the form re-renders.
    const active = document.activeElement;
    const focusLabel = root.contains(active) ? active.getAttribute('aria-label') || active.id : null;
    root.replaceChildren(...nodes.filter(Boolean));
    if (focusLabel) root.querySelector(`[aria-label="${CSS.escape(focusLabel)}"]`)?.focus();
    shown = { assets, signature: videoSignature() };
  }

  function videoSignature() {
    const { theme, selection } = store.getState();
    if (selection.length || theme.background.type !== 'video') return '';
    const c = video.context();
    return JSON.stringify([videoStatus(c), c.tools?.ready ?? null, c.tools?.installHints?.length ?? 0]);
  }

  /** The screen, live mode, the live video or ffmpeg changed. */
  function contextChanged() {
    const signature = videoSignature();
    if (signature !== shown.signature) render(shown.assets);
  }

  return { render, contextChanged };
}

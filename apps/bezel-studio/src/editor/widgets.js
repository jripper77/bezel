// The widget palette: what each draggable widget creates, in theme.json
// shape (see crates/bezel-themes/src/dto.rs). Defaults are sized relative to
// the canvas so they look right on a 0.96" and on a 12.3" panel alike.

const WHITE = '#ffffffff';
const ACCENT = '#38bdf8ff';
const TRACK = '#ffffff26';

const baseFont = () => ({ family: 'Inter', weight: 600, italic: false });

/** Text style for a given pixel size. */
export function textStyle(size, align = 'left') {
  return { font: baseFont(), size, paint: WHITE, align, valign: 'middle', letterSpacing: 0 };
}

/** Sensor range that makes sense by default for a quantity. */
export function defaultRange(quantity) {
  switch (quantity) {
    case 'celsius':
      return { min: 20, max: 100 };
    case 'megahertz':
      return { min: 0, max: 6000 };
    case 'watts':
      return { min: 0, max: 300 };
    case 'rpm':
      return { min: 0, max: 3000 };
    default:
      return { min: 0, max: 100 };
  }
}

/** Widget ids in palette order. */
export const WIDGETS = Object.freeze(['text', 'value', 'clock', 'image', 'shape', 'bar', 'ring', 'needle', 'graph']);

/**
 * The kind object and default size of a new widget.
 * @param {string} widget one of WIDGETS
 * @param {{width:number,height:number}} canvas
 * @param {{key?:string, quantity?:string}} [sensor] sensor to bind, when dropped from the sensor browser
 * @returns {{kind: object, width: number, height: number}}
 */
export function createWidget(widget, canvas, sensor = {}) {
  const short = Math.min(canvas.width, canvas.height);
  const key = sensor.key ?? 'cpu.usage';
  const range = defaultRange(sensor.quantity);
  const binding = { key, ...range };
  const fontSize = Math.max(10, Math.round(short / 12));
  switch (widget) {
    case 'text':
      return { kind: { type: 'text', content: { type: 'static', text: 'Text' }, style: textStyle(fontSize) }, width: short * 0.6, height: fontSize * 1.5 };
    case 'value':
      return {
        kind: {
          type: 'text',
          content: { type: 'sensor', key, format: { showUnit: true, fahrenheit: false, decimalBytes: false }, prefix: '', suffix: '' },
          style: textStyle(Math.round(fontSize * 1.4)),
        },
        width: short * 0.6,
        height: fontSize * 2,
      };
    case 'clock':
      return { kind: { type: 'text', content: { type: 'clock', pattern: '%H:%M' }, style: textStyle(Math.round(fontSize * 2), 'center') }, width: short * 0.8, height: fontSize * 2.6 };
    case 'image':
      return { kind: { type: 'image', asset: '', fit: 'contain' }, width: short * 0.5, height: short * 0.5 };
    case 'shape':
      return { kind: { type: 'shape', shape: 'rect', radius: Math.round(short / 24), fill: '#1e293bff', strokeWidth: 0 }, width: short * 0.8, height: short * 0.3 };
    case 'bar':
      return { kind: { type: 'bar', binding, direction: 'leftToRight', fill: ACCENT, track: TRACK, radius: Math.round(short / 48) }, width: short * 0.8, height: Math.max(6, Math.round(short / 24)) };
    case 'ring':
      return {
        kind: { type: 'ring', binding, startAngle: -135, sweep: 270, thickness: Math.max(4, Math.round(short / 20)), clockwise: true, fill: ACCENT, track: TRACK, roundCaps: true },
        width: short * 0.6,
        height: short * 0.6,
      };
    case 'needle':
      return {
        kind: { type: 'needle', binding, pivot: [0.5, 0.5], startAngle: -120, sweep: 240, color: '#f97316ff', width: Math.max(2, Math.round(short / 80)) },
        width: short * 0.6,
        height: short * 0.6,
      };
    case 'graph':
      return {
        kind: { type: 'graph', binding, history: 60, style: 'area', color: ACCENT, fill: '#38bdf840', lineWidth: 2, autoscale: false },
        width: short * 0.8,
        height: short * 0.35,
      };
    default:
      throw new Error(`unknown widget ${widget}`);
  }
}

/** The sensor key an element reads, or null. */
export function boundKey(element) {
  const k = element.kind;
  if (k.type === 'text' && k.content.type === 'sensor') return k.content.key;
  if (k.binding) return k.binding.key;
  return null;
}

/** Which widget an element was made from (for icons and names). */
export function widgetOf(element) {
  const k = element.kind;
  if (k.type === 'text') {
    if (k.content.type === 'sensor') return 'value';
    if (k.content.type === 'clock') return 'clock';
    return 'text';
  }
  return k.type;
}

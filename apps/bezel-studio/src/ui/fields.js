// Form field builders for the inspector. Each returns a labelled element and
// calls `onChange(value)` with a parsed value; nothing here knows the theme.
import { el } from './dom.js';

let counter = 0;
const fieldId = () => `f${(counter += 1)}`;

/** A labelled number input. `step` 1 rounds to integers. */
export function numberField(label, value, onChange, { min, max, step = 1 } = {}) {
  const id = fieldId();
  const input = el('input', { id, type: 'number', value: String(value ?? ''), min, max, step });
  input.addEventListener('change', () => {
    const v = Number(input.value);
    if (Number.isFinite(v)) onChange(step === 1 ? Math.round(v) : v);
  });
  return el('label', { class: 'field', for: id }, [el('span', { text: label }), input]);
}

/** A labelled text input (committed on change). */
export function textField(label, value, onChange, { placeholder, list } = {}) {
  const id = fieldId();
  const input = el('input', { id, type: 'text', value: value ?? '', placeholder, list, spellcheck: false });
  input.addEventListener('change', () => onChange(input.value));
  return el('label', { class: 'field', for: id }, [el('span', { text: label }), input]);
}

/** A labelled select from `[value, label]` pairs or `{group: [[v,l]]}` groups. */
export function selectField(label, value, options, onChange) {
  const id = fieldId();
  const select = el('select', { id });
  const add = (parent, [v, l]) => parent.append(el('option', { value: v, text: l, selected: v === value }));
  if (Array.isArray(options)) options.forEach((o) => add(select, o));
  else {
    for (const [group, list] of Object.entries(options)) {
      const og = el('optgroup', { label: group });
      list.forEach((o) => add(og, o));
      select.append(og);
    }
  }
  select.value = value ?? '';
  select.addEventListener('change', () => onChange(select.value));
  return el('label', { class: 'field', for: id }, [el('span', { text: label }), select]);
}

/** A checkbox. */
export function checkField(label, checked, onChange) {
  const input = el('input', { type: 'checkbox', checked: Boolean(checked) });
  input.addEventListener('change', () => onChange(input.checked));
  return el('label', { class: 'check' }, [input, el('span', { text: label })]);
}

/** A range slider with a numeric readout. */
export function rangeField(label, value, onChange, { min = 0, max = 100, step = 1, format = (v) => String(v) } = {}) {
  const id = fieldId();
  const out = el('output', { for: id, text: format(value) });
  const input = el('input', { id, type: 'range', min, max, step, value: String(value) });
  input.addEventListener('input', () => { out.textContent = format(Number(input.value)); });
  input.addEventListener('change', () => onChange(Number(input.value)));
  return el('label', { class: 'field', for: id }, [el('span', {}, [label, ' ', out]), input]);
}

/** Segmented buttons for a small set of choices. */
export function segmented(label, value, options, onChange) {
  const group = el('div', { class: 'segmented', role: 'group', 'aria-label': label });
  for (const [v, l] of options) {
    group.append(el('button', { type: 'button', text: l, 'aria-pressed': String(v === value), onclick: () => onChange(v) }));
  }
  return el('div', { class: 'field' }, [el('span', { text: label }), group]);
}

/** Splits `#rrggbbaa` into `#rrggbb` and an alpha percentage. */
export function splitColor(hex) {
  const h = typeof hex === 'string' && hex.startsWith('#') ? hex.slice(1) : 'ffffffff';
  const rgb = h.length >= 6 ? `#${h.slice(0, 6)}` : '#ffffff';
  const a = h.length === 8 ? parseInt(h.slice(6, 8), 16) : 255;
  return { rgb: rgb.toLowerCase(), alpha: Math.round((a / 255) * 100) };
}

/** Joins `#rrggbb` and an alpha percentage into `#rrggbbaa`. */
export function joinColor(rgb, alphaPercent) {
  const a = Math.round((Math.max(0, Math.min(100, alphaPercent)) / 100) * 255);
  return `${rgb.toLowerCase()}${a.toString(16).padStart(2, '0')}`;
}

/** A color with opacity: native picker + hex text + alpha %. */
export function colorField(label, hex, onChange, { alphaLabel = 'α' } = {}) {
  const { rgb, alpha } = splitColor(hex);
  const picker = el('input', { type: 'color', value: rgb, 'aria-label': label });
  const text = el('input', { type: 'text', value: rgb, 'aria-label': label, spellcheck: false });
  const a = el('input', { type: 'number', min: 0, max: 100, step: 1, value: String(alpha), 'aria-label': `${label} ${alphaLabel}` });
  const commit = () => onChange(joinColor(picker.value, Number(a.value)));
  picker.addEventListener('input', () => { text.value = picker.value; });
  picker.addEventListener('change', commit);
  text.addEventListener('change', () => {
    if (/^#[0-9a-f]{6}$/i.test(text.value)) {
      picker.value = text.value.toLowerCase();
      commit();
    }
  });
  a.addEventListener('change', commit);
  return el('div', { class: 'field' }, [el('span', { text: label }), el('div', { class: 'color-field' }, [picker, text, a])]);
}

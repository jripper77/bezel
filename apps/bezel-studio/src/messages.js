// Messages the backend sends as a stable code with named arguments
// (D-2026-09-30-release-polish-6): import warnings, and errors. The UI
// translates the code and fills in the arguments; the English `message`
// is shown only for a code this UI does not know.

/**
 * The text of an import warning `{code, args, message}`; the `layer`
 * argument names a kind of layer, translated too.
 * @param {(k: string, p?: object) => string} t
 * @param {{code?: string, args?: Record<string, string>, message?: string}|string} w
 */
export function warningText(t, w) {
  if (typeof w === 'string') return w;
  const key = `importWarning.${w?.code}`;
  if (!t.has(key)) return w?.message ?? String(w?.code);
  const args = { ...(w.args ?? {}) };
  if (args.layer !== undefined && t.has(`importWarning.layer.${args.layer}`)) {
    args.layer = t(`importWarning.layer.${args.layer}`);
  }
  return t(key, args);
}

/**
 * The translation key and params of a failed command's text: a backend
 * error `{code, args, message}` by its code (`error.<code>`, only when the
 * UI knows it); anything else (an unknown code, a JS error, a string)
 * `error.unknown` with its own message.
 * @param {{has: (k: string) => boolean}} t
 * @param {unknown} e
 * @returns {{key: string, params: Record<string, unknown>}}
 */
export function errorMessage(t, e) {
  const code = typeof e?.code === 'string' ? e.code : null;
  if (code && t.has(`error.${code}`)) return { key: `error.${code}`, params: e.args ?? {} };
  const message = e?.message ?? (typeof e === 'string' ? e : String(e));
  return { key: 'error.unknown', params: { message } };
}

/**
 * The text of a failed command (`errorMessage` translated).
 * @param {(k: string, p?: object) => string} t
 * @param {unknown} e
 */
export function errorText(t, e) {
  const { key, params } = errorMessage(t, e);
  return t(key, params);
}

/**
 * The name of a sensor of the catalog: the UI's own for the well-known
 * keys, else the name the machine gave it (a chip, a disk, a network card).
 * @param {(k: string, p?: object) => string} t
 * @param {{key: string, label: string}} sensor
 */
export function sensorLabel(t, sensor) {
  const key = `sensor.${sensor.key}`;
  return t.has(key) ? t(key) : sensor.label;
}

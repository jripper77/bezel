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
 * The text of a failed command: a backend error `{code, args, message}`
 * translated by its code; anything else (an unknown code, a JS error, a
 * string) with its own message.
 * @param {(k: string, p?: object) => string} t
 * @param {unknown} e
 */
export function errorText(t, e) {
  const code = typeof e?.code === 'string' ? e.code : null;
  if (code && t.has(`error.${code}`)) return t(`error.${code}`, e.args ?? {});
  const message = e?.message ?? (typeof e === 'string' ? e : String(e));
  return t('error.unknown', { message });
}

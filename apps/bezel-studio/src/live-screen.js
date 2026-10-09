// Which listed screen is the live one (D-2026-10-01-live-screen-controls-5).
// The backend may name the live screen by any of its ports: a rev C screen
// answers to its display and to its wake chip (MCU), and one put live by its
// MCU port was reported by that port (0.1.0-dev.287). The UI selects screens
// only by the key they are listed with.

/**
 * Whether `screen` answers to `address`: its key, its display's address or
 * its wake chip's.
 * @param {{key: string, display?: {address: string}|null, wake?: {address: string}|null}} screen
 * @param {string} address
 */
export function answersTo(screen, address) {
  return screen.key === address || screen.display?.address === address || screen.wake?.address === address;
}

/**
 * The listed screen the live key names (its key first, else its display's
 * or its wake chip's address), else `null`: a key no listed screen answers
 * to selects none.
 * @param {{key: string, display?: {address: string}|null, wake?: {address: string}|null}[]} screens
 * @param {string|null|undefined} liveKey
 */
export function liveScreenIn(screens, liveKey) {
  if (!liveKey) return null;
  return screens.find((s) => s.key === liveKey) ?? screens.find((s) => answersTo(s, liveKey)) ?? null;
}

/** The protocol families with a name of their own (`screen.family.<id>`). */
export const SCREEN_FAMILIES = Object.freeze(['turing-rev-a', 'turing-rev-b', 'turing-rev-c', 'turing-usb']);

/** A recognized model, otherwise the known protocol family or USB name; retain the
 * address so two identical panels remain distinguishable. Discovery need not
 * open a serial port or guess the exact model to provide a useful label.
 * @param {object} screen
 * @param {(key: string) => string} t the translator that names the family
 */
export function deviceLabel(screen, t) {
  const model = screen.models?.length === 1 ? screen.models[0] : null;
  if (model) return `${model.name} · ${model.width}×${model.height} · ${screen.key}`;
  const endpoint = screen.display ?? screen.wake;
  const manufacturer = endpoint?.manufacturer?.trim();
  const product = endpoint?.product?.trim();
  const usbName = [...new Set([manufacturer, product].filter(Boolean))].join(' ');
  const family = SCREEN_FAMILIES.includes(screen.family) ? t(`screen.family.${screen.family}`) : null;
  const name = family || usbName || screen.models?.[0]?.name;
  return name ? `${name} · ${screen.key}` : screen.key;
}

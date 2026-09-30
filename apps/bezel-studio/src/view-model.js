// Pure view logic: turns the screens the backend reports into what the UI
// shows. No DOM here, so every rule is unit-tested in Node.

const CAPABILITIES = Object.freeze([
  'brightness',
  'deviceRotation',
  'partialUpdate',
  'backplateLed',
  'storage',
  'videoPlayback',
]);

/**
 * The single model of a screen, or null while several candidates remain.
 * @param {{models: Array<object>}} screen
 */
export function soleModel(screen) {
  return screen.models.length === 1 ? screen.models[0] : null;
}

/**
 * Card shown in the screen list.
 * @param {object} screen a ScreenDto
 * @param {(key: string, params?: object) => string} t
 */
export function screenCard(screen, t) {
  const model = soleModel(screen);
  const title = model ? model.name : screen.models.map((m) => m.name).join(' / ');
  const meta = model ? `${model.width}×${model.height} · ${model.diagonal}` : t('model.unknown');
  return {
    key: screen.key,
    title,
    meta,
    state: screen.state,
    stateLabel: t(`state.${screen.state}`),
  };
}

/**
 * Status line for a list of screens.
 * @param {number} count
 * @param {(key: string, params?: object) => string} t
 */
export function countLabel(count, t) {
  if (count === 0) return t('screens.count.zero');
  if (count === 1) return t('screens.count.one');
  return t('screens.count.other', { count });
}

/**
 * Size of the device preview that fits `box` while keeping the panel's
 * proportions. Landscape-native themes are a later concern; the preview shows
 * the panel in portrait form.
 * @param {{width: number, height: number}} panel
 * @param {{width: number, height: number}} box available space in CSS pixels
 */
export function fitPreview(panel, box) {
  if (panel.width <= 0 || panel.height <= 0 || box.width <= 0 || box.height <= 0) {
    return { width: 0, height: 0 };
  }
  const scale = Math.min(box.width / panel.width, box.height / panel.height);
  return { width: Math.floor(panel.width * scale), height: Math.floor(panel.height * scale) };
}

/**
 * Rows of the details panel.
 * @param {object} screen a ScreenDto
 * @param {(key: string, params?: object) => string} t
 * @returns {Array<{label: string, value: string}>}
 */
export function detailRows(screen, t) {
  const model = soleModel(screen);
  const none = t('details.none');
  const endpoint = (e) => (e ? `${e.address} (${e.usb}${e.serial ? `, ${e.serial}` : ''})` : none);
  const caps = model
    ? CAPABILITIES.filter((c) => model.capabilities[c]).map((c) => t(`capability.${c}`)).join(', ')
    : none;
  return [
    { label: t('details.model'), value: model ? model.name : t('model.unknown') },
    { label: t('details.resolution'), value: model ? `${model.width}×${model.height}` : none },
    { label: t('details.diagonal'), value: model ? model.diagonal : none },
    { label: t('details.family'), value: screen.family },
    { label: t('details.display'), value: endpoint(screen.display) },
    { label: t('details.wake'), value: endpoint(screen.wake) },
    { label: t('details.capabilities'), value: caps || none },
    { label: t('details.validated'), value: model && model.hardwareValidated ? t('yes') : t('no') },
  ];
}

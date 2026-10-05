// The preferences dialog (the top bar's button): the app's language, which
// follows the system's until the user picks one, and the sensors that take
// settings (the host `net.ping` measures, and on Linux the folder of
// MangoHud's logs for `gpu.fps`). Each change applies at once and the
// backend remembers it.
import { el, icon } from './dom.js';
import { ICONS } from './icons.js';
import { errorText } from '../messages.js';

/** The languages the studio speaks. */
export const LANGUAGES = Object.freeze(['pt-BR', 'en']);

/**
 * The options of the language select: the system's (named) first, then
 * each language by its own name.
 * @param {(k: string, p?: object) => string} t
 * @param {string} systemLanguage
 * @returns {Array<[string, string]>} `[value, label]`, `''` for the system's
 */
export function languageOptions(t, systemLanguage) {
  return [
    ['', t('prefs.languageSystem', { language: t(`language.${systemLanguage}`) })],
    ...LANGUAGES.map((l) => [l, t(`language.${l}`)]),
  ];
}

/**
 * @param {object} deps
 * @param {(k: string, p?: object) => string} deps.t
 * @param {object} deps.bridge
 * @param {(locale: string) => void} deps.onLanguage the UI switches to `locale`
 * @param {(message: string) => void} deps.notify
 */
export function createPreferences({ t, bridge, onLanguage, notify }) {
  let dialog = null;
  let prefs = null;
  // Why the last sensor change was refused (shown under its field), and the
  // host typed then (kept in the field to fix it).
  let sensorError = null;
  let typedHost = null;

  function languageField() {
    const select = el('select', { id: 'prefs-language', 'aria-describedby': 'prefs-language-hint' });
    for (const [value, label] of languageOptions(t, prefs.systemLanguage)) {
      select.append(el('option', { value, text: label, selected: value === (prefs.language ?? '') }));
    }
    select.addEventListener('change', () => chooseLanguage(select.value || null));
    return el('section', { class: 'prefs-section', 'aria-labelledby': 'prefs-language-title' }, [
      el('h3', { id: 'prefs-language-title', text: t('prefs.languageTitle') }),
      el('label', { class: 'field', for: 'prefs-language' }, [el('span', { text: t('prefs.language') }), select]),
      el('p', { id: 'prefs-language-hint', class: 'hint', text: t('prefs.languageHint') }),
    ]);
  }

  function runtimeSection() {
    if (!prefs.lightRuntime) return null;
    const input = el('input', { id: 'prefs-light-on-close', type: 'checkbox', role: 'switch',
      checked: prefs.lightOnClose, 'aria-describedby': 'prefs-light-hint' });
    input.addEventListener('change', async () => {
      input.disabled = true;
      try {
        await bridge.setLightOnClose(input.checked);
        prefs.lightOnClose = input.checked;
      } catch (e) {
        input.checked = prefs.lightOnClose;
        notify(errorText(t, e));
      } finally { input.disabled = false; }
    });
    return el('section', { class: 'prefs-section', 'aria-labelledby': 'prefs-runtime-title' }, [
      el('h3', { id: 'prefs-runtime-title', text: t('prefs.runtimeTitle') }),
      el('label', { class: 'check', for: 'prefs-light-on-close' }, [input, el('span', { text: t('prefs.lightOnClose') })]),
      el('p', { id: 'prefs-light-hint', class: 'hint', text: t('prefs.lightHint') }),
    ]);
  }

  function pingField() {
    const input = el('input', {
      id: 'prefs-ping-host', type: 'text', value: typedHost ?? prefs.pingHost, spellcheck: false, autocomplete: 'off',
      'aria-describedby': sensorError ? 'prefs-ping-hint prefs-sensor-error' : 'prefs-ping-hint',
      'aria-invalid': sensorError ? 'true' : null,
    });
    input.addEventListener('change', () => saveSensors({ pingHost: input.value }));
    const error = sensorError
      ? el('p', { id: 'prefs-sensor-error', class: 'field-error', role: 'alert', text: sensorError })
      : null;
    return [
      el('label', { class: 'field', for: 'prefs-ping-host' }, [el('span', { text: t('prefs.pingHost') }), input]),
      error,
      el('p', { id: 'prefs-ping-hint', class: 'hint', text: t('prefs.pingHint', { host: prefs.defaultPingHost }) }),
    ];
  }

  function mangohudField() {
    if (!prefs.mangohud) return [];
    const choose = el('button', { type: 'button', class: 'text-button', id: 'prefs-mangohud-choose', text: t('prefs.mangohudChoose'), onclick: pickMangohud });
    const reset = prefs.mangohudDir
      ? el('button', { type: 'button', class: 'text-button', id: 'prefs-mangohud-reset', text: t('prefs.mangohudReset'), onclick: () => saveSensors({ mangohudDir: null }) })
      : null;
    return [
      el('div', { class: 'field', role: 'group', 'aria-labelledby': 'prefs-mangohud-label' }, [
        el('span', { id: 'prefs-mangohud-label', text: t('prefs.mangohud') }),
        el('p', { class: `folder-path${prefs.mangohudDir ? '' : ' default'}`, text: prefs.mangohudDir ?? t('prefs.mangohudDefault') }),
        el('div', { class: 'button-row' }, [choose, reset]),
      ]),
      el('p', { class: 'hint', text: t('prefs.mangohudHint') }),
    ];
  }

  function sensorsSection() {
    return el('section', { class: 'prefs-section', 'aria-labelledby': 'prefs-sensors-title' }, [
      el('h3', { id: 'prefs-sensors-title', text: t('prefs.sensorsTitle') }),
      ...pingField(),
      ...mangohudField(),
    ]);
  }

  async function pickMangohud() {
    let folder = null;
    try {
      folder = await bridge.pickFolder();
    } catch (e) {
      notify(errorText(t, e));
      return;
    }
    if (folder) await saveSensors({ mangohudDir: folder });
  }

  /** Saves the sensor options with `change`; a refusal is shown in place. */
  async function saveSensors(change) {
    const next = { pingHost: prefs.pingHost, mangohudDir: prefs.mangohudDir, ...change };
    try {
      await bridge.setSensorOptions(next.pingHost, next.mangohudDir);
      prefs = await bridge.preferences();
      sensorError = null;
      typedHost = null;
      notify(t('prefs.sensorsSaved'));
    } catch (e) {
      sensorError = errorText(t, e);
      typedHost = change.pingHost ?? null;
    }
    render();
  }

  async function chooseLanguage(language) {
    try {
      await bridge.setLanguage(language);
      prefs = { ...prefs, language };
      onLanguage(language ?? prefs.systemLanguage);
      render();
    } catch (e) {
      notify(errorText(t, e));
    }
  }

  /** Draws the dialog again; the focused control keeps the focus. */
  function render() {
    if (!dialog) return;
    const focused = dialog.contains(document.activeElement) ? document.activeElement.id : null;
    const close = el('button', { type: 'button', class: 'icon-button dialog-close', title: t('dialog.close'), 'aria-label': t('dialog.close'), onclick: () => dialog.close() }, [icon(ICONS.close, 16)]);
    dialog.replaceChildren(
      el('div', { class: 'dialog-head' }, [el('h2', { id: 'prefs-title', text: t('prefs.title') }), close]),
      el('div', { class: 'dialog-body' }, [languageField(), runtimeSection(), sensorsSection()]),
      el('div', { class: 'dialog-actions' }, [el('button', { type: 'button', class: 'primary-button', text: t('prefs.done'), onclick: () => dialog.close() })]),
    );
    if (focused) (dialog.querySelector(`#${focused}`) ?? dialog.querySelector('#prefs-mangohud-choose'))?.focus();
  }

  /** Opens the dialog; focus returns to the opener when it closes. */
  async function open() {
    if (dialog) return;
    try {
      prefs = await bridge.preferences();
    } catch (e) {
      notify(errorText(t, e));
      return;
    }
    const opener = document.activeElement;
    sensorError = null;
    typedHost = null;
    dialog = el('dialog', { class: 'confirm-dialog prefs-dialog', 'aria-labelledby': 'prefs-title' });
    dialog.addEventListener('close', () => {
      dialog.remove();
      dialog = null;
      if (opener?.isConnected) opener.focus();
    }, { once: true });
    render();
    document.body.append(dialog);
    dialog.showModal();
    dialog.querySelector('#prefs-language').focus();
  }

  return { open };
}

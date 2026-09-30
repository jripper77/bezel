// The preferences dialog (the top bar's button): the app's language, which
// follows the system's until the user picks one. Each change applies at
// once and the backend remembers it.
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

  async function chooseLanguage(language) {
    try {
      await bridge.setLanguage(language);
      prefs = { ...prefs, language };
      onLanguage(language ?? prefs.systemLanguage);
      render();
      dialog?.querySelector('#prefs-language')?.focus();
    } catch (e) {
      notify(errorText(t, e));
    }
  }

  function render() {
    if (!dialog) return;
    const close = el('button', { type: 'button', class: 'icon-button dialog-close', title: t('dialog.close'), 'aria-label': t('dialog.close'), onclick: () => dialog.close() }, [icon(ICONS.close, 16)]);
    dialog.replaceChildren(
      el('div', { class: 'dialog-head' }, [el('h2', { id: 'prefs-title', text: t('prefs.title') }), close]),
      el('div', { class: 'dialog-body' }, [languageField()]),
      el('div', { class: 'dialog-actions' }, [el('button', { type: 'button', class: 'primary-button', text: t('prefs.done'), onclick: () => dialog.close() })]),
    );
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

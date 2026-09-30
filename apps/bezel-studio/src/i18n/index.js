// Tiny i18n: pick a locale from the browser, translate keys with {param}
// substitution, and apply `data-i18n*` attributes to the DOM.
import ptBR from './pt-BR.js';
import en from './en.js';

export const LOCALES = Object.freeze({ 'pt-BR': ptBR, en });

/**
 * The best supported locale for a list of browser languages.
 * @param {readonly string[]} languages e.g. navigator.languages
 * @returns {'pt-BR' | 'en'}
 */
export function pickLocale(languages) {
  for (const lang of languages) {
    const lower = String(lang).toLowerCase();
    if (lower.startsWith('pt')) return 'pt-BR';
    if (lower.startsWith('en')) return 'en';
  }
  return 'en';
}

/**
 * A translator bound to a locale. Unknown keys come back as the key itself.
 * @param {'pt-BR' | 'en'} locale
 */
export function translator(locale) {
  const table = LOCALES[locale] ?? en;
  return (key, params = {}) => {
    const text = table[key] ?? en[key] ?? key;
    return text.replace(/\{(\w+)\}/g, (_, name) => String(params[name] ?? `{${name}}`));
  };
}

/**
 * Applies `data-i18n` (text), `data-i18n-title`, `data-i18n-aria-label` and `data-i18n-placeholder`.
 * @param {ParentNode} root
 * @param {(key: string) => string} t
 */
export function applyTranslations(root, t) {
  for (const el of root.querySelectorAll('[data-i18n]')) el.textContent = t(el.dataset.i18n);
  for (const el of root.querySelectorAll('[data-i18n-title]')) el.title = t(el.dataset.i18nTitle);
  for (const el of root.querySelectorAll('[data-i18n-aria-label]')) {
    el.setAttribute('aria-label', t(el.dataset.i18nAriaLabel));
  }
  for (const el of root.querySelectorAll('[data-i18n-placeholder]')) el.placeholder = t(el.dataset.i18nPlaceholder);
}

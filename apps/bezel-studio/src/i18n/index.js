// Tiny i18n: pick a locale from the browser, translate keys with {param}
// substitution, and apply `data-i18n*` attributes to the DOM.
import ptBR from './pt-BR.js';
import en from './en.js';
import it from './it.js';

export const LOCALES = Object.freeze({ 'pt-BR': ptBR, en, it });

/**
 * The best supported locale for a list of browser languages.
 * @param {readonly string[]} languages e.g. navigator.languages
 * @returns {'pt-BR' | 'en' | 'it'}
 */
export function pickLocale(languages) {
  for (const lang of languages) {
    const lower = String(lang).toLowerCase();
    if (lower.startsWith('pt')) return 'pt-BR';
    if (lower.startsWith('en')) return 'en';
    if (lower.startsWith('it')) return 'it';
  }
  return 'en';
}

/**
 * A translator bound to a locale. Unknown keys come back as the key itself;
 * `t.has(key)` tells whether a key is known.
 * @param {'pt-BR' | 'en' | 'it'} locale
 */
export function translator(locale) {
  const table = LOCALES[locale] ?? en;
  const t = (key, params = {}) => {
    const text = table[key] ?? en[key] ?? key;
    return text.replace(/\{(\w+)\}/g, (_, name) => String(params[name] ?? `{${name}}`));
  };
  t.has = (key) => key in table || key in en;
  return t;
}

/**
 * The language of the guide pages for a UI locale: the guide is written in
 * English and Portuguese only, so any other locale reads the English one.
 * @param {string} locale
 * @returns {'pt-BR' | 'en'}
 */
export function guideLocale(locale) {
  return locale === 'pt-BR' ? 'pt-BR' : 'en';
}

/** The `{name}` placeholders of a text, sorted and once each. */
export function placeholders(text) {
  return [...new Set([...String(text).matchAll(/\{(\w+)\}/g)].map((m) => m[1]))].sort();
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

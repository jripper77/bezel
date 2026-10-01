// The Themes tab's filter: which library themes the gallery lists (those
// that fit the screen in use, or all of them; vertical, horizontal or both),
// what it says when none is left, how a card names the screen a theme was
// made for, and under which key a theme's thumbnail is kept.
import { isHorizontal } from './editor/geometry.js';

/** What the gallery lists: the themes for the screen in use, or all of them. */
export const SCOPES = Object.freeze(['screen', 'all']);

/** The orientations the gallery lists: both, or one of them. */
export const AXES = Object.freeze(['all', 'vertical', 'horizontal']);

/** The filter "Show all" leaves: every theme, in both orientations. */
export const SHOW_ALL = Object.freeze({ scope: 'all', axis: 'all' });

/** `vertical` or `horizontal` for a theme entry (by orientation, else by shape). */
export function axisOf(entry) {
  if (entry.orientation) return isHorizontal(entry.orientation) ? 'horizontal' : 'vertical';
  return entry.canvas.width > entry.canvas.height ? 'horizontal' : 'vertical';
}

/**
 * The filter as the preferences remember it (`themeFilter`). The scope is
 * `null` until the user chooses one; anything unknown counts as not chosen.
 * @param {{scope?: string|null, axis?: string}|null|undefined} saved
 * @returns {{scope: 'screen'|'all'|null, axis: 'all'|'vertical'|'horizontal'}}
 */
export function rememberedFilter(saved) {
  return {
    scope: SCOPES.includes(saved?.scope) ? saved.scope : null,
    axis: AXES.includes(saved?.axis) ? saved.axis : 'all',
  };
}

/**
 * The scope in effect: every theme while no screen is known; otherwise the
 * one chosen, "for this screen" until the user chooses.
 */
export function scopeIn(filter, screen) {
  if (!screen) return 'all';
  return filter.scope ?? 'screen';
}

/**
 * Whether a theme fits `screen`: one of the catalog models the backend says
 * it fits (the core's rule) is a model the screen may be.
 * @param {{models?: string[]}} entry
 * @param {{models: {id: string}[]}|null} screen
 */
export function fitsScreen(entry, screen) {
  const ids = new Set((screen?.models ?? []).map((m) => m.id));
  return (entry.models ?? []).some((id) => ids.has(id));
}

/** The entries `filter` keeps for `screen` (the one in use, or `null`), in order. */
export function filterThemes(list, filter, screen) {
  const scope = scopeIn(filter, screen);
  return list.filter((entry) => (scope === 'all' || fitsScreen(entry, screen)) && (filter.axis === 'all' || axisOf(entry) === filter.axis));
}

/**
 * What the gallery says when the filter keeps nothing: the message's key,
 * and whether "Show all" would bring themes back.
 */
export function emptyState(list, filter, screen) {
  const byScreen = scopeIn(filter, screen) === 'screen';
  const byAxis = filter.axis !== 'all';
  if (!list.length || (!byScreen && !byAxis)) return { key: 'themes.empty', showAll: false };
  if (byScreen && byAxis) return { key: 'themes.noneForScreenAxis', showAll: true };
  return { key: byScreen ? 'themes.noneForScreen' : 'themes.noneForAxis', showAll: true };
}

/**
 * How many themes the gallery shows: the key of the sentence and its
 * values (`null` when there is none to show).
 */
export function countText(shown, total) {
  if (!shown) return null;
  if (shown < total) return { key: 'themes.countSome', params: { shown, total } };
  return shown === 1 ? { key: 'themes.countOne', params: {} } : { key: 'themes.countAll', params: { count: shown } };
}

/**
 * How a card names the screen a theme was made for: its diagonal in the
 * UI's language and its pixels (`8.8″ · 1920×480`, `8,8″ · 1920×480`), or
 * only the pixels when the panel comes in several sizes.
 * @param {{canvas: {width: number, height: number}, diagonalHundredths?: number|null}} entry
 * @param {string} locale
 */
export function screenLabel(entry, locale) {
  const pixels = `${entry.canvas.width}×${entry.canvas.height}`;
  if (!entry.diagonalHundredths) return pixels;
  const inches = new Intl.NumberFormat(locale, { maximumFractionDigits: 2 }).format(entry.diagonalHundredths / 100);
  return `${inches}″ · ${pixels}`;
}

/** The key a theme's thumbnail is kept under: its location and its files' revision. */
export function thumbnailKey(entry) {
  return `${entry.location}\n${entry.revision ?? ''}`;
}

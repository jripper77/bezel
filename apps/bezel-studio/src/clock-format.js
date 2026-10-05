// Mirrors the native clock formatter for inspector examples and browser previews.
export const CLOCK_PATTERNS = ['%H:%M', '%H:%M:%S', '%I:%M %p', '%d/%m/%Y', '%A %e %B', '%A', '%a %d %b', '%B %Y'];

export function formatClock(pattern, date = new Date(), language = null, casing = 'normal', system = globalThis.navigator?.languages?.[0] ?? 'en') {
  const chosen = (language || system).toLowerCase();
  const locale = chosen.startsWith('it') ? 'it' : chosen.startsWith('pt') ? 'pt-BR' : 'en';
  const day = new Intl.DateTimeFormat(locale, { weekday: 'long' }).format(date);
  const month = new Intl.DateTimeFormat(locale, { month: 'long' }).format(date);
  const two = (n) => String(n).padStart(2, '0');
  const short = (word) => [...word].slice(0, 3).join('');
  const values = { H: two(date.getHours()), I: two(date.getHours() % 12 || 12), p: date.getHours() < 12 ? 'AM' : 'PM', M: two(date.getMinutes()), S: two(date.getSeconds()), d: two(date.getDate()), e: String(date.getDate()), m: two(date.getMonth() + 1), Y: String(date.getFullYear()), y: two(((date.getFullYear() % 100) + 100) % 100), A: day, a: short(day), B: month, b: short(month), '%': '%' };
  const text = pattern.replace(/%([\s\S])/g, (all, code) => values[code] ?? all);
  if (casing === 'upper') return text.toUpperCase();
  if (casing === 'title') return text.replace(/(^|[^\p{L}])(\p{L})/gu, (_, prefix, letter) => prefix + letter.toUpperCase());
  return text;
}

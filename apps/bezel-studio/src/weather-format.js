import { WEATHER_CONDITIONS } from './i18n/weather.js';
// Localised weather text shared by the demo renderer and its examples.
export function weatherFamily(code) {
  if (code === 0) return 0;
  if (code === 1 || code === 2) return 1;
  if (code === 3) return 2;
  if (code === 45 || code === 48) return 3;
  if ((code >= 51 && code <= 67) || (code >= 80 && code <= 82)) return 4;
  if ((code >= 71 && code <= 77) || code === 85 || code === 86) return 5;
  if (code >= 95 && code <= 99) return 6;
  return 7;
}
export function weatherText(content, temperature, code, system = globalThis.navigator?.languages?.[0] ?? 'en') {
  const language = (content.language || system).toLowerCase();
  const labels = WEATHER_CONDITIONS[language.startsWith('it') ? 'it' : language.startsWith('pt') ? 'pt-BR' : 'en'];
  const value = content.fahrenheit ? temperature * 1.8 + 32 : temperature;
  return `${content.city}\n${Math.round(value)}\u00b0${content.fahrenheit ? 'F' : 'C'}\n${labels[weatherFamily(code)]}`;
}

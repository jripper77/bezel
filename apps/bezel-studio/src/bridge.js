// The only module that knows where data comes from: Tauri's IPC in the app,
// or an in-memory scenario in demo mode (a browser on localhost, used by the
// Playwright suite and for UI work without hardware).
import { SCENARIOS } from './demo-data.js';

/**
 * Picks the backend for this page.
 * @param {{__TAURI__?: any, location: {hostname: string, search: string}}} win
 */
export function createBridge(win) {
  const invoke = win.__TAURI__?.core?.invoke;
  if (typeof invoke === 'function') {
    return { mode: 'tauri', listScreens: () => invoke('list_screens') };
  }
  const local = ['localhost', '127.0.0.1'].includes(win.location.hostname);
  if (!local) {
    return { mode: 'unavailable', listScreens: () => Promise.reject(new Error('no backend')) };
  }
  const name = new URLSearchParams(win.location.search).get('demo') ?? 'turing88';
  const scenario = SCENARIOS[name] ?? SCENARIOS.turing88;
  return {
    mode: 'demo',
    listScreens: () =>
      scenario.error ? Promise.reject(new Error(scenario.error)) : Promise.resolve(structuredClone(scenario.screens)),
  };
}

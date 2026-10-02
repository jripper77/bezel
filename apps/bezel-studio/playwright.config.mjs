// The studio UI in demo mode (no `window.__TAURI__`, served from localhost),
// served as the static files Tauri embeds, in light and dark themes and in
// pt-BR and en: the system's language picks the UI's, and every spec looks
// for texts through the studio's own translations. Every run starts its own
// server so a leftover one cannot serve another checkout.
import { defineConfig } from '@playwright/test';

const PORT = Number(process.env.BEZEL_E2E_PORT) || 1430;

/** A project in `colorScheme` and `locale`. The same two settings go in its
 *  metadata: the JSON report keeps a project's metadata but not its `use`,
 *  and scripts/e2e-passed.mjs reads them there. */
function project(name, colorScheme, locale) {
  return { name, use: { colorScheme, locale }, metadata: { colorScheme, locale } };
}

export default defineConfig({
  testDir: 'tests/e2e',
  fullyParallel: true,
  forbidOnly: true,
  retries: 0,
  reporter: 'list',
  use: {
    browserName: 'chromium',
    baseURL: `http://localhost:${PORT}`,
    viewport: { width: 1280, height: 800 },
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    project('light-pt', 'light', 'pt-BR'),
    project('dark-pt', 'dark', 'pt-BR'),
    project('light-en', 'light', 'en-US'),
    project('dark-en', 'dark', 'en-US'),
  ],
  webServer: {
    command: `python3 -m http.server ${PORT} --directory src`,
    url: `http://localhost:${PORT}/index.html`,
    reuseExistingServer: false,
    stdout: 'ignore',
    stderr: 'ignore',
  },
});

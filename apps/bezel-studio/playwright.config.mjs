// The studio UI in demo mode (no `window.__TAURI__`, served from localhost),
// served as the static files Tauri embeds, in light and dark themes. Every
// run starts its own server so a leftover one cannot serve another checkout.
import { defineConfig } from '@playwright/test';

const PORT = 1430;

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
    locale: 'pt-BR',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'light', use: { colorScheme: 'light' } },
    { name: 'dark', use: { colorScheme: 'dark' } },
  ],
  webServer: {
    command: `python3 -m http.server ${PORT} --directory src`,
    url: `http://localhost:${PORT}/index.html`,
    reuseExistingServer: false,
    stdout: 'ignore',
    stderr: 'ignore',
  },
});

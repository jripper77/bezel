// Installs the Chromium the Playwright suite runs on, before `npm test`
// starts it. Only a GitHub-hosted runner —
// throwaway, with its own passwordless sudo — also gets the system
// libraries (`--with-deps`, which runs apt as root). A developer's machine
// and a self-hosted runner only get the browser, in the user's cache: this
// script never asks for sudo there. With the browser already cached, the
// install is a no-op.

import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

/**
 * The `playwright` arguments for the environment `env`.
 *
 * @param {Record<string, string | undefined>} env the process environment
 * @returns {string[]}
 */
export function installArgs(env) {
  const githubHosted =
    env.GITHUB_ACTIONS === 'true' && env.RUNNER_ENVIRONMENT === 'github-hosted';
  return githubHosted ? ['install', '--with-deps', 'chromium'] : ['install', 'chromium'];
}

// The CLI runs through this same Node, with an argument array and no shell:
// nothing from the environment is ever parsed as a command line.
function install() {
  const cli = createRequire(import.meta.url).resolve('@playwright/test/cli');
  const run = spawnSync(process.execPath, [cli, ...installArgs(process.env)], {
    stdio: 'inherit',
  });
  if (run.error) {
    console.error(run.error.message);
    return 1;
  }
  return run.status ?? 1;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  process.exitCode = install();
}

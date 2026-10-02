// Did the named Playwright tests really pass? A count of passed tests cannot
// tell: one parked test plus any other test keeps the count. This takes each
// test by name ("<describe> › <title>", as Playwright prints it) and
// requires, in EVERY project the config declares (read from the report,
// never counted here), a run that passed (not skipped, not in Playwright's
// parked state, not flaky, not expected to fail) and that recorded the axe
// check: `expectAccessible` annotates the run with `axe`. A test of the run
// that failed, named or not, fails it too. The projects must cover light
// and dark in pt-BR and in en: the JSON report drops a project's `use`, so
// the config copies its `colorScheme` and `locale` into the project's
// `metadata`, which the report keeps, and this reads them there.
//
//   node scripts/e2e-passed.mjs [--grep <pattern>] [--report <report.json>] "<describe> › <title>"...
//
// Without --report it runs `playwright test` (by default on the named tests'
// outer describes) with the JSON reporter into a temporary file; the port is
// BEZEL_E2E_PORT's, as for every run. A --report file is taken relative to
// the app folder and must lie in it or in the temporary directory; any other
// is refused. Exit: 0 every run proves its test, 1 something is missing
// (each problem named), 2 bad arguments.

import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

/** How Playwright joins a describe and a title. */
export const SEPARATOR = ' › ';
/** The annotation `expectAccessible` leaves on a run whose axe check passed. */
export const AXE = 'axe';
/** The verdict of a run that proves its test. */
export const OK = 'ok';
/** The verdict of a run Playwright parked; the annotation type it gives such a run too. */
export const PARKED = 'fixme';
/** The colour schemes whose every combination with {@link LOCALES} a project must run. */
export const SCHEMES = ['light', 'dark'];
/** The UI's locales; a project's locale counts for its language ("en-US" is "en"). */
export const LOCALES = ['pt-BR', 'en'];

const USAGE = 'usage: node scripts/e2e-passed.mjs [--grep <pattern>] [--report <report.json>] "<describe> › <title>"...';
const OPTIONS = { grep: { type: 'string' }, report: { type: 'string' } };
const APP = resolve(fileURLToPath(new URL('..', import.meta.url)));
/** The folders a `--report` file may lie in: the temporary directory and the app folder. */
export const REPORT_FOLDERS = [resolve(tmpdir()), APP];

/**
 * The `--report` file `arg` names, resolved against the app folder, or
 * `null` when it lies outside {@link REPORT_FOLDERS} (`../` included).
 */
export function reportFile(arg) {
  const file = resolve(APP, arg);
  const [temporary, app] = REPORT_FOLDERS;
  if (file.startsWith(temporary + sep) || file.startsWith(app + sep)) return file;
  return null;
}

/** `text` as Playwright names a test: its parts split on "›", trimmed, joined by " › ". */
export function testName(text) {
  return text
    .split('›')
    .map((part) => part.trim())
    .filter(Boolean)
    .join(SEPARATOR);
}

function parse(argv) {
  try {
    return parseArgs({ args: argv, options: OPTIONS, allowPositionals: true });
  } catch (err) {
    return { error: err.message };
  }
}

/** The options (`report` resolved by {@link reportFile}) and the required test names in `argv`, or `{ error }`. */
export function readArgs(argv) {
  const parsed = parse(argv);
  if (parsed.error) return parsed;
  const required = [...new Set(parsed.positionals.map(testName).filter(Boolean))];
  if (required.length === 0) return { error: 'no test named' };
  const { grep, report } = parsed.values;
  const args = grep === undefined ? { required } : { grep, required };
  if (report === undefined) return args;
  const file = reportFile(report);
  if (file === null) return { error: `--report ${report}: not in ${REPORT_FOLDERS.join(' or ')}` };
  return { ...args, report: file };
}

/** A Playwright `--grep` matching, literally, the outer describe (or the title) of each name. */
export function grepFor(required) {
  const outer = new Set(required.map((name) => name.split(SEPARATOR)[0]));
  return [...outer].map((text) => text.replaceAll(/[.*+?^${}()|[\]\\]/g, String.raw`\$&`)).join('|');
}

function collectRuns(suite, path, runs) {
  for (const spec of suite.specs ?? []) {
    const name = [...path, spec.title].join(SEPARATOR);
    runs.set(name, [...(runs.get(name) ?? []), ...(spec.tests ?? [])]);
  }
  for (const child of suite.suites ?? []) collectRuns(child, [...path, child.title], runs);
}

/** Every run in a JSON report, by test name (its describes and title, not its file). */
export function runsByName(report) {
  const runs = new Map();
  for (const file of report.suites ?? []) collectRuns(file, [], runs);
  return runs;
}

/** The projects the report's config declares. */
export function projectsOf(report) {
  return (report.config?.projects ?? []).map((project) => project.name);
}

/** Whether `locale` is `wanted` or one of its regions. */
const speaks = (locale, wanted) => locale === wanted || (typeof locale === 'string' && locale.startsWith(`${wanted}-`));

/** Each "<scheme> × <locale>" no project of the report runs in (read from its metadata). */
export function uncovered(report) {
  const settings = (report.config?.projects ?? []).map((project) => project.metadata ?? {});
  const runsIn = (scheme, locale) => settings.some((s) => s.colorScheme === scheme && speaks(s.locale, locale));
  return LOCALES.flatMap((locale) => SCHEMES.filter((scheme) => !runsIn(scheme, locale)).map((scheme) => `${scheme} × ${locale}`));
}

const has = (annotations, type) => (annotations ?? []).some((a) => a.type === type);

/** What one run (a JSON report's test in one project) proves: `ok`, or why not. */
export function verdictOf(run) {
  if (has(run.annotations, PARKED)) return PARKED;
  if (run.status === 'skipped' || run.status === 'flaky') return run.status;
  const last = run.results?.at(-1);
  if (run.status !== 'expected' || run.expectedStatus !== 'passed' || last?.status !== 'passed') return 'failed';
  return has(last.annotations ?? run.annotations, AXE) ? OK : 'no axe';
}

function cellOf(runs, project) {
  const mine = runs.filter((run) => run.projectName === project);
  if (mine.length === 0) return 'missing';
  return mine.map(verdictOf).find((verdict) => verdict !== OK) ?? OK;
}

function othersFailed(runs, required) {
  const failed = (run) => run.status === 'unexpected' || run.status === 'flaky';
  return [...runs]
    .filter(([name]) => !required.includes(name))
    .flatMap(([name, list]) => list.filter(failed).map((run) => `${name} [${run.projectName}]: ${verdictOf(run)} (not named, but in the run)`));
}

/** Each required test in each project of `report`: the table's rows, and every problem. */
export function checkReport(report, required) {
  const projects = projectsOf(report);
  const runs = runsByName(report);
  const rows = required.map((name) => ({ name, cells: projects.map((project) => cellOf(runs.get(name) ?? [], project)) }));
  const problems = rows.flatMap(({ name, cells }) =>
    cells.flatMap((cell, i) => (cell === OK ? [] : [`${name} [${projects[i]}]: ${cell}`])),
  );
  if (projects.length === 0) problems.push('the report declares no project');
  const lacking = uncovered(report).map((combination) => `no project runs in ${combination}`);
  return { projects, rows, problems: [...problems, ...lacking, ...othersFailed(runs, required)] };
}

/** The rows as a fixed-width table, under a header naming the projects. */
export function formatTable({ projects, rows }) {
  const lines = [['test', ...projects], ...rows.map((row) => [row.name, ...row.cells])];
  const widths = lines[0].map((_, col) => Math.max(...lines.map((line) => line[col].length)));
  return lines.map((line) => line.map((cell, col) => cell.padEnd(widths[col])).join('  ').trimEnd()).join('\n');
}

/** One line on the whole check, then each problem. */
export function formatVerdict({ projects, rows, problems }) {
  const runs = rows.length * projects.length;
  if (problems.length === 0) return `ok: ${rows.length} tests × ${projects.length} projects, ${runs}/${runs} runs passed with axe`;
  return [`FAIL: ${problems.length} problem(s)`, ...problems.map((problem) => `  - ${problem}`)].join('\n');
}

function readReport(file) {
  return JSON.parse(readFileSync(file, 'utf8'));
}

// The CLI runs through this same Node, with an argument array and no shell.
function runPlaywright(grep, env) {
  const dir = mkdtempSync(join(tmpdir(), 'bezel-e2e-'));
  const file = join(dir, 'report.json');
  try {
    const cli = createRequire(import.meta.url).resolve('@playwright/test/cli');
    const run = spawnSync(process.execPath, [cli, 'test', '--reporter=dot,json', '--grep', grep], {
      cwd: APP,
      stdio: 'inherit',
      env: { ...env, PLAYWRIGHT_JSON_OUTPUT_FILE: file },
    });
    if (run.error) throw run.error;
    return readReport(file);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function loadReport(args, env) {
  try {
    return { report: args.report ? readReport(args.report) : runPlaywright(args.grep ?? grepFor(args.required), env) };
  } catch (err) {
    return { error: `no Playwright report: ${err.message}` };
  }
}

/** Runs the check for `argv`; the exit code. */
export function main(argv, env) {
  const args = readArgs(argv);
  if (args.error) {
    console.error(`${args.error}\n${USAGE}`);
    return 2;
  }
  const { report, error } = loadReport(args, env);
  if (error) {
    console.error(error);
    return 1;
  }
  const result = checkReport(report, args.required);
  console.log(`${formatTable(result)}\n${formatVerdict(result)}`);
  return result.problems.length === 0 ? 0 : 1;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? '').href) {
  process.exitCode = main(process.argv.slice(2), process.env);
}

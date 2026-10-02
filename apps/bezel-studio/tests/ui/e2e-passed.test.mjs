// The proof that named e2e tests passed (scripts/e2e-passed.mjs), on small
// synthetic Playwright JSON reports: every named test must have, in every
// project the report declares, a run that passed and recorded the axe
// check; a skipped run (or one in Playwright's parked state), a flaky,
// failed or missing run, a run without axe, another test that failed in the
// run, or projects that leave out light or dark in pt-BR or en make it fail,
// naming why. A --report file outside the app folder and the temporary
// directory is refused.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  AXE, OK, PARKED, REPORT_FOLDERS, checkReport, formatTable, formatVerdict, grepFor, projectsOf, readArgs, reportFile,
  runsByName, testName, uncovered, verdictOf,
} from '../../scripts/e2e-passed.mjs';

/** The app folder, which a relative --report file is taken from. */
const APP = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const TEMPORARY = resolve(tmpdir());
/** A path that climbs with `../` from the app folder up to the root, then into /etc. */
const CLIMB = `${'../'.repeat(APP.split(sep).length)}etc/report.json`;

const PROJECTS = ['light-pt', 'dark-pt', 'light-en', 'dark-en'];
/** Each project's colour scheme and locale, as playwright.config.mjs puts them in its metadata. */
const SETTINGS = {
  'light-pt': { colorScheme: 'light', locale: 'pt-BR' },
  'dark-pt': { colorScheme: 'dark', locale: 'pt-BR' },
  'light-en': { colorScheme: 'light', locale: 'en-US' },
  'dark-en': { colorScheme: 'dark', locale: 'en-US' },
  'light-gb': { colorScheme: 'light', locale: 'en-GB' },
  'dark-pt-pt': { colorScheme: 'dark', locale: 'pt-PT' },
};
const axe = { type: AXE, description: '40 rules passed, no serious or critical violation' };

/** A run in `projectName` that passed with the axe check, with `changes`. */
const run = (projectName, changes = {}) => ({
  projectName,
  annotations: [],
  expectedStatus: 'passed',
  status: 'expected',
  results: [{ status: 'passed', annotations: [axe] }],
  ...changes,
});

/** A spec titled `title` that passed in every project, but for `changed` (project → changes). */
const spec = (title, changed = {}, projects = PROJECTS) => ({
  title,
  tests: projects.map((project) => run(project, changed[project])),
});

/** A report of one file with one describe, `gif search`, holding `specs`. */
const report = (specs, projects = PROJECTS) => ({
  config: { projects: projects.map((name) => ({ name, metadata: SETTINGS[name] ?? {} })) },
  suites: [{ title: 'gif-search.spec.mjs', specs: [], suites: [{ title: 'gif search', specs }] }],
});

const HELP = 'gif search › no key: help';
const RATE = 'gif search › 429: 100 per hour';
const both = (changed = {}) => report([spec('no key: help'), spec('429: 100 per hour', changed)]);

test('all passed with axe in every project: ok, no problem', () => {
  const result = checkReport(both(), [HELP, RATE]);
  assert.deepEqual(result.projects, PROJECTS);
  assert.deepEqual(result.rows, [
    { name: HELP, cells: [OK, OK, OK, OK] },
    { name: RATE, cells: [OK, OK, OK, OK] },
  ]);
  assert.deepEqual(result.problems, []);
  assert.equal(formatVerdict(result), 'ok: 2 tests × 4 projects, 8/8 runs passed with axe');
});

test('one project skipped: fails, naming the test and the project', () => {
  const skipped = { status: 'skipped', expectedStatus: 'skipped', results: [{ status: 'skipped', annotations: [] }] };
  const result = checkReport(both({ 'dark-en': skipped }), [HELP, RATE]);
  assert.deepEqual(result.rows[1].cells, [OK, OK, OK, 'skipped']);
  assert.deepEqual(result.problems, [`${RATE} [dark-en]: skipped`]);
});

test('a test parked by test.fixme: fails in every project, even with another test passing beside it', () => {
  const parked = { annotations: [{ type: 'fixme' }], status: 'skipped', expectedStatus: 'skipped', results: [] };
  const changed = Object.fromEntries(PROJECTS.map((project) => [project, parked]));
  const result = checkReport(report([spec('no key: help'), spec('429: 100 per hour', changed), spec('unrelated')]), [HELP, RATE]);
  assert.deepEqual(result.rows[1].cells, ['fixme', 'fixme', 'fixme', 'fixme']);
  assert.deepEqual(result.problems, PROJECTS.map((project) => `${RATE} [${project}]: ${PARKED}`));
});

test('a run without the axe annotation: fails', () => {
  const result = checkReport(both({ 'light-pt': { results: [{ status: 'passed', annotations: [{ type: 'issue' }] }] } }), [HELP, RATE]);
  assert.deepEqual(result.problems, [`${RATE} [light-pt]: no axe`]);
});

test('a named test the report lacks: fails in every project', () => {
  const missing = 'gif collection › add, rename, delete';
  const result = checkReport(both(), [HELP, missing]);
  assert.deepEqual(result.rows[1].cells, ['missing', 'missing', 'missing', 'missing']);
  assert.deepEqual(result.problems, PROJECTS.map((project) => `${missing} [${project}]: missing`));
});

test('a project without a run of the test: missing there', () => {
  const result = checkReport(report([spec('no key: help', {}, ['light-pt', 'dark-pt', 'light-en'])]), [HELP]);
  assert.deepEqual(result.problems, [`${HELP} [dark-en]: missing`]);
});

test('the projects come from the report, whatever their number', () => {
  const five = [...PROJECTS, 'light-gb'];
  const result = checkReport(report([spec('no key: help', {}, five)], five), [HELP]);
  assert.deepEqual(projectsOf(report([], five)), five);
  assert.deepEqual(result.rows, [{ name: HELP, cells: [OK, OK, OK, OK, OK] }]);
  assert.deepEqual(result.problems, []);
  assert.deepEqual(checkReport({ suites: [] }, [HELP]).problems, [
    'the report declares no project',
    'no project runs in light × pt-BR',
    'no project runs in dark × pt-BR',
    'no project runs in light × en',
    'no project runs in dark × en',
  ]);
});

test('a report of one project: fails, naming each colour scheme × locale it lacks', () => {
  const one = ['light-pt'];
  const result = checkReport(report([spec('no key: help', {}, one)], one), [HELP]);
  assert.deepEqual(result.rows, [{ name: HELP, cells: [OK] }]);
  assert.deepEqual(result.problems, [
    'no project runs in dark × pt-BR',
    'no project runs in light × en',
    'no project runs in dark × en',
  ]);
  assert.match(formatVerdict(result), /^FAIL: 3 problem\(s\)/);
});

test('without the dark projects: fails, naming dark in each locale', () => {
  const light = ['light-pt', 'light-en'];
  const result = checkReport(report([spec('no key: help', {}, light)], light), [HELP]);
  assert.deepEqual(result.problems, ['no project runs in dark × pt-BR', 'no project runs in dark × en']);
});

test('uncovered: all four combinations are ok; a region counts for its language only', () => {
  assert.deepEqual(uncovered(report([])), []);
  assert.deepEqual(uncovered(report([], ['light-pt', 'dark-pt', 'light-gb', 'dark-en'])), []);
  assert.deepEqual(uncovered(report([], ['light-pt', 'dark-pt-pt', 'light-en', 'dark-en'])), ['dark × pt-BR']);
  // Names prove nothing: a project without settings covers nothing.
  assert.equal(uncovered({ config: { projects: PROJECTS.map((name) => ({ name })) } }).length, 4);
});

test('another test that failed in the run fails the check too', () => {
  const failed = { status: 'unexpected', results: [{ status: 'failed', annotations: [] }] };
  const result = checkReport(report([spec('no key: help'), spec('other', { 'dark-pt': failed })]), [HELP]);
  assert.deepEqual(result.problems, ['gif search › other [dark-pt]: failed (not named, but in the run)']);
  assert.match(formatVerdict(result), /^FAIL: 1 problem\(s\)\n {2}- gif search › other/);
});

test('verdictOf: only a passed run, expected to pass, with axe, is ok', () => {
  assert.equal(verdictOf(run('p')), OK);
  assert.equal(verdictOf(run('p', { status: 'flaky' })), 'flaky');
  assert.equal(verdictOf(run('p', { status: 'unexpected', results: [{ status: 'failed', annotations: [axe] }] })), 'failed');
  // test.fail(): it failed as expected, which proves nothing.
  assert.equal(verdictOf(run('p', { expectedStatus: 'failed', results: [{ status: 'failed', annotations: [axe] }] })), 'failed');
  assert.equal(verdictOf(run('p', { results: [] })), 'failed');
  // An older reporter keeps runtime annotations on the test only.
  assert.equal(verdictOf(run('p', { annotations: [axe], results: [{ status: 'passed' }] })), OK);
});

test('runsByName: describes and title, without the file; tests at the top of a file too', () => {
  const runs = runsByName({
    suites: [{ title: 'a.spec.mjs', specs: [spec('top', {}, ['p'])], suites: [{ title: 'outer', specs: [], suites: [{ title: 'inner', specs: [spec('deep', {}, ['p'])] }] }] }],
  });
  assert.deepEqual([...runs.keys()], ['top', 'outer › inner › deep']);
  assert.deepEqual(runsByName({}), new Map());
});

test('testName and readArgs: names as Playwright prints them; options; errors', () => {
  assert.equal(testName('gif search›no key: help'), HELP);
  assert.equal(testName('  gif search  ›  no key: help '), HELP);
  assert.deepEqual(readArgs(['--report', 'r.json', HELP, 'gif search › no key: help']), {
    report: join(APP, 'r.json'),
    required: [HELP],
  });
  assert.deepEqual(readArgs(['--grep=gif', HELP]), { grep: 'gif', required: [HELP] });
  assert.deepEqual(readArgs(['--report', 'r.json']), { error: 'no test named' });
  assert.match(readArgs(['--nope', HELP]).error, /nope/);
});

test('reportFile: in the app folder or the temporary directory only; a `../` out of them is refused', () => {
  assert.deepEqual(REPORT_FOLDERS, [TEMPORARY, APP]);
  assert.equal(reportFile('r.json'), join(APP, 'r.json'));
  assert.equal(reportFile('test-results/../r.json'), join(APP, 'r.json'));
  assert.equal(reportFile(join(TEMPORARY, 'e2e', 'r.json')), join(TEMPORARY, 'e2e', 'r.json'));
  assert.equal(reportFile(CLIMB), null);
  assert.equal(reportFile(join(TEMPORARY, 'e2e', '..', '..', 'etc', 'r.json')), null);
  // A folder whose name merely starts like an allowed one is outside it.
  assert.equal(reportFile(`${TEMPORARY}-other${sep}r.json`), null);
  assert.equal(reportFile(TEMPORARY), null);
  assert.deepEqual(readArgs(['--report', CLIMB, HELP]), { error: `--report ${CLIMB}: not in ${TEMPORARY} or ${APP}` });
});

test('grepFor: each outer describe once, matched literally', () => {
  assert.equal(grepFor([HELP, RATE, 'gif collection › add, rename, delete']), 'gif search|gif collection');
  assert.equal(grepFor(['a.b (c) › x', 'top']), String.raw`a\.b \(c\)|top`);
});

test('formatTable: a header with the projects, a row per test, aligned', () => {
  const table = formatTable(checkReport(both({ 'dark-pt': { status: 'flaky' } }), [HELP, RATE]));
  assert.equal(
    table,
    [
      'test                            light-pt  dark-pt  light-en  dark-en',
      'gif search › no key: help       ok        ok       ok        ok',
      'gif search › 429: 100 per hour  ok        flaky    ok        ok',
    ].join('\n'),
  );
});

test('the command on a report file: exit 0 and the table; 1 naming the problem; 2 without a test or out of its folders', () => {
  const dir = mkdtempSync(join(tmpdir(), 'e2e-passed-'));
  const script = fileURLToPath(new URL('../../scripts/e2e-passed.mjs', import.meta.url));
  const cli = (file, ...names) => spawnSync(process.execPath, [script, '--report', file, ...names], { encoding: 'utf8' });
  try {
    const good = join(dir, 'good.json');
    writeFileSync(good, JSON.stringify(both()));
    const ok = cli(good, HELP, RATE);
    assert.equal(ok.status, 0, ok.stderr);
    assert.match(ok.stdout, /ok: 2 tests × 4 projects, 8\/8 runs passed with axe/);

    const bad = join(dir, 'bad.json');
    writeFileSync(bad, JSON.stringify(both({ 'light-en': { status: 'skipped' } })));
    const failed = cli(bad, HELP, RATE);
    assert.equal(failed.status, 1);
    assert.match(failed.stdout, /429: 100 per hour \[light-en\]: skipped/);

    const single = join(dir, 'single.json');
    writeFileSync(single, JSON.stringify(report([spec('no key: help', {}, ['dark-en'])], ['dark-en'])));
    const narrow = cli(single, HELP);
    assert.equal(narrow.status, 1);
    assert.match(narrow.stdout, /no project runs in light × pt-BR/);

    assert.equal(cli(join(dir, 'none.json'), HELP).status, 1);
    assert.equal(cli(good).status, 2);

    const refused = cli(CLIMB, HELP);
    assert.equal(refused.status, 2);
    assert.equal(refused.stdout, '');
    assert.match(refused.stderr, /^--report (\.\.\/)+etc\/report\.json: not in .+ or .+\nusage: /);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

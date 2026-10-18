import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { performance } from 'node:perf_hooks';
import { runCycle } from '../agent-bootstrap/strategy-run.mjs';
import { configurations, material, caseTitle, exampleTransfer } from './case.mjs';
import { grade, hiringProfile } from './grade.mjs';

const here = dirname(fileURLToPath(import.meta.url));
export function planFor(competence) {
  const task = (id, depends_on) => ({ id, depends_on, kind: 'tool', tool: 'test_' + id, instruction: 'Execute the labelled local TEST task.' });
  return [task('requirements', []), ...(competence ? [task('client', [])] : []), task('implement', ['requirements']),
    ...(competence ? [task('review', ['client', 'implement'])] : []), task('deliver', [competence ? 'review' : 'implement'])];
}

// Isolated TEST owner API adapter, not a production arena endpoint or registration.
async function execute(configuration, directory) {
  const plan = planFor(configuration.competence), id = createHash('sha256').update('work-test-' + configuration.id).digest('hex');
  const record = 'a'.repeat(64), version = 'b'.repeat(64), events = [];
  const run = { id, record, version, status: 'running', deadline_at: Math.floor(Date.now() / 1000) + 120,
    calls: {}, known_tokens: 0, budget: 100, brief: { facts: ['TEST'], required_terms: ['TEST'], cta: 'TEST', work_context: material(configuration.context) } };
  const snapshot = () => structuredClone({ versions: [{ id: version, content: { plan } }], runs: [run] });
  const api = { record, get: async () => snapshot(), request: async path => { assert.equal(path, '/strategy-library'); return { versions: [] }; },
    post: async (path, body) => {
      if (path.endsWith('/reserve')) {
        assert.equal(run.status, 'running');
        const spec = plan.find(t => t.id === body.task); assert.ok(spec);
        assert.ok(spec.depends_on.every(d => run.calls[d]?.quality_passed && run.calls[d]?.usage));
        assert.equal(body.source, 'local_tool'); assert.equal(body.max_total_tokens, 0); assert.ok(!run.calls[body.task]);
        run.calls[body.task] = { reserved: 0, usage: null, quality_passed: null, source: body.source, provider: body.provider, model: body.model };
      } else if (path.endsWith('/report')) {
        const call = run.calls[body.task]; assert.ok(call); assert.equal(body.usage?.total_tokens, 0);
        assert.ok(body.report && !body.request_failed);
        Object.assign(call, { usage: body.usage, report: body.report, quality_passed: true });
        // Protocol receipt only. Independent domain grading happens after delivery.
      } else if (path.endsWith('/finish')) {
        assert.ok(plan.every(t => run.calls[t.id]?.quality_passed && run.calls[t.id]?.usage)); run.status = 'completed';
      } else throw new Error('unexpected_test_route');
      return { run: structuredClone(run) };
    } };
  const journalPath = join(directory, configuration.id + '.json');
  const tools = Object.fromEntries(plan.map(t => [t.tool, { argv: [process.execPath, join(here, 'worker.mjs')], no_model_calls: true }]));
  const provider = { complete: () => { throw new Error('model_calls_forbidden_in_test'); } };
  const start = performance.now();
  const options = { api, id, provider, journalPath, tools, onProgress: event => events.push({ ...event, at_ms: Math.round(performance.now() - start) }) };
  await runCycle(options);
  const starts = events.filter(e => e.status === 'started').length;
  await runCycle(options); assert.equal(events.filter(e => e.status === 'started').length, starts, 'completed journal must not repeat work');
  const policy = run.calls.deliver.report.policy, checks = grade(policy);
  return { ...configuration, status: run.status, execution: 'scripted_TEST_tools', plan, events,
    tool_calls: starts, model_calls: 0, model_tokens: run.known_tokens, journal_resume_repeated_calls: 0,
    wall_ms: Math.round(performance.now() - start), policy, checks,
    passed: checks.filter(c => c.passed).length, total: checks.length,
    held_out_passed: checks.filter(c => c.held_out && c.passed).length, held_out_total: checks.filter(c => c.held_out).length,
    task_reports: Object.fromEntries(Object.entries(run.calls).map(([task, call]) => [task, call.report])) };
}

export async function runExperiment() {
  const directory = mkdtempSync(join(tmpdir(), 'alashi-work-TEST-'));
  try {
    const runs = [];
    for (const configuration of configurations) runs.push(await execute(configuration, directory));
    const sources = ['case.mjs', 'worker.mjs', 'grade.mjs', 'experiment.mjs', '../agent-bootstrap/strategy-run.mjs'];
    return { schema: 'alashi.work-simulation.TEST.v1', test_data: true, generated_at: new Date().toISOString(), title: caseTitle,
      execution: 'Actual local tool subprocesses through the existing alashi runCycle scheduler; isolated TEST API adapter.',
      real_people_tested: 0, external_model_calls: 0, company_data: false, human_cost: 'not_measured',
      sources: Object.fromEntries(sources.map(p => [p, createHash('sha256').update(readFileSync(resolve(here, p))).digest('hex')])),
      runs, profile: hiringProfile(runs), knowledge_probe: exampleTransfer() };
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const args = process.argv.slice(2); assert.ok(args.length === 0 || args.length === 2 && args[0] === '--out', 'usage: node experiment.mjs [--out FILE]');
  const report = await runExperiment();
  if (args.length) writeFileSync(resolve(args[1]), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ test_data: true, results: report.runs.map(r => ({ id: r.id, passed: r.passed, total: r.total, held_out: r.held_out_passed, calls: r.tool_calls })), profile: report.profile, biological_moat: report.knowledge_probe.hypothesis_status }));
}

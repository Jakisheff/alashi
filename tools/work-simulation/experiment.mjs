import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { performance } from 'node:perf_hooks';
import { runCycle } from '../agent-bootstrap/strategy-run.mjs';
import { configurations, scenarios, material, caseTitle, exampleTransfer } from './case.mjs';
import { grade, hiringProfile, mean } from './grade.mjs';
import { gameBinary } from './game.mjs';
import { conditionsFor, validationResult } from './validation.mjs';

const here = dirname(fileURLToPath(import.meta.url));
export function planFor(competence) {
  const task = (id, depends_on) => ({ id, depends_on, kind: 'tool', tool: 'test_' + id, instruction: 'Execute the labelled local TEST task.' });
  return [task('requirements', []), ...(competence ? [task('inventory', [])] : []), task('implement', ['requirements']),
    ...(competence ? [task('review', ['inventory', 'implement'])] : []), task('deliver', [competence ? 'review' : 'implement'])];
}

// Isolated TEST owner API adapter, not a production arena endpoint or registration.
async function execute(configuration, directory, conditions = scenarios) {
  const plan = planFor(configuration.competence), id = createHash('sha256').update('work-test-' + configuration.id).digest('hex');
  const record = 'a'.repeat(64), version = 'b'.repeat(64), events = [];
  const run = { id, record, version, status: 'running', deadline_at: Math.floor(Date.now() / 1000) + 120,
    calls: {}, known_tokens: 0, budget: 100, brief: { facts: ['TEST'], required_terms: ['TEST'], cta: 'TEST', work_context: material(configuration.context, configuration.direct_rule) } };
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
  const setup_ms = Math.round(performance.now() - start);
  const policy = run.calls.deliver.report.policy, games = await grade(policy, conditions, configuration.direct_rule === true);
  return { ...configuration, status: run.status, execution: 'scripted_TEST_tools', plan, events,
    tool_calls: starts, model_calls: 0, model_tokens: run.known_tokens, journal_resume_repeated_calls: 0,
    setup_ms, wall_ms: Math.round(performance.now() - start), policy, games,
    mean_cash: mean(games.map(g => g.cash)), mean_payout: mean(games.map(g => g.payout)),
    mean_goods: mean(games.map(g => g.goods)), invalid_actions: games.reduce((n, g) => n + g.invalid_actions, 0),
    task_reports: Object.fromEntries(Object.entries(run.calls).map(([task, call]) => [task, call.report])) };
}

export async function runExperiment() {
  const directory = mkdtempSync(join(tmpdir(), 'alashi-work-TEST-'));
  try {
    // Read and hash the fixed protocol before any game; never choose conditions from results.
    const protocolBytes = readFileSync(resolve(here, '../../experiments/work-simulation/protocol.json'));
    const protocol = JSON.parse(protocolBytes), conditions = conditionsFor(protocol), runs = [];
    for (const configuration of configurations) runs.push(await execute(configuration, directory));
    const validationRuns = [];
    for (const run of runs) {
      const frozen = JSON.stringify(run.policy);
      validationRuns.push({ id: run.id, context: run.context, competence: run.competence, policy: run.policy,
        tool_calls: run.tool_calls, setup_ms: run.setup_ms, games: await grade(run.policy, conditions, true) });
      assert.equal(JSON.stringify(run.policy), frozen, 'policy_changed_during_validation');
    }
    for (const context of [false, true]) validationRuns.push(await execute({ id: context ? 'F' : 'E', competence: false, context, direct_rule: true }, directory, conditions));
    const challengePolicy = { ...validationRuns.find(r => r.id === 'E').policy, defer_market_round: 2 };
    const validation = validationResult(validationRuns, { policy: challengePolicy, games: await grade(challengePolicy, conditions, true) }, protocol);
    const sources = ['case.mjs', 'worker.mjs', 'grade.mjs', 'game.mjs', 'experiment.mjs', 'validation.mjs', '../../experiments/work-simulation/protocol.json', '../agent-bootstrap/strategy-run.mjs', '../../arena/src/bin/evalgame.rs', '../../arena/src/runner.rs', '../../arena/src/strategies.rs', '../../rules/src/actions.rs', '../../rules/src/transitions.rs', '../../rules/src/logic.rs', '../../rules/src/constants.rs'];
    return { schema: 'alashi.work-simulation.TEST.v3', test_data: true, generated_at: new Date().toISOString(), title: caseTitle,
      execution: 'Local TEST task graphs through runCycle, then 174 full six-round games through the existing Rust evalgame and alashi-rules. No live arena.',
      native_binary_sha256: createHash('sha256').update(readFileSync(gameBinary())).digest('hex'),
      experiment: { games_per_configuration: 12, rounds: 6, factions: 3, epoch: 0, opponents: ['random', 'random'], paired_by: ['seed', 'seat', 'opponents', 'epoch', 'vote_weight_mode'], seeds_are: 'fixed_TEST_not_held_out' },
      real_people_tested: 0, external_model_calls: 0, company_data: false, human_cost: 'not_measured',
      sources: Object.fromEntries(sources.map(p => [p, createHash('sha256').update(readFileSync(resolve(here, p))).digest('hex')])),
      runs, profile: hiringProfile(runs), knowledge_probe: exampleTransfer(),
      validation: { ...validation, protocol, protocol_sha256: createHash('sha256').update(protocolBytes).digest('hex'),
        policy_sha256: Object.fromEntries(validationRuns.map(r => [r.id, createHash('sha256').update(JSON.stringify(r.policy)).digest('hex')])) } };
  } finally { rmSync(directory, { recursive: true, force: true }); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const args = process.argv.slice(2); assert.ok(args.length === 0 || args.length === 2 && args[0] === '--out', 'usage: node experiment.mjs [--out FILE]');
  const report = await runExperiment();
  if (args.length) writeFileSync(resolve(args[1]), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ test_data: true, results: report.runs.map(r => ({ id: r.id, mean_cash: r.mean_cash, mean_payout: r.mean_payout, mean_goods: r.mean_goods, invalid_actions: r.invalid_actions, calls: r.tool_calls })), profile: report.profile, biological_moat: report.knowledge_probe.hypothesis_status }));
}

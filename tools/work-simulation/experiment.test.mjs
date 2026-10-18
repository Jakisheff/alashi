import { test } from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { runExperiment, planFor } from './experiment.mjs';
import { decide, observationFor, scenarios, exampleTransfer } from './case.mjs';
import { play } from './game.mjs';
import { hiringProfile } from './grade.mjs';
import { workServer } from './server.mjs';
import { conditionsFor, validationResult } from './validation.mjs';

let report;
test('four TEST controllers and frozen controls execute 174 native games with matched conditions', async () => {
  report = await runExperiment();
  assert.equal(report.schema, 'alashi.work-simulation.TEST.v3');
  assert.equal(report.external_model_calls, 0); assert.equal(report.real_people_tested, 0);
  for (const run of report.runs) {
    assert.equal(run.status, 'completed'); assert.equal(run.journal_resume_repeated_calls, 0);
    assert.equal(run.tool_calls, run.plan.length); assert.equal(run.model_tokens, 0);
    assert.deepEqual(run.games.map(g => g.key), scenarios.map(s => s.key));
    assert.equal(run.invalid_actions, 0);
    for (const game of run.games) {
      assert.equal(game.participants.length, 3);
      assert.deepEqual(game.participants[game.seat].actions, game.actions);
      assert.equal(game.participants[game.seat].payout, game.payout);
      assert.deepEqual(game.initial, report.runs[0].games.find(g => g.key === game.key).initial);
      assert.equal(game.produced, 5); assert.equal(game.sales, 5);
      assert.equal(game.payouts.reduce((a, b) => a + b, 0) + game.rake, game.bank);
      assert.equal(game.actions.at(-1).round, 6);
    }
    for (const task of run.plan) {
      const start = run.events.findIndex(e => e.task === task.id && e.status === 'started');
      assert.ok(start >= 0);
      for (const dependency of task.depends_on) assert.ok(run.events.findIndex(e => e.task === dependency && e.status === 'running') < start);
    }
  }
  assert.ok(report.runs.filter(r => r.competence).every(r => r.games.every(g => g.goods === 0)));
  assert.ok(report.runs.filter(r => !r.competence).every(r => r.mean_goods > 0));
});

test('native receipts distinguish inventory planning from public context and payout from treasury', () => {
  const comparisons = report.profile.comparisons;
  assert.equal(comparisons.length, 4);
  assert.ok(comparisons.filter(c => c.factor === 'competence').every(c => c.mean_cash_delta > 0));
  assert.ok(comparisons.filter(c => c.factor === 'context').every(c => c.mean_payout_delta === 0));
  const sale = report.runs[2].games[0].actions.find(a => a.action === 'sell');
  assert.equal(sale.detail.units, 2); assert.equal(sale.detail.revenue, 26_000_000);
  assert.equal(report.profile.human_transfer, 'not_validated'); assert.equal(report.profile.human_cost, 'not_measured');
  assert.deepEqual(hiringProfile(report.runs.map((r, i) => ({ ...r, id: String(i + 7) }))).skills, report.profile.skills);
});

test('context masking changes visible information, not the game state or faction resources', () => {
  const o = { round: 2, my_idx: 0, n_factions: 3, cash: [50, 80, 90], goods: [4, 1, 2], decision_stage: 'primary', law_card: 2, president: 'a', my_wallet: 'a', veto_pending: false };
  assert.equal(observationFor({}, 'law', o).law_card, undefined);
  assert.equal(observationFor({ public_context: true }, 'law', o).law_card, 2);
  assert.deepEqual(decide({}, 'law', o), { action: 'vote_yes' });
  assert.deepEqual(decide({ public_context: true }, 'law', o), { action: 'vote_no' });
  assert.deepEqual(decide({ inventory_review: true }, 'market', o), { action: 'sell', units: 4 });
  assert.deepEqual(o.goods, [4, 1, 2]);
});

test('replaying a paired game reproduces native actions and settlement exactly', async () => {
  const old = report.runs[3].games[0], next = await play(report.runs[3].policy, scenarios[0]);
  assert.deepEqual(next.actions, old.actions); assert.deepEqual(next.payouts, old.payouts);
  assert.equal(next.cash, old.cash); assert.deepEqual(next.decisions, old.decisions);
});

test('native rules reject an oversized sale and an illegal controller cannot earn a profile', async () => {
  const game = await play({ sale_units: 65535 }, scenarios[0]);
  assert.ok(game.invalid_actions > 0); assert.ok(game.actions.some(a => a.err === 'NotEnoughGoods'));
  const runs = structuredClone(report.runs); runs[3].games[0].invalid_actions = 1;
  assert.equal(hiringProfile(runs).status, 'invalid_actions'); assert.deepEqual(hiringProfile(runs).skills, []);
});

test('missing, duplicate or mismatched game pairs cannot manufacture a profile', () => {
  assert.equal(hiringProfile(report.runs.slice(1)).status, 'insufficient_data');
  const runs = structuredClone(report.runs); runs[3].games[0].seed = 999;
  assert.equal(hiringProfile(runs).status, 'insufficient_data');
  runs[3].games = runs[0].games.map(() => runs[0].games[0]);
  assert.equal(hiringProfile(runs).status, 'insufficient_data');
  const unfair = structuredClone(report.runs); unfair[3].games[0].initial.cash += 1;
  assert.equal(hiringProfile(unfair).status, 'insufficient_data');
});

test('higher treasury alone and worse payout cannot manufacture a positive competency claim', () => {
  const runs = structuredClone(report.runs);
  for (const run of runs) run.games = structuredClone(runs[0].games);
  for (const run of runs.filter(r => r.competence)) for (const game of run.games) game.cash += 100_000_000;
  assert.deepEqual(hiringProfile(runs).skills, []);
  for (const game of runs[3].games) game.payout -= 1;
  assert.equal(hiringProfile(runs).status, 'no_competence_advantage');
});

test('new conditions test role ablation with full traces, unchanged context and measured setup calls', () => {
  const v = report.validation, conditions = conditionsFor(v.protocol);
  assert.equal(conditions.length, 18); assert.equal(v.runs.length, 6); assert.equal(v.challenge.games.length, 18);
  assert.ok(conditions.every(c => ![11, 29, 47, 83, 109, 251].includes(c.seed)));
  assert.equal(new Set(conditions.map(c => c.family)).size, 3);
  assert.equal(v.role_rule_pairs, 36); assert.equal(v.exact_matches, 36); assert.equal(v.status, 'rule_preserves_role');
  assert.equal(v.cost.role_tool_calls, 5); assert.equal(v.cost.rule_tool_calls, 3); assert.equal(v.cost.saved_calls_per_policy_setup, 2);
  for (const run of v.runs) {
    assert.ok(run.games.every(g => g.decisions.length > 0 && g.invalid_actions === 0));
    if (['E', 'F'].includes(run.id)) {
      assert.equal(run.policy.inventory_review, false); assert.equal(run.policy.direct_sale_rule, true);
      assert.ok(run.plan.every(t => !['inventory', 'review'].includes(t.id)));
    }
  }
  assert.ok(v.pairs.every(p => p.same_decisions && p.same_actions && p.same_settlement));
  assert.equal(v.mechanism.same_observation, true);
  assert.equal(v.mechanism.before.action.units, 1); assert.equal(v.mechanism.after.action.units, 2);
  assert.ok(v.mechanism.rule_payout > v.mechanism.baseline_payout);
  assert.equal(v.summaries.reduce((n, s) => n + s.cases, 0), 36);
  assert.equal(v.challenge.payout_wins + v.challenge.payout_losses + v.challenge.payout_ties, 18);
  assert.ok(v.challenge.games.every(g => g.decisions.find(d => d.phase === 'market' && d.observation.round === 2).action.action === 'pass'));
  assert.equal(v.human_transfer, 'not_validated'); assert.equal(v.biological_moat, 'hypothesis');
  assert.ok(v.opponent_receipts.every(o => o.games === 6 && o.rejected_actions > 0));
  assert.ok(v.opponent_receipts.find(o => o.family === 'market_pressure').reasons.includes('bid_license: WrongPhase'));
});

test('decision differences, illegal controls and changed pair conditions reject role equivalence', () => {
  const v = report.validation;
  const challenge = { policy: v.challenge.policy, games: v.challenge.games };
  let runs = structuredClone(v.runs);
  runs[4].games[0].decisions[0].action = { action: 'sell', units: 99 };
  assert.equal(validationResult(runs, challenge, v.protocol).status, 'equivalence_rejected');
  runs = structuredClone(v.runs); runs[4].games[0].invalid_actions = 1;
  assert.equal(validationResult(runs, challenge, v.protocol).status, 'invalid_actions');
  runs = structuredClone(v.runs); runs[4].games[0].initial.cash += 1;
  assert.throws(() => validationResult(runs, challenge, v.protocol), /unequal_initial_resources/);
  runs = structuredClone(v.runs); runs[4].games[0] = structuredClone(runs[4].games[1]);
  assert.throws(() => validationResult(runs, challenge, v.protocol), /duplicate_validation_game/);
  runs = structuredClone(v.runs); runs[4].games.pop();
  assert.throws(() => validationResult(runs, challenge, v.protocol), /missing_validation_games/);
  runs = structuredClone(v.runs); runs[4].games[0].decisions = [];
  assert.throws(() => validationResult(runs, challenge, v.protocol), /missing_decision_trace/);
});

test('unspoken convention probe preserves ambiguity and is separate from match evidence', () => {
  const probe = exampleTransfer();
  assert.equal(probe.human_reference, 'not_collected'); assert.equal(probe.hypothesis_status, 'not_validated');
  assert.ok(probe.cases.every(c => !probe.demonstrations.some(d => d.id === c.id)));
  assert.equal(probe.cases.filter(c => c.expected !== null && c.expected === c.with_examples).length, 2);
  assert.equal(probe.cases.at(-1).with_examples, 'unknown');
  assert.deepEqual(planFor(true).find(t => t.id === 'review').depends_on, ['inventory', 'implement']);
});

test('local rerun rejects foreign origins, arbitrary payloads and overlapping experiments', async t => {
  let release;
  const server = workServer(async () => { await new Promise(r => { release = r; }); return { test_data: true }; });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  t.after(() => { server.closeAllConnections(); server.close(); });
  const url = `http://127.0.0.1:${server.address().port}/run`, origin = { Origin: 'http://127.0.0.1:5183' };
  assert.equal((await fetch(url)).status, 405);
  assert.equal((await fetch(url, { method: 'POST', headers: { Origin: 'https://example.com' } })).status, 403);
  assert.equal((await fetch(url, { method: 'POST', headers: origin, body: '{}' })).status, 400);
  const first = fetch(url, { method: 'POST', headers: origin });
  while (!release) await new Promise(r => setTimeout(r, 1));
  assert.equal((await fetch(url, { method: 'POST', headers: origin })).status, 409);
  release(); assert.equal((await first).status, 200);
});

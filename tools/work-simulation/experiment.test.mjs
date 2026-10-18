import { test } from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { runExperiment, planFor } from './experiment.mjs';
import { handler, exampleTransfer } from './case.mjs';
import { grade, hiringProfile } from './grade.mjs';
import { workServer } from './server.mjs';

let report;
test('four configurations execute the existing scheduler and independent acceptance suite', async () => {
  report = await runExperiment();
  assert.deepEqual(report.runs.map(r => r.passed), [4, 8, 8, 12]);
  assert.deepEqual(report.runs.map(r => r.held_out_passed), [1, 2, 2, 3]);
  assert.equal(report.external_model_calls, 0); assert.equal(report.real_people_tested, 0);
  for (const run of report.runs) {
    assert.equal(run.status, 'completed'); assert.equal(run.journal_resume_repeated_calls, 0);
    assert.equal(run.tool_calls, run.plan.length); assert.equal(run.model_tokens, 0);
    for (const task of run.plan) {
      assert.equal(run.events.filter(e => e.task === task.id && e.status === 'started').length, 1);
      const start = run.events.findIndex(e => e.task === task.id && e.status === 'started');
      for (const dependency of task.depends_on) assert.ok(run.events.findIndex(e => e.task === dependency && e.status === 'running') < start);
    }
  }
});

test('context and competence repair different checks with the other factor held fixed', () => {
  assert.ok(report);
  assert.deepEqual(report.profile.competence_gains, ['legacy', 'missing-user', 'unknown-state', 'blank-user']);
  assert.deepEqual(report.profile.context_gains, ['owned', 'delisted', 'grace', 'gift']);
  assert.equal(report.profile.human_transfer, 'not_validated');
  assert.equal(report.profile.human_cost, 'not_measured');
  const changedIds = report.runs.map((r, i) => ({ ...r, id: String(i + 7) }));
  assert.deepEqual(hiringProfile(changedIds).competence_gains, report.profile.competence_gains);
});

test('independent acceptance catches a broken policy; a role name cannot earn a pass', () => {
  assert.ok(grade({}).some(c => !c.passed));
  const policy = report.runs[3].policy;
  assert.ok(grade({ ...policy, retain_ownership: false }).find(c => c.id === 'gift').passed === false);
  assert.equal(handler(policy, { userId: '  ', subscription: 'active' }).status, 400);
  assert.equal(handler(policy, { userId: 'TEST', subscription: 'cancelled', purchased: true }).canRead, true);
});

test('no improvement or missing cells cannot manufacture a hiring profile', () => {
  const runs = structuredClone(report.runs);
  for (const r of runs) r.checks = structuredClone(runs[0].checks);
  assert.deepEqual(hiringProfile(runs).skills, []);
  assert.equal(hiringProfile(runs.slice(1)).status, 'insufficient_data');
});

test('regressions stop a positive competency recommendation', () => {
  const runs = structuredClone(report.runs);
  runs[3].checks.find(c => c.id === 'active').passed = false;
  const profile = hiringProfile(runs);
  assert.equal(profile.status, 'mixed_test_evidence'); assert.deepEqual(profile.skills, []);
});

test('example transfer uses new identities and preserves ambiguity without claiming human evidence', () => {
  const probe = exampleTransfer();
  assert.equal(probe.human_reference, 'not_collected'); assert.equal(probe.hypothesis_status, 'not_validated');
  assert.ok(probe.cases.every(c => !probe.demonstrations.some(d => d.id === c.id)));
  assert.equal(probe.cases.filter(c => c.expected !== null && c.expected === c.with_examples).length, 2);
  assert.equal(probe.cases.at(-1).with_examples, 'unknown');
});

test('competency changes the executed work graph, not the acceptance rubric', () => {
  assert.deepEqual(planFor(false).map(t => t.id), ['requirements', 'implement', 'deliver']);
  assert.deepEqual(planFor(true).find(t => t.id === 'review').depends_on, ['client', 'implement']);
  assert.ok(planFor(true).every(t => t.kind === 'tool'));
});

test('local rerun endpoint rejects foreign origins and arbitrary payloads; one experiment at a time', async t => {
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

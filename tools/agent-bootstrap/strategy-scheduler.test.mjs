import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { runCycle } from './strategy-run.mjs';

const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
const gate = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const task = (id, depends_on = []) => ({ id, depends_on, kind: 'model', instruction: id, tool: null });
const fork = () => [task('fast'), task('slow'), task('follow', ['fast']), task('deliver', ['follow', 'slow'])];
const result = id => ({ report: { task: id }, usage: { prompt_tokens: 8, completion_tokens: 2, total_tokens: 10, cached_tokens: 0 }, response_id: id, request_failed: false });

function fixture(t, plan = fork(), hooks = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'alashi-scheduler-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const id = 'a'.repeat(64), record = 'b'.repeat(64), version = 'c'.repeat(64);
  const run = { id, record, version, status: 'running', deadline_at: Math.floor(Date.now()/1000)+60,
    calls: {}, brief: { facts: [], required_terms: [], cta: 'test' }, known_tokens: 0, budget: 50000 };
  const events = [], counts = {}, inputs = {}, journalPath = join(dir, 'journal');
  const snapshot = () => structuredClone({ versions: [{ id: version, content: { plan } }], runs: [run] });
  const held = () => Object.values(run.calls).reduce((n, c) => n + (c.usage ? 0 : c.reserved), 0);
  const api = { record, get: async () => snapshot(), request: async () => snapshot(), post: async (path, body) => {
    if (path.endsWith('/reserve')) {
      await hooks.reserve?.(body, run);
      if (run.status !== 'running') throw new Error('run_stopped');
      const spec = plan.find(s => s.id === body.task);
      assert.ok(spec.depends_on.every(d => run.calls[d]?.quality_passed && run.calls[d]?.usage));
      if (run.known_tokens + held() + body.max_total_tokens > run.budget) throw new Error('budget_exhausted');
      assert.ok(!run.calls[body.task], 'reserve each task once');
      run.calls[body.task] = { reserved: body.max_total_tokens, status: 'reserved', usage: null,
        quality_passed: null, source: body.source, provider: body.provider, model: body.model };
      events.push(body.task + ':reserved');
    } else if (path.endsWith('/report')) {
      await hooks.beforeReport?.(body, run);
      const call = run.calls[body.task];
      assert.ok(call, 'report requires reservation');
      Object.assign(call, { usage: body.usage, report: body.report, status: 'reported', quality_passed: !body.request_failed && !!body.report });
      run.known_tokens = Object.values(run.calls).reduce((n, c) => n + (c.usage?.total_tokens ?? 0), 0);
      run.status = Object.values(run.calls).some(c => c.usage?.total_tokens > c.reserved) ? 'budget_overrun'
        : Object.values(run.calls).some(c => c.status === 'reported' && !c.usage) ? 'usage_unknown'
        : Object.values(run.calls).some(c => c.quality_passed === false) ? 'quality_failed' : 'running';
      events.push(body.task + ':reported');
      const receipt = { run: structuredClone(run) };
      await hooks.afterReport?.(body, run);
      return receipt;
    } else if (path.endsWith('/finish')) {
      assert.equal(run.status, 'running');
      assert.ok(plan.every(s => run.calls[s.id]?.quality_passed && run.calls[s.id]?.usage));
      run.status = 'completed'; events.push('finished');
    } else throw new Error('Unexpected mock route: ' + path);
    return { run: structuredClone(run) };
  } };
  const provider = { source: 'test_provider', provider: 'local', model: 'test', complete: async messages => {
    const input = JSON.parse(messages[1].content), id = input.task;
    assert.equal(run.calls[id]?.status, 'reserved', 'no model call without its reservation');
    const disk = JSON.parse(readFileSync(journalPath));
    assert.equal(disk.calls[id].phase, 'inflight', 'persist uncertainty before the call');
    counts[id] = (counts[id] ?? 0) + 1; inputs[id] = input;
    events.push(id + ':started');
    return hooks.complete ? await hooks.complete(id, input, run) : result(id);
  } };
  return { run, api, provider, events, counts, inputs, journalPath,
    start: () => runCycle({ api, id, provider, journalPath }),
    seedJournal: calls => writeFileSync(journalPath, JSON.stringify({ run: id, record, calls }), { mode: 0o600 }) };
}

test('a dependent starts after its accepted report while an unrelated task is still running', async t => {
  const slow = gate(), reported = gate(), followed = gate();
  const f = fixture(t, fork(), {
    complete: async id => { if (id === 'slow') await slow.promise; if (id === 'follow') followed.resolve(); return result(id); },
    afterReport: async body => { if (body.task === 'fast') reported.resolve(); },
  });
  const pending = f.start();
  await reported.promise;
  const early = await Promise.race([followed.promise.then(() => true), wait(100).then(() => false)]);
  slow.resolve(); await pending;
  assert.equal(early, true, 'follow must not wait for slow');
  assert.deepEqual(f.inputs.follow.dependency_reports, { fast: { task: 'fast' } });
  assert.deepEqual(f.counts, { fast: 1, slow: 1, follow: 1, deliver: 1 });
});

test('a rejected reservation cannot call the provider or finish the run', async t => {
  const f = fixture(t, [task('deliver')]); f.run.budget = 1;
  await assert.rejects(f.start());
  assert.deepEqual(f.counts, {}); assert.equal(f.run.known_tokens, 0);
  assert.ok(!f.events.includes('finished'));
});

for (const failure of ['unknown usage', 'invalid report']) test(`${failure} stops new work and drains active calls before releasing the journal lock`, async t => {
  const slow = gate(), bothStarted = gate(), unknown = gate(); let starts = 0;
  const f = fixture(t, fork(), {
    complete: async id => {
      if (++starts === 2) bothStarted.resolve();
      await bothStarted.promise;
      if (id === 'slow') { await slow.promise; return result(id); }
      return failure === 'unknown usage' ? { ...result(id), usage: null } : { ...result(id), report: null };
    },
    afterReport: async body => { if (body.task === 'fast') unknown.resolve(); },
  });
  let settled = false;
  const pending = f.start().then(() => { settled = true; }, error => { settled = true; return error; });
  await unknown.promise; await wait(20);
  assert.equal(settled, false); assert.equal(existsSync(f.journalPath + '.lock'), true);
  slow.resolve(); assert.ok(await pending instanceof Error);
  assert.deepEqual(f.counts, { fast: 1, slow: 1 });
  assert.equal(f.run.known_tokens, failure === 'unknown usage' ? 10 : 20);
  assert.equal(f.run.status, failure === 'unknown usage' ? 'usage_unknown' : 'quality_failed');
  assert.equal(existsSync(f.journalPath + '.lock'), false);
});

for (const accepted of [false, true]) test(`resume a lost report ${accepted ? 'after' : 'before'} server acceptance without repeating model work`, async t => {
  let dropped = false;
  const failOnce = async body => { if (body.task === 'fast' && !dropped) { dropped = true; throw new Error('lost_report'); } };
  const f = fixture(t, fork(), accepted ? { afterReport: failOnce } : { beforeReport: failOnce });
  await assert.rejects(f.start());
  await f.start();
  assert.deepEqual(f.counts, { fast: 1, slow: 1, follow: 1, deliver: 1 });
  assert.deepEqual(f.inputs.follow.dependency_reports, { fast: { task: 'fast' } });
  assert.equal(f.run.known_tokens, 40); assert.equal(f.run.status, 'completed');
});

test('a stopped run reconciles saved paid responses without starting more work', async t => {
  const f = fixture(t);
  f.run.status = 'usage_unknown';
  f.run.calls.fast = { reserved: 5000, status: 'reported', quality_passed: true, usage: null, source: 'test_provider', provider: 'local', model: 'test' };
  f.run.calls.slow = { reserved: 5000, status: 'reserved', quality_passed: null, usage: null, source: 'test_provider', provider: 'local', model: 'test' };
  f.seedJournal({ fast: { phase: 'inflight', result: { ...result('fast'), usage: null } }, slow: { phase: 'inflight', result: result('slow') } });
  await assert.rejects(f.start());
  assert.deepEqual(f.counts, {}); assert.equal(f.run.calls.slow.usage.total_tokens, 10);
  assert.equal(f.run.known_tokens, 10); assert.equal(f.run.status, 'usage_unknown');
});

test('out-of-order report snapshots cannot forget an accepted dependency or repeat a task', async t => {
  const fastReported = gate();
  const f = fixture(t, fork(), { afterReport: async body => {
    if (body.task === 'fast') { await wait(20); fastReported.resolve(); }
    if (body.task === 'slow') { await fastReported.promise; await wait(20); }
  } });
  await f.start();
  assert.deepEqual(f.counts, { fast: 1, slow: 1, follow: 1, deliver: 1 });
  assert.equal(f.run.known_tokens, 40);
});

test('an overrun with a valid report still blocks dependent model calls', async t => {
  const f = fixture(t, [task('first'), task('deliver', ['first'])], { complete: async (id, input, run) => {
    const used = run.calls[id].reserved + 1;
    return { ...result(id), usage: { prompt_tokens: used, completion_tokens: 0, total_tokens: used, cached_tokens: 0 } };
  } });
  await assert.rejects(f.start());
  assert.deepEqual(f.counts, { first: 1 }); assert.equal(f.run.status, 'budget_overrun');
});

test('a reservation that completes after a failure cannot start a model call', async t => {
  const failed = gate();
  const f = fixture(t, [task('bad'), task('late'), task('deliver', ['bad', 'late'])], { reserve: async body => {
    if (body.task === 'bad') { failed.resolve(); throw new Error('budget_exhausted'); }
    if (body.task === 'late') { await failed.promise; await wait(20); }
  } });
  await assert.rejects(f.start()); assert.deepEqual(f.counts, {});
});

test('expiry during reservation prevents a model request', async t => {
  const f = fixture(t, [task('deliver')], { reserve: async () => { await wait(1100); } });
  f.run.deadline_at = Math.floor(Date.now()/1000)+1;
  await assert.rejects(f.start()); assert.deepEqual(f.counts, {});
});

test('a newly ready task cannot spend budget held by an unrelated active call', async t => {
  const slow = gate(), attempted = gate(); let activeReservationSeen = false;
  const f = fixture(t, fork(), {
    complete: async id => { if (id === 'slow') await slow.promise; return result(id); },
    reserve: async (body, run) => {
      if (body.task === 'follow') {
        activeReservationSeen = run.calls.slow.status === 'reserved';
        run.budget = run.known_tokens + run.calls.slow.reserved;
        attempted.resolve();
      }
    },
  });
  const pending = f.start().catch(error => error);
  const reached = await Promise.race([attempted.promise.then(() => true), wait(1000).then(() => false)]);
  await wait(20); slow.resolve();
  assert.ok(await pending instanceof Error); assert.equal(reached, true);
  assert.equal(activeReservationSeen, true); assert.deepEqual(f.counts, { fast: 1, slow: 1 });
  assert.equal(f.run.known_tokens, 20);
});

test('a saved response is reconciled after the deadline without another model call', async t => {
  const f = fixture(t, [task('deliver')]); f.run.deadline_at = 1;
  f.run.calls.deliver = { reserved: 5000, status: 'reserved', usage: null, quality_passed: null,
    source: 'test_provider', provider: 'local', model: 'test' };
  f.seedJournal({ deliver: { phase: 'inflight', result: result('deliver') } });
  await f.start(); assert.deepEqual(f.counts, {}); assert.equal(f.run.known_tokens, 10);
});

test('a prepared reservation resumes once without reserving or spending twice', async t => {
  const f = fixture(t, [task('deliver')]);
  f.run.calls.deliver = { reserved: 5000, status: 'reserved', usage: null, quality_passed: null,
    source: 'test_provider', provider: 'local', model: 'test' };
  f.seedJournal({ deliver: { phase: 'prepared' } });
  await f.start(); assert.deepEqual(f.counts, { deliver: 1 });
  assert.ok(!f.events.includes('deliver:reserved')); assert.equal(f.run.known_tokens, 10);
});

test('fixed workload timing: same reports and four calls, with controlled provider delays', async t => {
  const times = [];
  for (let n = 0; n < 3; n++) {
    const f = fixture(t, fork(), { complete: async id => { await wait({ fast: 30, slow: 200, follow: 150, deliver: 10 }[id]); return result(id); } });
    const start = performance.now(); await f.start(); times.push(Math.round(performance.now() - start));
    assert.deepEqual(f.counts, { fast: 1, slow: 1, follow: 1, deliver: 1 });
    assert.deepEqual(f.run.calls.deliver.report, { task: 'deliver' }); assert.equal(f.run.known_tokens, 40);
  }
  t.diagnostic(JSON.stringify({ fixture: 'fast30-slow200-follow150-deliver10', milliseconds: times,
    median_ms: [...times].sort((a,b) => a-b)[1], provider_calls_per_run: 4, synthetic_tokens_per_run: 40,
    external_model_calls: 0 }));
});

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { runComparison, modelProvider } from './strategy-run.mjs';

const usage = { prompt_tokens: 8, completion_tokens: 2, total_tokens: 10, cached_tokens: 0 };
const gate = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const result = report => ({ report, usage, response_id: 'local-mock-response', request_failed: false });

function fixture(t, hooks = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'alashi-comparison-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const id = '1'.repeat(64), attemptId = '2'.repeat(64), record = '3'.repeat(64), wallet = 'owner-wallet';
  const profile = { source: 'test_provider', provider: 'http://127.0.0.1', model: 'TEST-local', temperature: 0, max_tokens: 256 };
  const brief = { owner: wallet, facts: ['ALASHI supports experiments.'], audience: 'Agent owners', required_terms: ['ALASHI'], cta: 'Explore ALASHI', max_words: 60, budget: 50000 };
  const versions = ['baseline', 'candidate'].map((label, i) => ({ id: String(i+4).repeat(64), content: { owner: wallet, plan: [
    { id: 'facts', kind: 'tool', tool: 'brief_extract', depends_on: [], instruction: 'Read the brief.' },
    { id: 'research', kind: 'model', tool: null, depends_on: ['facts'], instruction: label+' research' },
    { id: 'deliver', kind: 'model', tool: null, depends_on: ['research'], instruction: 'Write the ad.' },
  ] } }));
  const attempt = { id: attemptId, number: 1, client_id: 'attempt-01' }, deadline = Math.floor(Date.now()/1000)+180;
  for (let i = 0; i < 2; i++) attempt[['baseline', 'candidate'][i]] = {
    id: String(i+6).repeat(64), record, owner: wallet, version: versions[i].id, label: ['baseline', 'candidate'][i],
    challenge: '8'.repeat(64), brief: structuredClone(brief), deadline_at: deadline,
    budget: brief.budget, status: 'running', calls: {}, known_tokens: 0,
  };
  const comparison = { id, content: { owner: wallet, record, profile, baseline_version: versions[0].id,
    candidate_version: versions[1].id, challenge_id: attempt.baseline.challenge, attempt_limit: 1 }, attempts: [attempt] };
  const events = [], counts = {}, inputs = {}, journalPrefix = join(dir, 'comparison');
  let attemptPosts = 0, created = false;
  const snapshot = () => structuredClone({ ok: true, comparisons: [comparison] });
  const api = { record, wallet, get: async () => structuredClone({ versions }), request: async path => {
    if(path === `/agents/${record}/owner/comparisons`) return snapshot();
    assert.equal(path, '/strategy-library'); return { versions: [] };
  }, post: async (path, body) => {
    if(path === `/comparisons/${id}/attempts`) {
      attemptPosts++; assert.equal(body.client_id, 'attempt-01');
      if(!created) { created = true; events.push('attempt-created'); }
      return structuredClone({ ok: true, comparison, attempt });
    }
    const match = path.match(new RegExp(`^/comparisons/${id}/attempts/${attemptId}/(baseline|candidate)/(reserve|report|finish)$`));
    assert.ok(match, 'only nested comparison actions are allowed');
    const [, label, action] = match, run = attempt[label], plan = versions[label === 'baseline' ? 0 : 1].content.plan;
    if(action === 'reserve') {
      const existing=run.calls[body.task];
      if(existing) {
        if(!body.execution_id||body.execution_id!==existing.execution_id)throw new Error('execution_conflict');
        assert.equal(body.max_total_tokens,existing.reserved);
        return {ok:true,run:structuredClone(run)};
      }
      assert.match(body.execution_id,/^[0-9a-f]{64}$/);
      const spec = plan.find(task => task.id === body.task);
      assert.ok(spec.depends_on.every(task => run.calls[task]?.quality_passed));
      assert.ok(!run.calls[body.task], 'no duplicate reservation');
      run.calls[body.task] = { ...body, reserved: body.max_total_tokens, usage: null, quality_passed: null, status: 'reserved',execution_claimed:true };
      events.push(`${label}:${body.task}:reserved`);
    } else if(action === 'report') {
      await hooks.beforeReport?.(label, body, run);
      assert.ok(run.calls[body.task], 'reports require reservations');
      assert.equal(body.execution_id,run.calls[body.task].execution_id);
      Object.assign(run.calls[body.task], body, { quality_passed: !body.request_failed && !!body.report, status: 'reported' });
      run.known_tokens = Object.values(run.calls).reduce((sum, call) => sum + (call.usage?.total_tokens ?? 0), 0);
      if(!body.usage || body.request_failed || !body.report) run.status = 'quality_failed';
      events.push(`${label}:${body.task}:reported`);
      await hooks.afterReport?.(label, body, run);
    } else {
      assert.ok(plan.every(task => run.calls[task.id]?.quality_passed && run.calls[task.id]?.usage));
      run.status = 'completed'; run.result = run.calls.deliver.report;
      events.push(`${label}:finished`);
    }
    return { ok: true, run: structuredClone(run) };
  } };
  const provider = { ...profile, complete: async messages => {
    const input = JSON.parse(messages[1].content), task = input.task;
    const label = task === 'research' ? (messages[0].content.endsWith('baseline research') ? 'baseline' : 'candidate') : input.dependency_reports.research.label;
    const run = attempt[label], journalPath = `${journalPrefix}.${label}.json`;
    assert.equal(run.calls[task]?.status, 'reserved', 'reserve before any model work');
    assert.equal(JSON.parse(readFileSync(journalPath)).calls[task].phase, 'inflight', 'save uncertainty before model work');
    counts[label] = (counts[label] ?? 0) + 1;
    (inputs[label] ??= []).push(input); events.push(`${label}:${task}:started`);
    await hooks.complete?.(label, task, input);
    return result(task === 'research' ? { label } : { headline: 'ALASHI '+label, body: 'A bounded experiment.', cta: brief.cta });
  } };
  return { comparison, attempt, provider, versions, counts, events, inputs, journalPrefix,
    get attemptPosts() { return attemptPosts; },
    start: () => runComparison({ api, id, clientId: 'attempt-01', provider, journalPrefix }),
  };
}

test('own baseline and candidate run concurrently with independent reports and private journals', { timeout: 5000 }, async t => {
  const both = gate(); let researchStarted = 0;
  const f = fixture(t, { complete: async (_label, task) => {
    if(task === 'research') { if(++researchStarted === 2) both.resolve(); await both.promise; }
  } });
  const done = await f.start();
  assert.equal(researchStarted, 2);
  assert.deepEqual(f.counts, { baseline: 2, candidate: 2 });
  assert.equal(done.baseline.result.headline, 'ALASHI baseline');
  assert.equal(done.candidate.result.headline, 'ALASHI candidate');
  assert.deepEqual(f.inputs.baseline[1].dependency_reports, { research: { label: 'baseline' } });
  assert.deepEqual(f.inputs.candidate[1].dependency_reports, { research: { label: 'candidate' } });
  assert.deepEqual(f.inputs.baseline[0].brief, f.inputs.candidate[0].brief);
  for(const label of ['baseline', 'candidate']) {
    const path = `${f.journalPrefix}.${label}.json`, journal = JSON.parse(readFileSync(path));
    assert.equal(journal.run, done[label].id); assert.equal(journal.calls.facts.result.usage.total_tokens, 0);
    assert.equal(existsSync(path+'.lock'), false);
  }
  assert.equal(existsSync(f.journalPrefix+'.comparison.lock'), false);
});

for(const change of ['model', 'temperature', 'max_tokens', 'source']) test(`wrong comparison ${change} is rejected before attempt creation or model work`, async t => {
  const f = fixture(t); f.comparison.content.profile[change] = change === 'model' ? 'other' : change === 'source' ? 'provider_reported' : 1;
  await assert.rejects(f.start(), /comparison_provider_mismatch/);
  assert.equal(f.attemptPosts, 0); assert.deepEqual(f.counts, {});
});

test('custom executables are rejected before creating or running either half', async t => {
  const f = fixture(t); f.versions[1].content.plan[0].tool = 'owner-custom-command';
  await assert.rejects(f.start(), /comparison_tool_not_allowed/);
  assert.equal(f.attemptPosts, 0); assert.deepEqual(f.counts, {});
});

test('a different candidate brief stops both halves before reservations', async t => {
  const f = fixture(t); f.attempt.candidate.brief.facts.push('Candidate-only fact.');
  await assert.rejects(f.start(), /comparison_conditions_mismatch/);
  assert.deepEqual(f.counts, {});
  assert.ok(!f.events.some(event => event.endsWith(':reserved')));
});

test('a mismatched second journal is rejected before the first half starts', async t => {
  const f = fixture(t);
  writeFileSync(f.journalPrefix+'.candidate.json', JSON.stringify({ run: '9'.repeat(64), record: f.attempt.candidate.record, calls: {} }), { mode: 0o600 });
  await assert.rejects(f.start(), /journal_scope_mismatch/);
  assert.deepEqual(f.counts, {});
  assert.ok(!f.events.some(event => event.endsWith(':reserved')));
});

for(const accepted of [false, true]) test(`recovery after a lost report ${accepted ? 'after' : 'before'} acceptance never pays for either model response twice`, async t => {
  let dropped = false;
  const drop = async (label, body) => {
    if(label === 'baseline' && body.task === 'research' && !dropped) { dropped = true; throw new Error('lost_report'); }
  };
  const f = fixture(t, accepted ? { afterReport: drop } : { beforeReport: drop });
  await assert.rejects(f.start(), /lost_report/);
  assert.equal(f.attempt.candidate.status, 'completed', 'the other half drains before releasing the pair lock');
  await f.start();
  assert.deepEqual(f.counts, { baseline: 2, candidate: 2 });
  assert.equal(f.attemptPosts, 2);
  assert.equal(f.events.filter(event => event === 'attempt-created').length, 1);
  for(const label of ['baseline', 'candidate']) {
    assert.equal(f.attempt[label].known_tokens, 20);
    assert.equal(f.events.filter(event => event === `${label}:research:reserved`).length, 1);
  }
});

test('the real provider adapter sends the fixed comparison generation profile to a local fetch mock', async t => {
  let body;
  t.mock.method(globalThis, 'fetch', async (url, options) => {
    assert.equal(url, 'http://127.0.0.1:8799/completions');
    body = JSON.parse(options.body);
    return new Response(JSON.stringify({ id: 'mock', usage, choices: [{ message: { content: '{"ok":true}' } }] }), { status: 200 });
  });
  const provider = modelProvider({ url: 'http://127.0.0.1:8799/completions', model: 'TEST-local', test: true });
  const output = await provider.complete([{ role: 'user', content: 'local fixture' }]);
  assert.equal(output.usage.total_tokens, 10);
  assert.equal(body.temperature, provider.temperature); assert.equal(body.temperature, 0);
  assert.equal(body.max_tokens, provider.max_tokens); assert.equal(body.max_tokens, 256);
  assert.equal(body.model, provider.model);
});

test('concurrent journals for one attempt cannot duplicate provider work or alter the winning receipts', {timeout:5000},async t=>{
  const dir=mkdtempSync(join(tmpdir(),'alashi-comparison-claim-'));t.after(()=>rmSync(dir,{recursive:true,force:true}));
  const id='1'.repeat(64),aid='2'.repeat(64),record='3'.repeat(64),wallet='owner';
  const labels=['baseline','candidate'],profile={source:'test_provider',provider:'local',model:'mock',temperature:0,max_tokens:256};
  const brief={facts:[],required_terms:[],cta:'Inspect',budget:50000};
  const versions=labels.map((label,i)=>({id:String(i+4).repeat(64),content:{owner:wallet,plan:[{id:'deliver',kind:'model',tool:null,depends_on:[],instruction:label}]}}));
  const attempt={id:aid,client_id:'attempt-01'};
  for(let i=0;i<2;i++)attempt[labels[i]]={id:String(i+6).repeat(64),record,owner:wallet,version:versions[i].id,challenge:'8'.repeat(64),brief,status:'running',deadline_at:Math.floor(Date.now()/1000)+180,budget:50000,calls:{},known_tokens:0};
  const comparison={id,content:{owner:wallet,record,profile,baseline_version:versions[0].id,candidate_version:versions[1].id,challenge_id:attempt.baseline.challenge},attempts:[attempt]};
  const allSnapshots=gate();let lists=0,starts=0,calls=0,conflicts=0;
  const claims=new Map();
  const api={record,wallet,get:async()=>structuredClone({versions}),request:async path=>{
    if(path==='/strategy-library')return {versions:[]};
    const value=structuredClone({comparisons:[comparison]}),n=++lists;
    // Both processes see the same unreserved baseline and candidate before racing.
    if(n>=3&&n<=6){if(n===6)allSnapshots.resolve();await allSnapshots.promise;}
    return value;
  },post:async(path,body)=>{
    if(path===`/comparisons/${id}/attempts`){starts++;return structuredClone({attempt});}
    const match=path.match(/\/(baseline|candidate)\/(reserve|report|finish)$/);assert.ok(match);
    const [,label,action]=match,run=attempt[label],key=label+':'+body.task;
    if(action==='reserve'){
      assert.match(body.execution_id,/^[0-9a-f]{64}$/);
      if(claims.has(key)&&claims.get(key)!==body.execution_id){conflicts++;throw new Error('execution_conflict');}
      if(!claims.has(key)){
        claims.set(key,body.execution_id);
        run.calls[body.task]={reserved:body.max_total_tokens,source:body.source,provider:body.provider,model:body.model,status:'reserved',usage:null,quality_passed:null,execution_claimed:true};
      }
    }else if(action==='report'){
      assert.equal(body.execution_id,claims.get(key));
      const old=run.calls[body.task];
      if(old.status!=='reserved')assert.equal(old.response_id,body.response_id);
      else {Object.assign(old,{usage:body.usage,report:body.report,response_id:body.response_id,quality_passed:true,status:'reported'});run.known_tokens=body.usage.total_tokens;}
    }else{assert.equal(run.calls.deliver.quality_passed,true);run.status='completed';}
    return structuredClone({run});
  }};
  const provider={...profile,complete:async()=>({report:{answer:'bounded'},usage,response_id:'response-'+(++calls),request_failed:false})};
  const outcomes=await Promise.allSettled(['first','second'].map(name=>runComparison({api,id,clientId:'attempt-01',provider,journalPrefix:join(dir,name)})));
  assert.equal(starts,2);assert.equal(claims.size,2);assert.equal(calls,2);assert.equal(conflicts,2);
  assert.equal(outcomes.filter(value=>value.status==='fulfilled').length,1);
  assert.equal(outcomes.find(value=>value.status==='rejected').reason.message,'execution_conflict');
  assert.equal(attempt.baseline.known_tokens+attempt.candidate.known_tokens,20);
  assert.ok(labels.every(label=>attempt[label].status==='completed'));
});

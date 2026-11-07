import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, chmodSync, lstatSync, existsSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { initExperiment, freezeExperiment, runExperiment, decideExperiment, applyExperiment, exportExperiment,
  exampleManifest, validateManifest, artifactChecks } from './local-experiment.mjs';

const SECRET = 'PRIVATE_SOURCE_SENTINEL_72ea';
const usage = { prompt_tokens: 8, completion_tokens: 2, total_tokens: 10, cached_tokens: 0 };
const artifact = { headline: 'Acme offers a weekly workshop', body: 'Bring your team to review ideas together in the weekly workshop.', cta: 'Explore Acme' };
const read = path => JSON.parse(readFileSync(path, 'utf8'));
const save = (path, value) => writeFileSync(path, JSON.stringify(value), { mode: 0o600 });
const receipt = () => ({ report: artifact, usage: { ...usage }, response_id: 'test-response', request_failed: false });
const card = () => ({ title: 'Source fact first', mechanism: 'Move a source fact into the headline.', task: 'Review a static advertisement draft.',
  conditions: 'One instruction changed; the brief and generation settings stayed fixed.', result: 'The owner preferred the candidate in this local example.',
  limitations: 'This does not measure sales or establish repeatable performance.' });

function fixture(t, configure = () => {}) {
  const root = mkdtempSync(join(tmpdir(), 'alashi-private-experiment-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const dir = join(root, 'experiment'); initExperiment(dir);
  const path = join(dir, 'manifest.json'), manifest = read(path);
  manifest.profile = { url: 'http://127.0.0.1:12345/chat', model: 'local-test', source: 'test_provider', temperature: 0, max_tokens: 256 };
  manifest.brief.facts.push(SECRET); manifest.baseline[1].instruction += ` ${SECRET}`; manifest.candidate[1].instruction += ` ${SECRET}`;
  configure(manifest); save(path, manifest);
  const calls = [];
  const provider = { source: 'test_provider', provider: 'http://127.0.0.1:12345', model: 'local-test', temperature: 0, max_tokens: 256,
    complete: async messages => { calls.push(messages); return receipt(); } };
  return { root, dir, manifest, calls, provider, freeze: () => freezeExperiment(dir),
    run: extra => runExperiment({ dir, provider, allowProvider: true, ...extra }), state: () => read(join(dir, 'state.json')) };
}

test('freeze binds private brief, plans and question with a random salt', async t => {
  const f = fixture(t), frozen = await f.freeze();
  assert.match(frozen.digest, /^[0-9a-f]{64}$/); assert.match(frozen.salt, /^[0-9a-f]{64}$/);
  assert.deepEqual(await f.freeze(), frozen);
  const other = fixture(t); assert.notEqual((await other.freeze()).digest, frozen.digest);
  f.manifest.prediction += ' changed after freeze'; save(join(f.dir, 'manifest.json'), f.manifest);
  await assert.rejects(f.run(), /frozen_manifest_changed/); assert.equal(f.calls.length, 0);
});

test('rejects a second changed instruction, changed topology, remote mock and arbitrary executable', () => {
  for (const mutate of [
    m => { m.candidate[0].instruction += ' changed'; },
    m => { m.candidate[1].depends_on = []; },
    m => { m.profile.source = 'test_provider'; },
    m => { m.baseline[0].tool = m.candidate[0].tool = 'shell'; },
    m => { m.candidate[1].instruction = m.baseline[1].instruction; },
    m => { m.profile.api_key = 'not-permitted'; },
    m => { m.profile.url = 'https://alashi.network/chat'; },
    m => { m.profile.url = 'https://api.alashi.network/chat'; },
  ]) { const manifest = exampleManifest(); mutate(manifest); assert.throws(() => validateManifest(manifest)); }
});

test('requires explicit provider consent and a matching frozen provider before any request', async t => {
  const f = fixture(t); await f.freeze();
  await assert.rejects(f.run({ allowProvider: false }), /require_allow_provider_yes/);
  await assert.rejects(f.run({ provider: { ...f.provider, model: 'other' } }), /local_provider_mismatch/);
  assert.equal(f.calls.length, 0); assert.equal(existsSync(join(f.dir, 'state.json')), false);
});

test('runs custom private brief locally and resumes without another provider call', async t => {
  const f = fixture(t); await f.freeze();
  const result = await f.run();
  assert.equal(result.status, 'ready_for_owner_decision'); assert.equal(result.alashi_requests, 0); assert.equal(result.cost_usd, null);
  assert.equal(f.calls.length, 2); assert.ok(f.calls.every(messages => JSON.stringify(messages).includes(SECRET)));
  assert.ok(result.runs.every(run => run.known_tokens === 10));
  assert.deepEqual(await f.run(), result); assert.equal(f.calls.length, 2);
  for (const name of ['manifest.json', 'frozen.json', 'state.json', 'baseline.journal.json', 'candidate.journal.json'])
    assert.equal(lstatSync(join(f.dir, name)).mode & 0o777, 0o600);
  assert.equal(lstatSync(f.dir).mode & 0o777, 0o700);
});

test('unknown usage or provider failure permits only inconclusive and never repeats requests', async t => {
  for (const throws of [false, true]) {
    const f = fixture(t); await f.freeze();
    f.provider.complete = async messages => { f.calls.push(messages); if (throws) throw new Error('provider connection lost'); return { ...receipt(), usage: null }; };
    assert.equal((await f.run()).status, 'inconclusive'); assert.equal(f.calls.length, 2);
    await f.run(); assert.equal(f.calls.length, 2);
    await assert.rejects(decideExperiment(f.dir, 'adopt_candidate', 'Looks good'), /incomplete_evidence_requires_inconclusive/);
    const result = await decideExperiment(f.dir, 'inconclusive', 'Usage could not be verified.');
    assert.equal(result.decision.choice, 'inconclusive');
    await assert.rejects(applyExperiment(f.dir), /completed_owner_choice_required/);
  }
});

test('a saved receipt is reconciled without reissuing a model call', async t => {
  const f = fixture(t); await f.freeze(); await f.run();
  const state = f.state();
  const saved = state.runs[0].calls.deliver;
  state.runs[0].status = 'running'; state.runs[0].result = null;
  state.runs[0].known_tokens = 0;
  state.runs[0].calls.deliver = { ...saved, status: 'reserved', usage: null, quality_passed: null, report: null };
  delete state.runs[0].calls.deliver.receipt;
  save(join(f.dir, 'state.json'), state);
  assert.equal((await f.run()).status, 'ready_for_owner_decision');
  assert.equal(f.calls.length, 2); assert.equal(f.state().runs[0].known_tokens, 10);
});

test('an inflight journal without a saved response is unknown and is not retried', async t => {
  const f = fixture(t); await f.freeze(); await f.run();
  const state = f.state(), run = state.runs[0], old = run.calls.deliver;
  run.status = 'running'; run.known_tokens = 0; run.result = null;
  run.calls.deliver = { ...old, status: 'reserved', usage: null, quality_passed: null, report: null }; delete run.calls.deliver.receipt;
  save(join(f.dir, 'state.json'), state);
  const path = join(f.dir, 'baseline.journal.json'), journal = read(path);
  delete journal.calls.deliver.result; journal.calls.deliver.phase = 'inflight'; save(path, journal);
  assert.equal((await f.run()).status, 'inconclusive'); assert.equal(f.calls.length, 2);
  assert.equal(f.state().runs[0].status, 'usage_unknown');
});

test('reservation budget is checked before provider work and incomplete result cannot be adopted', async t => {
  const f = fixture(t, m => { m.budget = 1; }); await f.freeze();
  assert.equal((await f.run()).status, 'inconclusive'); assert.equal(f.calls.length, 0);
  await assert.rejects(decideExperiment(f.dir, 'adopt_candidate', 'Try it'), /incomplete_evidence_requires_inconclusive/);
  assert.equal((await decideExperiment(f.dir, 'inconclusive', 'The frozen budget was too small.')).decision.choice, 'inconclusive');
});

test('a concurrent process cannot run the same experiment or change its decision', async t => {
  const f = fixture(t); await f.freeze();
  let release, entered;
  const gate = new Promise(resolve => { release = resolve; });
  const started = new Promise(resolve => { entered = resolve; });
  f.provider.complete = async messages => { f.calls.push(messages); entered(); await gate; return receipt(); };
  const pending = f.run(); await started;
  await assert.rejects(f.run(), /experiment_busy/);
  await assert.rejects(decideExperiment(f.dir, 'inconclusive', 'Too early'), /experiment_busy/);
  release(); await pending;
  assert.equal(existsSync(join(f.dir, '.experiment.lock')), false); assert.equal(f.calls.length, 2);
});

test('decision is immutable and apply writes the actual selected plan for the next experiment', async t => {
  const f = fixture(t); await f.freeze(); await f.run();
  const decision = await decideExperiment(f.dir, 'adopt_candidate', 'Its headline states the supplied fact.');
  assert.deepEqual(await decideExperiment(f.dir, 'adopt_candidate', 'Its headline states the supplied fact.'), decision);
  await assert.rejects(decideExperiment(f.dir, 'keep_baseline', 'Changed my mind'), /decision_immutable/);
  await assert.rejects(f.run(), /decision_already_recorded/);
  const applied = await applyExperiment(f.dir);
  assert.deepEqual(read(applied.active_strategy).plan, f.manifest.candidate);
  assert.deepEqual(await applyExperiment(f.dir), applied);
  const next = join(f.root, 'next'); initExperiment(next, applied.active_strategy);
  assert.deepEqual(read(join(next, 'manifest.json')).baseline, f.manifest.candidate);
});

test('public export contains only reviewed text, stable retry ID and explicit CC0 consent', async t => {
  const f = fixture(t); await f.freeze(); await f.run(); await decideExperiment(f.dir, 'adopt_candidate', SECRET);
  const input = join(f.root, 'reviewed-card.json'), output = join(f.root, 'public-card.json'); save(input, card());
  await assert.rejects(exportExperiment(f.dir, input, output, false), /explicit_consent/); assert.equal(existsSync(output), false);
  const exported = await exportExperiment(f.dir, input, output, true), publicCard = read(output);
  assert.equal(exported.published, false); assert.equal(publicCard.license, 'CC0-1.0'); assert.equal(publicCard.consent, true);
  assert.equal(Object.keys(publicCard).length, 9); assert.equal(JSON.stringify(publicCard).includes(SECRET), false);
  assert.equal(Object.hasOwn(publicCard, 'evidence_status'), false); assert.equal(Object.hasOwn(publicCard, 'digest'), false);
  assert.deepEqual(await exportExperiment(f.dir, input, output, true), exported);
  save(input, { ...card(), result: 'A different result' });
  await assert.rejects(exportExperiment(f.dir, input, output, true), /export_intent_immutable/);
});

test('export refuses arbitrary fields and private-state overwrite', async t => {
  const f = fixture(t); await f.freeze(); await f.run(); await decideExperiment(f.dir, 'keep_baseline', 'Keep the current draft.');
  const input = join(f.root, 'card.json'); save(input, { ...card(), raw_report: SECRET });
  await assert.rejects(exportExperiment(f.dir, input, join(f.root, 'out.json'), true), /invalid_public_card_fields/);
  save(input, card());
  const original = readFileSync(join(f.dir, 'state.json'), 'utf8');
  await assert.rejects(exportExperiment(f.dir, input, join(f.dir, 'manifest.json'), true), /export_path_conflicts/);
  const alias = join(f.root, 'directory-alias'); symlinkSync(f.dir, alias);
  await assert.rejects(exportExperiment(f.dir, input, join(alias, 'state.json'), true), /export_path_conflicts/);
  await assert.rejects(exportExperiment(f.dir, input, input, true), /separate_output/);
  const target = join(f.root, 'output-alias.json'); symlinkSync(join(f.dir, 'state.json'), target);
  await assert.rejects(exportExperiment(f.dir, input, target, true), /private_file_requires_mode_600/);
  assert.equal(read(join(f.dir, 'manifest.json')).engine, 'alashi-local-ad-v1');
  assert.ok(original.includes(SECRET));
});

test('negative usage, changed provider or false completed state is rejected before a request', async t => {
  const f = fixture(t); await f.freeze(); await f.run(); const original = f.state();
  for (const mutate of [
    state => { state.runs[0].calls.deliver.usage.prompt_tokens = -1; },
    state => { state.runs[0].calls.deliver.model = 'other-model'; },
    state => { state.runs[0].result.artifact.body = 'Unexpected changed body'; },
    state => { state.runs[0].result.checks.passed = false; },
    state => { state.runs[0].known_tokens = 0; },
  ]) {
    const changed = structuredClone(original); mutate(changed); save(join(f.dir, 'state.json'), changed);
    await assert.rejects(f.run(), /invalid_local_|invalid_completed_local_run/);
  }
  assert.equal(f.calls.length, 2);
});

test('export preserves another experiment state and arbitrary existing JSON byte for byte', async t => {
  const f = fixture(t), other = fixture(t);
  await f.freeze(); await f.run(); await decideExperiment(f.dir, 'keep_baseline', 'Keep the current draft.');
  await other.freeze(); await other.run();
  const input = join(f.root, 'card.json'); save(input, card());
  const unrelated = join(f.root, 'existing-private.json'); save(unrelated, { private: SECRET });
  for (const output of [join(other.dir, 'state.json'), unrelated]) {
    const before = readFileSync(output);
    await assert.rejects(exportExperiment(f.dir, input, output, true), /public_export_output_exists/);
    assert.deepEqual(readFileSync(output), before);
    assert.equal(f.state().public_export, null, 'a rejected output must not freeze a new export intent');
  }
  const output = join(f.root, 'new-public-card.json');
  const result = await exportExperiment(f.dir, input, output, true), beforeRetry = readFileSync(output);
  assert.deepEqual(await exportExperiment(f.dir, input, output, true), result);
  assert.deepEqual(readFileSync(output), beforeRetry);
});

test('a journal for another attempt cannot invoke the provider', async t => {
  const f = fixture(t); await f.freeze(); await f.run();
  const state = f.state(); state.runs[0].status = 'running'; state.runs[0].result = null;
  save(join(f.dir, 'state.json'), state);
  const path = join(f.dir, 'baseline.journal.json'), journal = read(path); journal.run = 'a'.repeat(64); save(path, journal);
  assert.equal((await f.run()).status, 'inconclusive'); assert.equal(f.calls.length, 2);
  assert.equal(f.state().runs[0].error, 'journal_scope_mismatch');
});

test('private file and directory permissions and symlink targets are enforced', async t => {
  const f = fixture(t); await f.freeze();
  chmodSync(join(f.dir, 'manifest.json'), 0o644);
  await assert.rejects(f.run(), /private_file_requires_mode_600/); assert.equal(f.calls.length, 0);
  chmodSync(join(f.dir, 'manifest.json'), 0o600); chmodSync(f.dir, 0o755);
  await assert.rejects(f.run(), /experiment_directory_requires_mode_700/);
  chmodSync(f.dir, 0o700);
  const alias = join(f.root, 'symlink'); symlinkSync(f.dir, alias);
  await assert.rejects(runExperiment({ dir: alias, provider: f.provider, allowProvider: true }), /experiment_directory_requires_mode_700/);
});

test('refuses a saved state from another manifest before any request', async t => {
  const f = fixture(t); await f.freeze(); await f.run();
  const state = f.state(); state.digest = 'a'.repeat(64); save(join(f.dir, 'state.json'), state);
  await assert.rejects(f.run(), /local_state_manifest_mismatch/); assert.equal(f.calls.length, 2);
});

test('format checks distinguish schema compliance from business effects', () => {
  const brief = exampleManifest().brief;
  assert.equal(artifactChecks(brief, artifact).passed, true);
  for (const invalid of [{ ...artifact, price: 2 }, { ...artifact, cta: 'Something else' }, { ...artifact, body: 'Guaranteed success' },
    { headline: 'Acme', body: 'workshop', cta: 'Explore Acme' }]) assert.equal(artifactChecks(brief, invalid).passed, false);
});

const cli = (args, env = {}) => new Promise((resolve, reject) => {
  const child = spawn(process.execPath, [new URL('./local-experiment.mjs', import.meta.url).pathname, ...args],
    { env: { ...process.env, ...env }, stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '', stderr = ''; child.stdout.on('data', data => { stdout += data; }); child.stderr.on('data', data => { stderr += data; });
  child.on('error', reject); child.on('close', code => resolve({ code, stdout, stderr }));
});

test('CLI end-to-end calls only the configured loopback provider, never ALASHI', async t => {
  const f = fixture(t); const received = [];
  const server = createServer(async (request, response) => {
    let body = ''; for await (const part of request) body += part;
    received.push({ url: request.url, body: JSON.parse(body), auth: request.headers.authorization });
    response.writeHead(200, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify({ id: 'mock-receipt', usage: { prompt_tokens: 8, completion_tokens: 2, total_tokens: 10 }, choices: [{ message: { content: JSON.stringify(artifact) } }] }));
  });
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  t.after(() => new Promise(resolve => server.close(resolve)));
  f.manifest.profile.url = `http://127.0.0.1:${server.address().port}/completion`; save(join(f.dir, 'manifest.json'), f.manifest);
  const guard = join(f.root, 'network-guard.mjs');
  writeFileSync(guard, `const original = globalThis.fetch;
globalThis.fetch = (url, options) => {
  if (String(url) !== ${JSON.stringify(f.manifest.profile.url)}) throw new Error('unexpected_network_destination');
  return original(url, options);
};\n`, { mode: 0o600 });
  assert.equal((await cli(['freeze', '--dir', f.dir])).code, 0);
  const denied = await cli(['run', '--dir', f.dir]); assert.equal(denied.code, 1); assert.equal(received.length, 0);
  const run = await cli(['run', '--dir', f.dir, '--allow-provider', 'yes'], { ALASHI_MODEL_KEY: 'inert-test-value', NODE_OPTIONS: `--import=${guard}` });
  assert.equal(run.code, 0, run.stderr); assert.equal(JSON.parse(run.stdout).status, 'ready_for_owner_decision');
  assert.equal(received.length, 2); assert.ok(received.every(r => r.url === '/completion' && r.auth === 'Bearer inert-test-value'));
  assert.ok(received.every(r => JSON.stringify(r.body).includes(SECRET)));
  assert.equal(readFileSync(join(f.dir, 'state.json'), 'utf8').includes('inert-test-value'), false);
  assert.equal((await cli(['decide', '--dir', f.dir, '--choice', 'adopt_candidate', '--reason', 'Source fact came first.'])).code, 0);
  assert.equal((await cli(['apply', '--dir', f.dir])).code, 0);
  assert.equal((await cli(['report', '--dir', f.dir])).code, 0);
  assert.equal(received.length, 2);
});

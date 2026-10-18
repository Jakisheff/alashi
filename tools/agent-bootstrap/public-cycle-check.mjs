// Actual installed CLI processes, HTTPS owner API and deterministic local TEST provider.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, openSync, closeSync, unlinkSync, lstatSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { performance } from 'node:perf_hooks';
import { setTimeout as pause } from 'node:timers/promises';

const [runtimeArg, directoryArg, hostPath, aPath, bPath, base = 'https://alashi.network'] = process.argv.slice(2);
assert.ok(runtimeArg && directoryArg && hostPath && aPath && bPath, 'usage: public-cycle-check.mjs RUNTIME PRIVATE_DIRECTORY HOST_PROFILE A_PROFILE B_PROFILE [API]');
assert.ok(base === 'https://alashi.network' || /^http:\/\/127\.0\.0\.1:\d+$/.test(base));
const runtime = resolve(runtimeArg), directory = resolve(directoryArg);
assert.equal(JSON.parse(readFileSync(join(runtime, 'version.json'))).source_commit, '523e14ac058a58ff25bc4786407fbb8bd60105fe');
const { ownerApi, readPrivate } = await import(pathToFileURL(join(runtime, 'strategy-run.mjs')));
const profiles = [hostPath, aPath, bPath].map(p => readPrivate(resolve(p)));
assert.equal(new Set(profiles.map(p => p.wallet)).size, 3, 'host and two competitors require distinct wallets');
mkdirSync(directory, { recursive: true, mode: 0o700 });
assert.equal(lstatSync(directory).mode & 0o077, 0, 'private directory must be mode 700');
const session = randomUUID().replaceAll('-', '').slice(0, 12);
writeFileSync(join(directory, 'session.json'), JSON.stringify({ session, base }), { mode: 0o600, flag: 'wx' });
const gate = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
async function bounded(promise, message) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message)), 30000); })]); }
  finally { clearTimeout(timer); }
}
const ad = { headline: 'ALASHI agents at work', body: 'Use your agents and tools in ALASHI. Compare reported tokens on a shared task.', cta: 'Explore ALASHI' };
const plan = [['fast', []], ['slow', []], ['follow', ['fast']], ['deliver', ['follow', 'slow']]].map(([id, depends_on]) => ({ id, depends_on, kind: 'model', instruction: id, tool: null }));
const children = new Set(), summary = { source_commit: '523e14ac058a58ff25bc4786407fbb8bd60105fe', api: base, external_provider_calls: 0, cases: [] };
// Each login uses two requests; public nginx allows twelve per minute per IP.
let lastLogin = -Infinity;
async function loginTurn() {
  if (base === 'https://alashi.network') await pause(Math.max(0, 11000 - (performance.now() - lastLogin)));
  lastLogin = performance.now();
}
let current;
const proxy = createServer(async (req, res) => {
  try {
    const parts = []; for await (const part of req) parts.push(part);
    const body = Buffer.concat(parts), headers = { Accept: 'application/json' };
    for (const name of ['authorization', 'origin', 'content-type']) if (req.headers[name]) headers[name] = req.headers[name];
    const response = await fetch(base + req.url, { method: req.method, headers, redirect: 'error', signal: AbortSignal.timeout(15000), ...(body.length ? { body } : {}) });
    const text = await response.text(), value = JSON.parse(text), input = body.length ? JSON.parse(body) : {};
    if (current && req.url.includes(current.runA.id)) {
      if (req.url.endsWith('/reserve') && value.error === 'budget_exhausted') { current.budgetDenied = true; current.events.push({ event: 'budget_rejected', task: input.task, at: performance.now() }); current.denied.resolve(); }
      if (req.url.endsWith('/report') && value.ok === true) {
        current.events.push({ event: 'accepted_report', task: input.task, at: performance.now() });
        if (current.kind === 'budget_recovery' && input.task === 'slow' && current.budgetDenied && !current.lost) {
          assert.ok(value.run.calls.slow.quality_passed && value.run.calls.slow.usage);
          current.lost = true; current.accepted.resolve();
          await current.drop.promise; res.destroy(); return;
        }
      }
    }
    if (!res.destroyed) { res.writeHead(response.status, { 'Content-Type': 'application/json' }); res.end(text); }
  } catch { if (!res.destroyed) { res.writeHead(502); res.end('{"ok":false,"error":"test_proxy_failed"}'); } }
});
const provider = createServer(async (req, res) => {
  try {
    let text = ''; for await (const part of req) text += part;
    const input = JSON.parse(JSON.parse(text).messages[1].content), owner = req.url === '/a' ? 'a' : 'b', task = input.task, c = current;
    const team = c[owner]; team.counts[task] = (team.counts[task] ?? 0) + 1;
    c.events.push({ event: 'model_start', owner, task, at: performance.now() });
    let total = 120;
    if (task === 'slow') {
      team.started.resolve();
      await bounded(owner === 'a' && c.kind === 'budget_recovery' ? c.denied.promise : team.followed.promise, 'continuation_did_not_unlock');
      team.slowEnded = true;
    }
    if (task === 'fast') {
      await bounded(team.started.promise, 'second_root_did_not_start');
      if (owner === 'a' && c.kind === 'budget_recovery') {
        const run = (await c.apiA.get()).runs.find(r => r.id === c.runA.id);
        total = run.budget - run.calls.slow.reserved - run.calls.fast.reserved + 1;
        assert.ok(total >= 20 && total <= run.calls.fast.reserved, 'fixture budget must cover roots and reject continuation while slow is reserved');
      }
    }
    if (task === 'follow' && !(owner === 'a' && c.kind === 'budget_recovery')) {
      assert.equal(team.slowEnded, false); team.followed.resolve();
    }
    c.events.push({ event: 'model_end', owner, task, at: performance.now() });
    const report = task === 'deliver' ? ad : { facts: ['ALASHI agents use tokens.'] };
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ id: randomUUID(), usage: { prompt_tokens: total - 20, completion_tokens: 20, total_tokens: total }, choices: [{ message: { content: JSON.stringify(report) } }] }));
  } catch (error) { res.writeHead(500, { 'Content-Type': 'application/json' }); res.end(JSON.stringify({ error: String(error.message) })); }
});
await new Promise(r => proxy.listen(0, '127.0.0.1', r));
await new Promise(r => provider.listen(0, '127.0.0.1', r));
const proxyBase = `http://127.0.0.1:${proxy.address().port}`;
function launch(owner, profile, run, kind) {
  const journal = join(directory, kind + '-' + owner + '.json'), fd = openSync(join(directory, kind + '-' + owner + '.log'), 'a', 0o600);
  const env = { ...process.env, ALASHI_MODEL_URL: `http://127.0.0.1:${provider.address().port}/${owner}`, ALASHI_MODEL: 'local-protocol-TEST' }; delete env.ALASHI_MODEL_KEY;
  const child = spawn(process.execPath, [join(runtime, 'strategy-run.mjs'), '--profile', resolve(profile), '--api', owner === 'a' ? proxyBase : base, '--run', run.id, '--journal', journal, '--allow-provider', 'yes', '--test-provider', 'yes'], { env, stdio: ['ignore', fd, fd] });
  closeSync(fd); children.add(child);
  const done = new Promise((yes, no) => { child.once('error', no); child.once('close', (code, signal) => { children.delete(child); yes({ code, signal }); }); });
  return { child, done, journal };
}
try {
  const apis = [];
  for (const profile of profiles) { await loginTurn(); apis.push(await ownerApi(base, profile)); }
  const [host, a, b] = apis;
  const versions = await Promise.all([a, b].map((api, i) => api.post('/strategies', { title: 'Private TEST plan ' + session.slice(0, 8) + '-' + i, plan, change: '', published: false })));
  // Keep actual CLI logins within the existing five-challenges-per-five-minutes limit.
  for (const kind of ['dependencies', 'budget_recovery']) {
    const challenge = (await host.post('/strategy-challenges', { match_key: session + '-' + kind, round_seconds: 180, title: 'Local runner TEST ' + kind, audience: 'Owners of AI agents', task: 'Write an English ad for ALASHI.', facts: ['Owners use their agents and tools.', 'Crystals represent model tokens.'], required_terms: ['ALASHI', 'agents', 'tokens'], cta: ad.cta, max_words: 80, budget: kind === 'budget_recovery' ? 8500 : 20000 })).challenge;
    const runA = (await a.post('/strategy-runs', { version: versions[0].version.id, challenge_id: challenge.id, client_id: session + '-' + kind + '-a' })).run;
    const runB = (await b.post('/strategy-runs', { version: versions[1].version.id, challenge_id: challenge.id, client_id: session + '-' + kind + '-b' })).run;
    const team = () => ({ counts: {}, started: gate(), followed: gate(), slowEnded: false });
    current = { kind, runA, runB, apiA: a, a: team(), b: team(), events: [], denied: gate(), accepted: gate(), drop: gate(), lost: false, budgetDenied: false };
    writeFileSync(join(directory, kind + '-runs.json'), JSON.stringify({ challenge: challenge.id, a: runA.id, b: runB.id }), { mode: 0o600, flag: 'wx' });
    await loginTurn(); let ca = launch('a', aPath, runA, kind);
    await loginTurn(); const cb = launch('b', bPath, runB, kind);
    if (kind === 'budget_recovery') {
      await bounded(current.accepted.promise, 'accepted_report_not_observed');
      assert.equal(current.budgetDenied, true);
      ca.child.kill('SIGKILL'); assert.equal((await bounded(ca.done, 'client_did_not_exit')).signal, 'SIGKILL');
      current.drop.resolve();
      assert.ok(lstatSync(ca.journal + '.lock').isFile()); unlinkSync(ca.journal + '.lock');
      assert.deepEqual(current.a.counts, { fast: 1, slow: 1 });
      await loginTurn(); ca = launch('a', aPath, runA, kind);
    }
    const exits = await Promise.all([bounded(ca.done, 'client_a_timeout'), bounded(cb.done, 'client_b_timeout')]);
    assert.ok(exits.every(e => e.code === 0), 'inspect private CLI logs');
    for (const owner of ['a', 'b']) assert.deepEqual(current[owner].counts, { fast: 1, slow: 1, follow: 1, deliver: 1 });
    const saved = await Promise.all([a, b].map(api => api.get()));
    for (const [i, run] of [runA, runB].entries()) {
      const receipt = saved[i].runs.find(r => r.id === run.id);
      assert.equal(receipt.status, 'completed'); assert.equal(receipt.reserved_tokens, 0); assert.equal(receipt.unknown_usage, false); assert.equal(receipt.test_provider, true);
      await host.post(`/strategy-runs/${run.id}/review`, { accepted: true, reason: 'Protocol TEST fixture acceptance only; no campaign efficacy or monetary claim.' });
    }
    summary.cases.push({ kind, passed: true, calls_a: current.a.counts, calls_b: current.b.counts, budget_rejected_before_continuation: current.budgetDenied, client_sigkill: kind === 'budget_recovery', events: current.events });
    writeFileSync(join(directory, 'result.json'), JSON.stringify(summary, null, 2), { mode: 0o600 });
  }
  console.log(JSON.stringify(summary));
} finally {
  current?.drop.resolve(); current?.denied.resolve(); current?.a.followed.resolve(); current?.b.followed.resolve();
  for (const child of children) child.kill('SIGTERM');
  proxy.closeAllConnections(); provider.closeAllConnections();
  await Promise.all([new Promise(r => proxy.close(r)), new Promise(r => provider.close(r))]);
}

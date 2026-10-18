// Isolated acceptance against the exact deployed arenad; deterministic TEST calls only.
import assert from 'node:assert/strict';
import { createPrivateKey, createPublicKey, createHash } from 'node:crypto';
import { mkdirSync, writeFileSync, readFileSync, openSync, closeSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { performance } from 'node:perf_hooks';
import { base58, ownerApi, runCycle } from './strategy-run.mjs';

const [binaryArg, directoryArg, portArg = '18595'] = process.argv.slice(2);
assert.ok(binaryArg && directoryArg, 'usage: deployment-check.mjs ARENAD PRIVATE_DIRECTORY [PORT]');
const binary = resolve(binaryArg), directory = resolve(directoryArg), port = Number(portArg);
assert.ok(Number.isInteger(port) && port > 1024 && port < 65536);
mkdirSync(directory, { recursive: true, mode: 0o700 });
const profiles = [1, 2, 3, 4].map((byte, i) => {
  const seed = Buffer.alloc(32, byte), key = createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]), format: 'der', type: 'pkcs8' });
  const pub = createPublicKey(key).export({ format: 'der', type: 'spki' }).subarray(-32);
  return { wallet: base58(pub), secret_key: [...seed, ...pub], agent_record_id: ['a', 'b', 'e', 'f'][i].repeat(64) };
});
const registrations = Object.fromEntries(profiles.map((p, i) => [p.agent_record_id, {
  wallet: p.wallet, owner_id: i.toString(16).padStart(64, '0'), character_id: p.agent_record_id,
  recovery_hash: 'e'.repeat(64), challenge: 'f'.repeat(64), created_at: 1,
  receipt: { mode: 'agent_lifecycle_v2', network: 'devnet', wallet: p.wallet, signature: 'offline-fixture', slot: 0, fee_lamports: '0', commitment: 'confirmed' },
}]));
writeFileSync(join(directory, 'state.json'), JSON.stringify({ v: 1, games: [], registrations, completed: [], master_seed: 1, next_id_hint: 1, proposal_key: '1'.repeat(64) }), { mode: 0o600, flag: 'wx' });
writeFileSync(join(directory, 'sequence'), '18\n', { mode: 0o600, flag: 'wx' });
const base = `http://127.0.0.1:${port}`, wait = ms => new Promise(r => setTimeout(r, ms));
let child;
async function stop(signal = 'SIGTERM') {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  const exited = once(child, 'exit'); child.kill(signal); await exited;
}
async function start() {
  const fd = openSync(join(directory, 'server.log'), 'a', 0o600);
  child = spawn(binary, ['--bind', '127.0.0.1', '--port', String(port), '--tick-ms', '60000'], {
    env: { ...process.env, ALASHI_STATE_FILE: join(directory, 'state.json'), ALASHI_SEQ_FILE: join(directory, 'sequence'), ALASHI_REQUIRE_EXISTING_STATE: '1', ALASHI_REQUIRE_PLATFORM_V2: '1', ALASHI_REQUIRE_DEVNET_REGISTRATION: '1', ALASHI_OWNER_ORIGIN: 'https://alashi.network' },
    stdio: ['ignore', fd, fd],
  }); closeSync(fd);
  for (let i = 0; i < 100; i++) {
    assert.equal(child.exitCode, null, 'isolated arenad exited; inspect server.log');
    try { if ((await fetch(base + '/games', { signal: AbortSignal.timeout(500) })).ok) return; } catch {}
    await wait(50);
  }
  throw new Error('isolated arenad did not become ready');
}
const ad = { headline: 'ALASHI agents at work', body: 'Use your agents and tools in ALASHI. Compare reported tokens on a shared task.', cta: 'Explore ALASHI' };
const plan = [['fast', []], ['slow', []], ['follow', ['fast']], ['deliver', ['follow', 'slow']]].map(([id, depends_on]) => ({ id, depends_on, kind: 'model', instruction: id, tool: null }));
const summary = { binary_sha256: createHash('sha256').update(readFileSync(binary)).digest('hex'), real_provider_calls: 0, cases: [] };
try {
  await start();
  const [host, a, b] = await Promise.all(profiles.slice(0, 3).map(p => ownerApi(base, p)));
  const version = (await a.post('/strategies', { title: 'Deployment fixture', plan, change: '', published: true })).version;
  const rival = (await b.post('/strategies', { title: 'Deployment rival fixture', plan, change: '', published: true })).version;
  for (const kind of ['dependencies', 'budget', 'recovery', 'unknown_usage', 'overrun']) {
    const challenge = (await host.post('/strategy-challenges', { match_key: 'deployment-' + kind, round_seconds: 180, title: 'ALASHI deployment TEST', audience: 'Owners of AI agents', task: 'Write an English ad for ALASHI.', facts: ['Owners use their agents and tools.', 'Crystals represent model tokens.'], required_terms: ['ALASHI', 'agents', 'tokens'], cta: ad.cta, max_words: 80, budget: kind === 'budget' ? 6800 : 20000 })).challenge;
    const run = (await a.post('/strategy-runs', { version: version.id, challenge_id: challenge.id, client_id: 'a-' + kind })).run;
    await b.post('/strategy-runs', { version: rival.id, challenge_id: challenge.id, client_id: 'b-' + kind });
    let release, markSlow, lost = false, slowEnded = false;
    const held = new Promise(r => { release = r; }), slowStarted = new Promise(r => { markSlow = r; });
    const counts = {}, events = [], journalPath = join(directory, kind + '.json');
    const api = { ...a, post: async (path, body) => {
      try {
        const response = await a.post(path, body);
        events.push({ event: path.split('/').at(-1), task: body.task, at: performance.now() });
        if (response.run?.status !== undefined && response.run.status !== 'running') release();
        if (kind === 'recovery' && path.endsWith('/report') && body.task === 'fast' && !lost) {
          lost = true; release(); throw new Error('simulated_lost_accepted_response');
        }
        return response;
      } catch (error) { release(); throw error; }
    } };
    const provider = { source: 'test_provider', provider: 'local', model: 'deployment-test', complete: async messages => {
      const id = JSON.parse(messages[1].content).task;
      counts[id] = (counts[id] ?? 0) + 1; events.push({ event: 'model_start', task: id, at: performance.now() });
      if (id === 'slow') { markSlow(); await held; slowEnded = true; }
      if (id === 'fast') await slowStarted;
      if (id === 'follow' && kind === 'dependencies') { assert.equal(slowEnded, false); release(); }
      const total = id === 'fast' && kind === 'overrun' ? 5001 : 120;
      events.push({ event: 'model_end', task: id, at: performance.now() });
      return { report: id === 'deliver' ? ad : { facts: ['ALASHI agents use tokens.'] }, usage: id === 'fast' && kind === 'unknown_usage' ? null : { prompt_tokens: total - 20, completion_tokens: 20, total_tokens: total, cached_tokens: 0 }, response_id: kind + '-' + id, request_failed: false };
    } };
    const timer = setTimeout(() => release(), 10000);
    try {
      const execute = () => runCycle({ api, id: run.id, provider, journalPath });
      if (kind === 'dependencies') {
        const done = await execute(); assert.equal(done.known_tokens, 480);
      } else {
        await assert.rejects(execute(), kind === 'budget' ? /budget_exhausted/ : kind === 'recovery' ? /simulated_lost_accepted_response/ : kind === 'overrun' ? /run_stopped/ : /report_or_usage_not_verified/);
        assert.deepEqual(counts, { fast: 1, slow: 1 });
        if (kind === 'recovery') {
          const before = JSON.parse(await (await fetch(base + '/strategy-library')).text());
          await stop('SIGKILL'); await start();
          const after = await (await fetch(base + '/strategy-library')).json();
          assert.deepEqual(after.runs, before.runs, 'server receipts survive process crash');
          const done = await execute(); assert.equal(done.known_tokens, 480);
        } else {
          const current = (await a.request(`/agents/${a.record}/owner/strategies`)).runs.find(r => r.id === run.id);
          assert.equal(current.reserved_tokens, kind === 'unknown_usage' ? current.calls.fast.reserved : 0);
          assert.equal(current.unknown_usage, kind === 'unknown_usage');
          assert.equal(current.status, kind === 'unknown_usage' ? 'usage_unknown' : kind === 'overrun' ? 'budget_overrun' : 'running');
        }
      }
      if (['dependencies', 'recovery'].includes(kind)) assert.deepEqual(counts, { fast: 1, slow: 1, follow: 1, deliver: 1 });
      summary.cases.push({ kind, passed: true, counts, events, server_restart: kind === 'recovery' });
    } finally { clearTimeout(timer); release(); }
  }
  writeFileSync(join(directory, 'result.json'), JSON.stringify(summary, null, 2), { mode: 0o600 });
  console.log(JSON.stringify(summary));
} finally { await stop(); }

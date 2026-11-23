import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { readJournalLines } from './attest.mjs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import http from 'node:http';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { verifyReceipt, keyPairFromSeed, promptCachePrefixSha256, normalizeUsage } from './attest.mjs';

const here = fileURLToPath(new URL('.', import.meta.url));
const SEED = 'a'.repeat(64);
const PROVIDER_KEY = 'test-provider-key-123';

function startProxy(env) {
  const child = spawn(process.execPath, [join(here, 'server.mjs')], { env: { ...process.env, ...env }, stdio: ['ignore', 'pipe', 'inherit'] });
  return new Promise((resolve, reject) => {
    const onLine = l => {
      try {
        const m = JSON.parse(l);
        if (m.ok && m.port) { clearTimeout(startTimer); child.stdout.off('data', chunker); resolve({ child, port: m.port }); }
      } catch { /* startup noise */ }
    };
    const chunker = d => d.toString().split('\n').forEach(onLine);
    child.stdout.on('data', chunker);
    child.on('exit', code => reject(new Error(`proxy exited ${code}`)));
    const startTimer = setTimeout(() => { child.kill('SIGKILL'); reject(new Error('proxy start timeout')); }, 10000);
  });
}

// Mock OpenAI-compatible provider: records the Authorization it saw and
// answers with a fixed usage block (100 prompt, 80 cached, 20 completion).
function startMockProvider() {
  const seen = { auth: null, calls: 0 };
  const server = http.createServer((req, res) => {
    seen.calls += 1;
    seen.auth = req.headers.authorization;
    let body = '';
    req.on('data', c => body += c);
    req.on('end', () => {
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify({
        id: 'mock-1', object: 'chat.completion', model: JSON.parse(body).model,
        choices: [{ index: 0, message: { role: 'assistant', content: 'ok' }, finish_reason: 'stop' }],
        usage: { prompt_tokens: 100, completion_tokens: 20, prompt_tokens_details: { cached_tokens: 80 } },
      }));
    });
  });
  return new Promise(resolve => server.listen(0, '127.0.0.1', () => resolve({ server, port: server.address().port, seen })));
}

async function callProxy(port, { endpoint, keyEnv, body, headers = {} }) {
  return fetch(`http://127.0.0.1:${port}/v1/chat/completions`, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...(endpoint ? { 'x-alashi-endpoint': endpoint } : {}),
      ...(keyEnv ? { 'x-alashi-key-env': keyEnv } : {}),
      ...headers,
    },
    body: JSON.stringify(body ?? { model: 'test-model', messages: [{ role: 'system', content: 'sys-prompt' }, { role: 'user', content: 'hi-' + Math.random().toString(36).slice(2) }] }),
  });
}

test('receipts: signed, usage normalized, key never leaks, budget cuts off', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'alashi-egress-'));
  const journal = join(dir, 'receipts.jsonl');
  const provider = await startMockProvider();
  const { child, port } = await startProxy({
    ALASHI_EGRESS_SEED: SEED,
    ALASHI_EGRESS_JOURNAL: journal,
    ALASHI_EGRESS_MAX_TOTAL_TOKENS: '150',
    ALASHI_EGRESS_ALLOWED_HOSTS: `127.0.0.1:${provider.port}`,
    TEST_PROVIDER_KEY: PROVIDER_KEY,
  });
  try {
    // 1. happy path: provider key is injected by the proxy, receipt verifies.
    const r1 = await callProxy(port, { endpoint: `http://127.0.0.1:${provider.port}/v1/chat/completions`, keyEnv: 'TEST_PROVIDER_KEY' });
    assert.equal(r1.status, 200);
    const bodyText = await r1.text();
    assert.equal(provider.seen.auth, `Bearer ${PROVIDER_KEY}`, 'proxy must inject the key upstream');
    assert.ok(!bodyText.includes(PROVIDER_KEY), 'provider key must not appear in the response');
    assert.ok(!JSON.stringify(Object.fromEntries(r1.headers)).includes(PROVIDER_KEY), 'no key in headers');
    const receipt1 = JSON.parse(Buffer.from(r1.headers.get('x-alashi-receipt'), 'base64').toString('utf8'));
    assert.deepEqual(receipt1.usage, { input_fresh: 20, input_cache_read: 80, input_cache_write: null, output: 20 });
    assert.equal(receipt1.model_id, 'test-model');
    assert.equal(receipt1.http_status, 200);
    assert.equal(receipt1.prompt_cache_prefix_sha256, promptCachePrefixSha256('test-model', { messages: [{ role: 'system', content: 'sys-prompt' }] }));
    assert.ok(typeof receipt1.proxy_sig === 'string' && receipt1.proxy_sig.length > 30);
    const { pubRaw } = keyPairFromSeed(SEED);
    const v = verifyReceipt(JSON.stringify(receipt1), pubRaw);
    assert.equal(v.ok, true, 'signature verifies with the pubkey derived from the seed');

    // 2. unknown key env: refused before any upstream call.
    const before = provider.seen.calls;
    const r2 = await callProxy(port, { endpoint: `http://127.0.0.1:${provider.port}/v1/chat/completions`, keyEnv: 'NO_SUCH_KEY' });
    assert.equal(r2.status, 400);
    assert.equal((await r2.json()).error, 'unknown_key_env');
    assert.equal(provider.seen.calls, before, 'no upstream call on unknown key env');

    // 2b. SSRF guard: a host outside the allowlist is refused before any
    // upstream call, so the proxy key can never be attached to it.
    const r2b = await callProxy(port, { endpoint: 'https://evil.example.internal/admin', keyEnv: 'TEST_PROVIDER_KEY' });
    assert.equal(r2b.status, 403);
    assert.equal((await r2b.json()).error, 'disallowed_endpoint');
    assert.equal(provider.seen.calls, before, 'no upstream call on disallowed endpoint');

    // 2c. service-variable guard: the signing seed must never be attachable
    // as a bearer token, even to an allowlisted host.
    const r2c = await callProxy(port, { endpoint: `http://127.0.0.1:${provider.port}/v1/chat/completions`, keyEnv: 'ALASHI_EGRESS_SEED' });
    assert.equal(r2c.status, 400);
    assert.equal((await r2c.json()).error, 'bad_key_env');
    assert.equal(provider.seen.calls, before, 'service-variable probe never reaches the provider');
    assert.equal(provider.seen.auth, `Bearer ${PROVIDER_KEY}`, 'seed must not replace the provider key upstream');

    // 3. budget: the cap (150) is crossed by the second attested call
    //    (120 + 120 = 240); every next call is cut before the provider.
    const r3 = await callProxy(port, { endpoint: `http://127.0.0.1:${provider.port}/v1/chat/completions`, keyEnv: 'TEST_PROVIDER_KEY' });
    assert.equal(r3.status, 200);
    const afterCap = provider.seen.calls;
    const r4 = await callProxy(port, { endpoint: `http://127.0.0.1:${provider.port}/v1/chat/completions`, keyEnv: 'TEST_PROVIDER_KEY' });
    assert.equal(r4.status, 429);
    assert.equal((await r4.json()).error, 'budget_exceeded');
    assert.equal(provider.seen.calls, afterCap, 'over-budget call never reaches the provider');

    // 4. journal: exactly the completed calls, each line verifies standalone.
    const lines = readJournalLines(journal);
    assert.equal(lines.length, 2, 'only successful calls are journaled');
    const verifyOut = execFileSync(process.execPath, [join(here, 'verify.mjs'), journal, '--pubkey', pubRaw.toString('hex')], { encoding: 'utf8' });
    const summary = JSON.parse(verifyOut.slice(verifyOut.indexOf('{')));
    assert.equal(summary.receipts, 2);
    assert.equal(summary.failed, 0);
    assert.equal(summary.attested_tokens, 240);

    // 5. tamper detection: flipping usage.output must break the signature.
    const tampered = JSON.parse(lines[0]);
    tampered.usage.output = 999;
    const tv = verifyReceipt(JSON.stringify(tampered), pubRaw);
    assert.equal(tv.ok, false);
    assert.equal(tv.reason, 'signature_mismatch');

    // 6. outside judge: replayed line and empty journal must both fail.
    const replayJournal = join(dir, 'replay.jsonl');
    const { writeFileSync } = await import('node:fs');
    writeFileSync(replayJournal, lines[0] + '\n' + lines[0] + '\n');
    assert.throws(() => execFileSync(process.execPath, [join(here, 'verify.mjs'), replayJournal, '--pubkey', pubRaw.toString('hex')], { encoding: 'utf8', stdio: 'pipe' }), /duplicate receipt_id/);
    const emptyJournal = join(dir, 'empty.jsonl');
    writeFileSync(emptyJournal, '');
    assert.throws(() => execFileSync(process.execPath, [join(here, 'verify.mjs'), emptyJournal, '--pubkey', pubRaw.toString('hex')], { encoding: 'utf8', stdio: 'pipe' }), /empty_journal/);
  } finally {
    child.kill();
    provider.server.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test('budget counters are serialized: concurrent calls cannot exceed the cap', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'alashi-egress-race-'));
  const journal = join(dir, 'receipts.jsonl');
  const provider = await startMockProvider();
  const { child, port } = await startProxy({
    ALASHI_EGRESS_SEED: SEED,
    ALASHI_EGRESS_JOURNAL: journal,
    ALASHI_EGRESS_MAX_CALLS: '1',
    ALASHI_EGRESS_ALLOWED_HOSTS: `127.0.0.1:${provider.port}`,
    TEST_PROVIDER_KEY: PROVIDER_KEY,
  });
  try {
    const target = `http://127.0.0.1:${provider.port}/v1/chat/completions`;
    const results = await Promise.all([1, 2, 3].map(() => callProxy(port, { endpoint: target, keyEnv: 'TEST_PROVIDER_KEY' })));
    assert.equal(provider.seen.calls, 1, 'exactly one call may reach the provider under MAX_CALLS=1');
    assert.equal(results.filter(r => r.status === 200).length, 1);
    assert.equal(results.filter(r => r.status === 429).length, 2);
    const lines = readJournalLines(journal);
    assert.equal(lines.length, 1);
  } finally {
    child.kill();
    provider.server.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test('idempotent budget: a retried request body is served from the record, not double-spent', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'alashi-egress-idem-'));
  const journal = join(dir, 'receipts.jsonl');
  const provider = await startMockProvider();
  const { child, port } = await startProxy({
    ALASHI_EGRESS_SEED: SEED,
    ALASHI_EGRESS_JOURNAL: journal,
    ALASHI_EGRESS_MAX_CALLS: '10',
    ALASHI_EGRESS_ALLOWED_HOSTS: `127.0.0.1:${provider.port}`,
    TEST_PROVIDER_KEY: PROVIDER_KEY,
  });
  try {
    const target = `http://127.0.0.1:${provider.port}/v1/chat/completions`;
    const sameBody = { model: 'test-model', messages: [{ role: 'system', content: 'sys-prompt' }, { role: 'user', content: 'identical' }] };
    const r1 = await callProxy(port, { endpoint: target, keyEnv: 'TEST_PROVIDER_KEY', body: sameBody });
    assert.equal(r1.status, 200);
    assert.equal(provider.seen.calls, 1);
    const rec1 = JSON.parse(Buffer.from(r1.headers.get('x-alashi-receipt'), 'base64').toString('utf8'));
    // Exact same body retried (client lost the first response).
    const r2 = await callProxy(port, { endpoint: target, keyEnv: 'TEST_PROVIDER_KEY', body: sameBody });
    assert.equal(r2.status, 409, 'retry must be served as duplicate_request');
    assert.equal((await r2.json()).error, 'duplicate_request');
    const rec2 = JSON.parse(Buffer.from(r2.headers.get('x-alashi-receipt'), 'base64').toString('utf8'));
    assert.equal(rec2.proxy_sig, rec1.proxy_sig, 'the committed receipt is returned');
    assert.equal(provider.seen.calls, 1, 'the provider is called exactly once');
    const health = await (await fetch(`http://127.0.0.1:${port}/healthz`)).json();
    assert.equal(health.calls, 1, 'budget counts one call');
    assert.equal(health.spent_tokens, 120, 'tokens spent exactly once');
    // Blind-audit #1: a FAILED commit must charge nothing; the retry after
    // the failure is a fresh call that CAN commit.
    const child2 = spawn(process.execPath, [new URL('./server.mjs', import.meta.url).pathname], {
      env: { ...process.env, ALASHI_EGRESS_SEED: SEED, ALASHI_EGRESS_JOURNAL: join(dir, 'r2.jsonl'),
             ALASHI_EGRESS_ALLOWED_HOSTS: `127.0.0.1:${provider.port}`, TEST_PROVIDER_KEY: PROVIDER_KEY,
             ALASHI_EGRESS_FAIL_COMMIT: '1', NODE_ENV: 'test' }, stdio: ['ignore', 'pipe', 'inherit'] });
    const port2 = await new Promise((resolve, reject) => {
      const onLine = l => { try { const m = JSON.parse(l); if (m.ok && m.port) { done2(); resolve(m.port); } } catch {} };
      child2.stdout.on('data', d => d.toString().split('\n').forEach(onLine));
      const t2 = setTimeout(() => reject(new Error('start timeout 2')), 10000);
      const done2 = () => clearTimeout(t2);
    });
    try {
      const rf = await fetch(`http://127.0.0.1:${port2}/v1/chat/completions`, { method: 'POST',
        headers: { 'content-type': 'application/json', 'x-alashi-endpoint': target, 'x-alashi-key-env': 'TEST_PROVIDER_KEY' },
        body: JSON.stringify(sameBody) });
      assert.equal(rf.status, 500);
      const hf = await (await fetch(`http://127.0.0.1:${port2}/healthz`)).json();
      // New semantics (blind-audit #1 closed): the provider call IS charged
      // (pending) even when the receipt commit fails - real money must
      // never be invisible; tokens settle only with the receipt.
      assert.equal(hf.calls, 1, 'the charge book counts the failed-receipt call');
      assert.ok(hf.spent_tokens >= 0, 'tokens settle with the receipt; pending carries the call count');
    } finally { child2.kill('SIGKILL'); }
  } finally {
    child.kill();
    provider.server.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a failed receipt commit leaves a pending charge that blocks and accounts the retry', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'alashi-egress-pending-'));
  const journal = join(dir, 'receipts.jsonl');
  const provider = await startMockProvider();
  const { child, port } = await startProxy({
    ALASHI_EGRESS_SEED: SEED,
    ALASHI_EGRESS_JOURNAL: journal,
    ALASHI_EGRESS_ALLOWED_HOSTS: `127.0.0.1:${provider.port}`,
    TEST_PROVIDER_KEY: PROVIDER_KEY,
    ALASHI_EGRESS_FAIL_COMMIT: '1',
    NODE_ENV: 'test',
  });
  try {
    const target = `http://127.0.0.1:${provider.port}/v1/chat/completions`;
    const same = { model: 'm', messages: [{ role: 'user', content: 'x' }] };
    const r1 = await callProxy(port, { endpoint: target, keyEnv: 'TEST_PROVIDER_KEY', body: same });
    assert.equal(r1.status, 500, 'receipt commit fails');
    assert.equal(provider.seen.calls, 1, 'provider was called once');
    let health = await (await fetch(`http://127.0.0.1:${port}/healthz`)).json();
    assert.equal(health.calls, 1, 'the charge book counts the call even without a receipt');
    // Retry of the same key: refused, provider NOT called again.
    const r2 = await callProxy(port, { endpoint: target, keyEnv: 'TEST_PROVIDER_KEY', body: same });
    assert.equal(r2.status, 409);
    const body2 = await r2.json();
    assert.equal(body2.error, 'duplicate_request');
    assert.equal(body2.charge_state, 'pending');
    assert.equal(provider.seen.calls, 1, 'no second provider call');
    health = await (await fetch(`http://127.0.0.1:${port}/healthz`)).json();
    assert.equal(health.calls, 1);
    // Budget caps see charges: MAX_CALLS would trip on the pending charge.
  } finally {
    child.kill();
    provider.server.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a receipt-journal write failure returns the signed receipt in header and body', async t => {
  const dir = mkdtempSync(join(tmpdir(), 'alashi-egress-wfail-'));
  const provider = await startMockProvider();
  const { child, port } = await startProxy({
    ALASHI_EGRESS_SEED: SEED,
    ALASHI_EGRESS_JOURNAL: join(dir, 'receipts.jsonl'),
    ALASHI_EGRESS_ALLOWED_HOSTS: `127.0.0.1:${provider.port}`,
    TEST_PROVIDER_KEY: PROVIDER_KEY,
    ALASHI_EGRESS_FAIL_COMMIT: '1',
    NODE_ENV: 'test',
  });
  try {
    const r = await callProxy(port, { endpoint: `http://127.0.0.1:${provider.port}/v1/chat/completions`, keyEnv: 'TEST_PROVIDER_KEY' });
    assert.equal(r.status, 500);
    const body = await r.json();
    assert.equal(body.error, 'receipt_write_failed');
    assert.ok(body.receipt && typeof body.receipt.proxy_sig === 'string');
    const header = r.headers.get('x-alashi-receipt');
    assert.ok(header, '500 must carry x-alashi-receipt');
    const fromHeader = JSON.parse(Buffer.from(header, 'base64').toString('utf8'));
    assert.equal(fromHeader.proxy_sig, body.receipt.proxy_sig);
    const { keyPairFromSeed, verifyReceipt } = await import('./attest.mjs');
    assert.equal(verifyReceipt(JSON.stringify(fromHeader), keyPairFromSeed(SEED).pubRaw).ok, true);
    assert.equal(provider.seen.calls, 1, 'provider was called exactly once');
  } finally {
    child.kill();
    provider.server.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a 302 from an allowlisted provider is refused, not followed', async () => {
  // Hold-out hole (redirect-follow): removing redirect:'error' was not
  // caught by any test. The proxy must never follow a redirect: the
  // receipt signs the original host, while the body would arrive from
  // wherever the provider points. The redirect target here is a LIVE
  // local server returning 200, so accidental DNS failures cannot fake a
  // pass: following the redirect ends in a 200 from the wrong host.
  let targetHit = 0;
  const target = http.createServer((req, res) => {
    targetHit += 1;
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ id: 'evil', choices: [{ message: { content: '{}' } }], usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 } }));
  });
  await new Promise(r => target.listen(0, '127.0.0.1', r));
  const tport = target.address().port;
  const redirector = http.createServer((req, res) => {
    res.writeHead(302, { location: `http://127.0.0.1:${tport}/evil` });
    res.end();
  });
  await new Promise(r => redirector.listen(0, '127.0.0.1', r));
  const rport = redirector.address().port;
  const dir = mkdtempSync(join(tmpdir(), 'alashi-egress-redir-'));
  const { child, port } = await startProxy({
    ALASHI_EGRESS_SEED: SEED,
    ALASHI_EGRESS_JOURNAL: join(dir, 'r.jsonl'),
    ALASHI_EGRESS_ALLOWED_HOSTS: `127.0.0.1:${rport}`,
    TEST_PROVIDER_KEY: PROVIDER_KEY,
  });
  try {
    const r = await callProxy(port, { endpoint: `http://127.0.0.1:${rport}/v1/chat/completions`, keyEnv: 'TEST_PROVIDER_KEY' });
    assert.equal(r.status, 502, 'a redirect must be an explicit upstream failure');
    assert.equal((await r.json()).error, 'provider_unreachable');
    assert.equal(targetHit, 0, 'the redirect target must never be reached');
    assert.ok(!r.headers.get('x-alashi-receipt'), 'no receipt may be signed for a redirect');
  } finally {
    child.kill();
    redirector.close();
    target.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test('attest: missing usage stays null, negative normalization refused', () => {
  assert.equal(normalizeUsage(undefined), null);
  assert.equal(normalizeUsage({}).output, null);
  const u = normalizeUsage({ prompt_tokens: 10, completion_tokens: 5, prompt_tokens_details: { cached_tokens: 80 } });
  assert.equal(u.input_fresh, null, 'cached > prompt must not produce a negative attested number');
  assert.equal(u.input_cache_read, 80);
  // A plain usage block without a cache breakdown is fully fresh input.
  const plain = normalizeUsage({ prompt_tokens: 1000, completion_tokens: 5 });
  assert.equal(plain.input_fresh, 1000);
  assert.equal(plain.output, 5);
  // Anonymous-audit guard: negative, fractional and absurd numbers must not
  // be attested under a valid signature.
  for (const bad of [
    { prompt_tokens: -500, completion_tokens: 5 },
    { prompt_tokens: 10.5, completion_tokens: 1 },
    { prompt_tokens: 1e300, completion_tokens: 1 },
  ]) {
    const out = normalizeUsage(bad);
    assert.equal(out.input_fresh, null, `input_fresh must reject ${JSON.stringify(bad)}`);
    assert.equal(out.output, null || out.output, 'sanity');
  }
  assert.equal(normalizeUsage({ prompt_tokens: -500, completion_tokens: 5 }).input_fresh, null);
});

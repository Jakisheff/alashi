#!/usr/bin/env node
// alashi egress-proxy (Diff 2, Zero-Trust RFC v2): the one place provider
// keys live and the one place provider calls leave through. Runners and
// agents hold no .env secrets; every completed call returns a signed
// InferenceReceipt and appends the same record to a JSONL journal.
//
//   ALASHI_EGRESS_SEED        64-hex-char Ed25519 seed for receipt signing
//   ALASHI_EGRESS_JOURNAL     receipts JSONL path (default ./egress-receipts.jsonl)
//   ALASHI_EGRESS_MAX_TOTAL_TOKENS  budget per proxy process, 0 = unlimited
//   ALASHI_EGRESS_MAX_CALLS   call budget per proxy process, 0 = unlimited
//   ALASHI_EGRESS_BIND        bind address, default 127.0.0.1
//   ALASHI_EGRESS_PORT        port, default 0 (ephemeral)
//
// Provider keys stay in THIS process env only. A request names the variable,
// never the value: POST /v1/chat/completions with headers
//   X-Alashi-Endpoint:  https://provider/v1/chat/completions
//   X-Alashi-Key-Env:   PROVIDER_KEY_VARIABLE_NAME
// Body: the provider's own chat-completions payload, untouched.
// Reply: the provider body plus X-Alashi-Receipt (base64 JSON, signed).
// GET /healthz -> liveness; GET /pubkey -> raw 32-byte signing pubkey.
// 429 budget_exceeded is returned before the provider is called.

import http from 'node:http';
import { keyPairFromSeed, sha256Hex, normalizeUsage, promptCachePrefixSha256, signReceipt } from './attest.mjs';
import { DatabaseSync } from 'node:sqlite';

const env = name => process.env[name];

const seed = env('ALASHI_EGRESS_SEED');
if (!seed || !/^[0-9a-fA-F]{64}$/.test(seed)) {
  console.error('egress-proxy: ALASHI_EGRESS_SEED (64 hex chars) is required');
  process.exit(2);
}
const { priv, pubRaw } = keyPairFromSeed(seed);

// Blocker fix (hy4 falsification): the proxy attaches provider credentials,
// so an arbitrary X-Alashi-Endpoint would hand the key to any host a local
// process names. Endpoints are allowlisted explicitly; no list, no calls.
const allowedHosts = new Set(
  (env('ALASHI_EGRESS_ALLOWED_HOSTS') || '')
    .split(',').map(s => s.trim().toLowerCase()).filter(Boolean)
);
if (allowedHosts.size === 0) {
  console.error('egress-proxy: ALASHI_EGRESS_ALLOWED_HOSTS is required (comma-separated provider hosts; no default trust)');
  process.exit(2);
}
function endpointAllowed(endpointUrl) {
  const host = endpointUrl.port ? `${endpointUrl.hostname}:${endpointUrl.port}` : endpointUrl.hostname;
  return allowedHosts.has(host.toLowerCase());
}

const journalPath = env('ALASHI_EGRESS_JOURNAL') || 'egress-receipts.jsonl';
const maxTotalTokens = Number(env('ALASHI_EGRESS_MAX_TOTAL_TOKENS') || 0);
const maxCalls = Number(env('ALASHI_EGRESS_MAX_CALLS') || 0);

// Budget counters are DERIVED from the committed store (blind-audit #1/#2):
// in-memory counters incremented before the commit double-charged retries
// when the commit failed, and reset on restart while the db survived.
function budgetState() {
  const row = store.prepare('SELECT COUNT(*) AS n, COALESCE(SUM(tokens), 0) AS tok FROM receipts').get();
  return { calls: Number(row.n), spentTokens: Number(row.tok) };
}

// Idempotent budget + durable receipt journal (falsification C closed):
// one sqlite database holds the receipts AND the per-request spend, so a
// retry of the same request body (same request_sha256) is served from the
// committed record: no second provider accounting, no double budget spend.
const store = new DatabaseSync(journalPath + '.db');
store.exec('PRAGMA journal_mode = WAL;');
store.exec(`CREATE TABLE IF NOT EXISTS receipts (
  request_sha256 TEXT PRIMARY KEY,
  receipt_json TEXT NOT NULL,
  tokens INTEGER NOT NULL,
  ts_ms INTEGER NOT NULL);`);

function lookupRequest(requestSha) {
  const row = store.prepare('SELECT receipt_json, tokens FROM receipts WHERE request_sha256 = ?').get(requestSha);
  return row || null;
}

function commitReceipt(requestSha, receiptJson, tokens) {
  // Fault-injection hook (tests only): a deterministic commit failure with
  // an already-open RW descriptor cannot be triggered by chmod, and ENOSPC
  // is not reproducible on demand. Explicitly gated by an env var.
  if (env('ALASHI_EGRESS_FAIL_COMMIT') === '1' && env('NODE_ENV') === 'test') {
    throw new Error('simulated receipt commit failure');
  }
  store.exec('BEGIN');
  try {
    store.prepare('INSERT OR REPLACE INTO receipts(request_sha256, receipt_json, tokens, ts_ms) VALUES(?, ?, ?, ?)')
      .run(requestSha, receiptJson, tokens, Date.now());
    store.exec('COMMIT');
  } catch (e) {
    store.exec('ROLLBACK');
    throw e;
  }
}

// Major fix (hy4 falsification): check-then-fetch-then-increment raced under
// concurrent requests. All budget mutations and journal appends now run in
// one serialized chain, so the counters are consistent per proxy process.
let queue = Promise.resolve();
function serialized(task) {
  const run = queue.then(task, task);
  queue = run.then(() => {}, () => {});
  return run;
}

// Pre-flight guard: a session already over budget is cut before the provider
// is called. The tokens of the call that crosses the line are still spent and
// attested; every later call is refused.
function budgetExceeded() {
  const { calls, spentTokens } = budgetState();
  if (maxCalls > 0 && calls >= maxCalls) return `calls ${calls} >= ${maxCalls}`;
  if (maxTotalTokens > 0 && spentTokens >= maxTotalTokens) return `tokens ${spentTokens} >= ${maxTotalTokens}`;
  return null;
}

function json(res, status, value, extraHeaders = {}) {
  const body = JSON.stringify(value);
  res.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store', ...extraHeaders });
  res.end(body);
}

const server = http.createServer(async (req, res) => {
  if (req.method === 'GET' && req.url === '/healthz') { const s = budgetState(); return json(res, 200, { ok: true, calls: s.calls, spent_tokens: s.spentTokens }); }
  if (req.method === 'GET' && req.url === '/pubkey') {
    res.writeHead(200, { 'content-type': 'application/octet-stream' });
    return res.end(pubRaw);
  }
  if (req.method === 'POST' && req.url === '/v1/chat/completions') {
    const endpoint = req.headers['x-alashi-endpoint'];
    const keyEnv = req.headers['x-alashi-key-env'];
    if (!endpoint) return json(res, 400, { error: 'bad_endpoint', message: 'X-Alashi-Endpoint must be a provider URL' });
    let endpointUrl;
    try { endpointUrl = new URL(endpoint); } catch { return json(res, 400, { error: 'bad_endpoint', message: 'X-Alashi-Endpoint must be a provider URL' }); }
    // https for real providers; plain http only on loopback (local models, tests).
    const loopback = endpointUrl.hostname === '127.0.0.1' || endpointUrl.hostname === 'localhost' || endpointUrl.hostname === '[::1]';
    if (!(endpointUrl.protocol === 'https:' || (endpointUrl.protocol === 'http:' && loopback))) {
      return json(res, 400, { error: 'bad_endpoint', message: 'https required outside loopback' });
    }
    if (!endpointAllowed(endpointUrl)) {
      return json(res, 403, { error: 'disallowed_endpoint', message: 'endpoint host is not in ALASHI_EGRESS_ALLOWED_HOSTS' });
    }
    if (!keyEnv || !/^[A-Z][A-Z0-9_]{0,127}$/.test(keyEnv)) return json(res, 400, { error: 'bad_key_env', message: 'X-Alashi-Key-Env must name an environment variable' });
    // Falsification R4b: service variables must never be usable as bearer
    // material - ALASHI_EGRESS_SEED matches the name regex and would leak
    // the receipt signing identity to whatever host the caller allowlisted.
    if (keyEnv.startsWith('ALASHI_')) return json(res, 400, { error: 'bad_key_env', message: 'X-Alashi-Key-Env must not name an ALASHI_* service variable' });
    const apiKey = env(keyEnv);
    if (!apiKey) return json(res, 400, { error: 'unknown_key_env', message: `no such key variable in proxy env: ${keyEnv}` });

    const chunks = [];
    for await (const c of req) chunks.push(c);
    const requestBody = Buffer.concat(chunks);
    let body;
    try { body = JSON.parse(requestBody.toString('utf8')); } catch { return json(res, 400, { error: 'bad_json', message: 'request body must be JSON' }); }
    const model = typeof body?.model === 'string' ? body.model : '';

    return serialized(async () => {
      // Idempotency: the same request body retried is served from the
      // committed record - no second provider call, no double budget spend
      // (falsification C: a 500 + client retry used to spend twice).
      // Blind-audit #2: the idempotency key binds endpoint AND key env to
      // the body, so the same payload to different hosts is not served a
      // stranger's receipt.
      const requestSha = sha256Hex(endpoint + '\0' + keyEnv + '\0' + requestBody.toString('utf8'));
      const prior = lookupRequest(requestSha);
      if (prior) {
        return json(res, 409, { error: 'duplicate_request',
          message: 'this exact request body was already processed; receipt attached',
          receipt: JSON.parse(prior.receipt_json) }, {
          'x-alashi-receipt': Buffer.from(prior.receipt_json).toString('base64'),
        });
      }
      const over = budgetExceeded();
      if (over) { const s = budgetState(); return json(res, 429, { error: 'budget_exceeded', detail: over, calls: s.calls, spent_tokens: s.spentTokens }); }

      const started = Date.now();
      let upstream;
      try {
        upstream = await fetch(endpoint, {
          method: 'POST',
          redirect: 'error',
          headers: { 'content-type': 'application/json', authorization: `Bearer ${apiKey}` },
          body: requestBody,
          signal: AbortSignal.timeout(Number(env('ALASHI_EGRESS_TIMEOUT_MS') || 120000)),
        });
      } catch (e) {
        return json(res, 502, { error: 'provider_unreachable', message: String(e && e.message || e) });
      }
      const responseBuf = Buffer.from(await upstream.arrayBuffer());
      let responseJson = null;
      try { responseJson = JSON.parse(responseBuf.toString('utf8')); } catch { /* non-JSON provider body still gets a receipt */ }
      const usage = normalizeUsage(responseJson?.usage);
      const total = usage ? (usage.output ?? 0) + (usage.input_cache_read ?? 0) + (usage.input_cache_write ?? 0) + (usage.input_fresh ?? 0) : 0;

      const { calls: committedCalls } = budgetState();
      const receipt = {
        receipt_id: `${started}-${committedCalls + 1}`,
        ts_ms: started,
        model_id: model,
        endpoint_host: new URL(endpoint).host,
        request_sha256: requestSha,
        response_sha256: sha256Hex(responseBuf),
        prompt_cache_prefix_sha256: promptCachePrefixSha256(model, body),
        usage,
        latency_ms: Date.now() - started,
        http_status: upstream.status,
      };
      const proxy_sig = signReceipt(receipt, priv);
      try {
        commitReceipt(receipt.request_sha256, JSON.stringify({ ...receipt, proxy_sig }), total);
      } catch (e) {
        // The call completed without a durable receipt: say so AND hand the
        // signed receipt to the caller - in the BODY and in the header the
        // driver already reads, so a retry reconciles by request_sha256
        // instead of silently double-spending the budget (falsification C +
        // anonymous round 2: the body alone never reached the driver).
        const receiptJson = JSON.stringify({ ...receipt, proxy_sig });
        res.writeHead(500, {
          'content-type': 'application/json',
          'cache-control': 'no-store',
          'x-alashi-receipt': Buffer.from(receiptJson).toString('base64'),
        });
        return res.end(JSON.stringify({ error: 'receipt_write_failed', message: String(e && e.message || e), receipt: { ...receipt, proxy_sig } }));
      }

      const receiptHeader = Buffer.from(JSON.stringify({ ...receipt, proxy_sig })).toString('base64');
      res.writeHead(upstream.status, {
        'content-type': upstream.headers.get('content-type') || 'application/json',
        'cache-control': 'no-store',
        'x-alashi-receipt': receiptHeader,
      });
      res.end(responseBuf);
    });
  }
  return json(res, 404, { error: 'not_found' });
});

const port = Number(env('ALASHI_EGRESS_PORT') || 0);
server.listen(port, env('ALASHI_EGRESS_BIND') || '127.0.0.1', () => {
  const { port: actual, family, address } = server.address();
  console.log(JSON.stringify({ ok: true, listening: family === 'IPv6' ? `[${address}]` : address, port: actual, journal: journalPath }));
});

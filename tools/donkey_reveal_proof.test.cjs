'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createRequire } = require('node:module');
const helper = require('./donkey_reveal_proof.cjs');

const spikeRequire = createRequire(path.resolve(__dirname, '../spike/package.json'));
const web3 = spikeRequire('@solana/web3.js');
const sb = spikeRequire('@switchboard-xyz/on-demand');
const sdkPackage = spikeRequire('@switchboard-xyz/on-demand/package.json');

function key() { return web3.Keypair.generate().publicKey.toBase58(); }

test('gateway request is one bounded pinned endpoint call', async () => {
  assert.equal(sdkPackage.version, '3.10.6');
  let calls = 0;
  const proof = await helper.fetchGatewayProof(async (url, options) => {
    calls++;
    assert.equal(url, 'https://gateway.example/gateway/api/v1/randomness_reveal');
    assert.equal(options.method, 'POST');
    assert.equal(options.signal.aborted, false);
    assert.equal(options.redirect, 'error');
    assert.deepEqual(JSON.parse(options.body), { randomness_key: 'source', slothash: 'hash', slot: 42, rpc: helper.PUBLIC_DEVNET_RPC });
    return { ok: true, json: async () => ({ randomness_pubkey: 'source', signature: 'c2ln', recovery_id: 1, value: 'opaque-sdk-value' }) };
  }, 'https://gateway.example/', { randomness_key: 'source', slothash: 'hash', slot: 42, rpc: 'https://rpc.example/private-token' });
  assert.equal(calls, 1);
  assert.equal(proof.recovery_id, 1);
  assert.equal(helper.GATEWAY_TIMEOUT_MS, 10_000);
  assert.throws(() => helper.gatewayEndpoint('http://gateway.example'), /unsafe_gateway_url/);
});

test('gateway timeout or error is not retried', async () => {
  let calls = 0;
  await assert.rejects(() => helper.fetchGatewayProof(async () => {
    calls++;
    const error = new Error('aborted');
    error.name = 'AbortError';
    error.cause = { code: 'ABORT_ERR' };
    throw error;
  }, 'https://gateway.example', {}), (error) => {
    assert.match(error.message, /gateway_proof_timeout_or_unavailable/);
    assert.equal(error.gateway_diagnostic.stage, 'timeout');
    assert.equal(error.gateway_diagnostic.error_name, 'AbortError');
    assert.equal(error.gateway_diagnostic.error_code, 'ABORT_ERR');
    assert.equal(typeof error.gateway_diagnostic.elapsed_ms, 'number');
    return true;
  });
  assert.equal(calls, 1);
});


test('redirect responses fail after one request and cannot forward the POST', async () => {
  let calls = 0;
  await assert.rejects(() => helper.fetchGatewayProof(async (_url, options) => {
    calls++;
    assert.equal(options.redirect, 'error');
    return { ok: false, status: 307, json: async () => ({}) };
  }, 'https://gateway.example', { rpc: 'https://rpc.example/path-token' }),
  (error) => {
    assert.match(error.message, /gateway_proof_timeout_or_unavailable/);
    assert.equal(error.gateway_diagnostic.stage, 'http');
    assert.equal(error.gateway_diagnostic.http_status, 307);
    return true;
  });
  assert.equal(calls, 1);
});

test('gateway network diagnostics retain only bounded transport fields', async () => {
  await assert.rejects(() => helper.fetchGatewayProof(async () => {
    const network = new TypeError('fetch failed at https://secret.example/path-token');
    network.cause = { code: 'ENOTFOUND' };
    throw network;
  }, 'https://gateway.example/private-prefix', {}), (error) => {
    assert.match(error.message, /gateway_proof_timeout_or_unavailable/);
    assert.deepEqual(error.gateway_diagnostic, {
      stage: 'network',
      error_name: 'TypeError',
      error_code: 'ENOTFOUND',
      elapsed_ms: error.gateway_diagnostic.elapsed_ms,
    });
    assert.equal(typeof error.gateway_diagnostic.elapsed_ms, 'number');
    assert.doesNotMatch(JSON.stringify(error.gateway_diagnostic), /secret|path-token|gateway\.example/i);
    return true;
  });
});

test('pinned SDK encoder owns gateway value serialization', () => {
  const source = { address: key(), oracle: key(), queue: key(), authority: key() };
  const payer = key();
  const encodedSignature = Buffer.alloc(64, 7);
  const encodedValue = Buffer.alloc(32, 9);
  let captured;
  const program = { instruction: { randomnessReveal: (args, context) => {
    captured = { args, context };
    return { programId: sb.ON_DEMAND_DEVNET_PID,
      data: Buffer.concat([Buffer.alloc(8, 1), encodedSignature, Buffer.from([2]), encodedValue]) };
  } } };
  const raw = { randomness_pubkey: source.address, signature: Buffer.alloc(64, 3).toString('base64'),
    recovery_id: 2, value: 'opaque-provider-wire-value' };
  const normalized = helper.encodeGatewayProof({ web3, sb, program, source, payer, raw });
  assert.equal(captured.args.value, raw.value);
  assert.equal(captured.args.recoveryId, 2);
  assert.equal(captured.context.accounts.authority.toBase58(), source.authority);
  assert.deepEqual(Buffer.from(normalized.signature_base64, 'base64'), encodedSignature);
  assert.equal(normalized.recovery_id, 2);
  assert.deepEqual(Buffer.from(normalized.value_base64, 'base64'), encodedValue);
  assert.throws(() => helper.encodeGatewayProof({ web3, sb, program, source,
    payer, raw: { ...raw, randomness_pubkey: key() } }), /gateway_proof_source_mismatch/);
});

test('proof output is owner-only, atomic, and never overwritten', () => {
  const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'alashi-reveal-proof-'));
  fs.chmodSync(parent, 0o700);
  const output = path.join(parent, 'proof.json');
  const proof = { signature_base64: Buffer.alloc(64, 1).toString('base64'), recovery_id: 0,
    value_base64: Buffer.alloc(32, 2).toString('base64') };
  try {
    helper.writeProofAtomically(output, proof);
    assert.equal(fs.statSync(output).mode & 0o777, 0o600);
    assert.deepEqual(JSON.parse(fs.readFileSync(output, 'utf8')), proof);
    assert.throws(() => helper.writeProofAtomically(output, proof), /proof_file_already_exists/);
    fs.chmodSync(parent, 0o755);
    assert.throws(() => helper.safeProofOutput(path.join(parent, 'another.json')), /proof_directory_must_be_0700/);
  } finally {
    fs.rmSync(parent, { recursive: true, force: true });
  }
});

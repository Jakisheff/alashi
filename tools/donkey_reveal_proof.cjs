#!/usr/bin/env node
'use strict';

// Fetch exactly one gateway proof and normalize it through the pinned
// Switchboard SDK's own encoder. This never reads a signer, signs, simulates,
// or submits a transaction. The 0600 output is input to donkey_crank only.
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const crank = require('./donkey_crank.cjs');
const core = require('./donkey_crank_core.cjs');

const spikeRequire = createRequire(path.resolve(__dirname, '../spike/package.json'));
const GATEWAY_TIMEOUT_MS = 10_000;
const PUBLIC_DEVNET_RPC = 'https://api.devnet.solana.com';
const ALASHI_PID = '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC';
const TOKEN_PROGRAM_ID = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';
const NATIVE_MINT = 'So11111111111111111111111111111111111111112';
const ATA_PROGRAM_ID = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';

function fail(code) { throw new Error(code); }
function key(web3, value) { return new web3.PublicKey(value); }
function same(a, b) { return String(a) === String(b); }

function safeDiagnosticToken(value) {
  const text = typeof value === 'string' ? value : '';
  return /^[A-Za-z0-9_.-]{1,80}$/.test(text) ? text : undefined;
}

function gatewayDiagnostic(startedAt, { stage, status, error } = {}) {
  const diagnostic = {
    stage,
    elapsed_ms: Math.max(0, Date.now() - startedAt),
  };
  if (Number.isInteger(status) && status >= 100 && status <= 599) {
    diagnostic.http_status = status;
  }
  const errorName = safeDiagnosticToken(error?.name);
  const errorCode = safeDiagnosticToken(error?.code) || safeDiagnosticToken(error?.cause?.code);
  if (errorName) diagnostic.error_name = errorName;
  if (errorCode) diagnostic.error_code = errorCode;
  return diagnostic;
}

function failGatewayProof(diagnostic) {
  const error = new Error('gateway_proof_timeout_or_unavailable');
  error.gateway_diagnostic = diagnostic;
  throw error;
}

function safeHttpsUrl(value, code) {
  let url;
  try { url = new URL(value); } catch { fail(code); }
  if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash) fail(code);
  return url;
}

function readConfig(file) {
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink()) fail('unsafe_config_file');
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function gatewayEndpoint(gatewayUrl) {
  const base = safeHttpsUrl(gatewayUrl, 'unsafe_gateway_url');
  // Match the pinned SDK's `${gatewayUrl}/gateway/...` behavior, including a
  // deliberate provider path prefix, while still rejecting credentials/query.
  return `${base.toString().replace(/\/$/, '')}/gateway/api/v1/randomness_reveal`;
}

async function fetchGatewayProof(fetchImpl, gatewayUrl, request) {
  const endpoint = gatewayEndpoint(gatewayUrl);
  // Never forward a configured RPC URL: it may contain a provider path token.
  const publicRequest = { ...request, rpc: PUBLIC_DEVNET_RPC };
  const startedAt = Date.now();
  let response;
  try {
    response = await fetchImpl(endpoint, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(publicRequest),
      signal: AbortSignal.timeout(GATEWAY_TIMEOUT_MS),
      redirect: 'error',
    });
  } catch (error) {
    const timeout = error?.name === 'AbortError' || error?.name === 'TimeoutError' ||
      error?.code === 'ABORT_ERR' || error?.cause?.code === 'ABORT_ERR';
    failGatewayProof(gatewayDiagnostic(startedAt, {
      stage: timeout ? 'timeout' : 'network',
      error,
    }));
  }
  if (!response?.ok) {
    failGatewayProof(gatewayDiagnostic(startedAt, { stage: 'http', status: response?.status }));
  }
  let proof;
  try { proof = await response.json(); } catch { fail('invalid_gateway_proof'); }
  if (!proof || typeof proof !== 'object' || Array.isArray(proof) ||
      typeof proof.randomness_pubkey !== 'string' ||
      typeof proof.signature !== 'string' ||
      !Number.isInteger(proof.recovery_id) || proof.recovery_id < 0 || proof.recovery_id > 255 ||
      typeof proof.value !== 'string') {
    fail('invalid_gateway_proof');
  }
  return proof;
}

function encodeGatewayProof({ web3, sb, program, source, payer, raw }) {
  if (!same(raw.randomness_pubkey, source.address)) fail('gateway_proof_source_mismatch');
  const provider = sb.ON_DEMAND_DEVNET_PID;
  const sourceKey = key(web3, source.address);
  const oracle = key(web3, source.oracle);
  const queue = key(web3, source.queue);
  const authority = key(web3, source.authority);
  const payerKey = key(web3, payer);
  const stats = web3.PublicKey.findProgramAddressSync(
    [Buffer.from('OracleRandomnessStats'), oracle.toBuffer()], provider)[0];
  const state = web3.PublicKey.findProgramAddressSync([Buffer.from('STATE')], provider)[0];
  const escrow = web3.PublicKey.findProgramAddressSync([
    sourceKey.toBuffer(), key(web3, TOKEN_PROGRAM_ID).toBuffer(), key(web3, NATIVE_MINT).toBuffer(),
  ], key(web3, ATA_PROGRAM_ID))[0];
  // Keep `raw.value` opaque: only the pinned SDK serializes its gateway wire
  // representation into the provider ABI.
  const ix = program.instruction.randomnessReveal({
    signature: Buffer.from(raw.signature, 'base64'),
    recoveryId: raw.recovery_id,
    value: raw.value,
  }, { accounts: {
    randomness: sourceKey, oracle, queue, stats, authority, payer: payerKey,
    recentSlothashes: web3.SYSVAR_SLOT_HASHES_PUBKEY,
    systemProgram: web3.SystemProgram.programId,
    rewardEscrow: escrow,
    tokenProgram: key(web3, TOKEN_PROGRAM_ID),
    associatedTokenProgram: key(web3, ATA_PROGRAM_ID),
    wrappedSolMint: key(web3, NATIVE_MINT),
    programState: state,
  } });
  if (!ix || !same(ix.programId, provider) || !Buffer.isBuffer(ix.data) || ix.data.length !== 105) {
    fail('pinned_sdk_reveal_encoder_invalid');
  }
  return {
    signature_base64: ix.data.subarray(8, 72).toString('base64'),
    recovery_id: ix.data[72],
    value_base64: ix.data.subarray(73, 105).toString('base64'),
  };
}

function safeProofOutput(file) {
  const absolute = path.resolve(file);
  const checkout = path.resolve(__dirname, '..') + path.sep;
  if (absolute.startsWith(checkout)) fail('proof_file_inside_checkout');
  const parent = fs.lstatSync(path.dirname(absolute));
  if (!parent.isDirectory() || parent.isSymbolicLink() || (parent.mode & 0o077) !== 0) {
    fail('proof_directory_must_be_0700');
  }
  if (fs.existsSync(absolute)) fail('proof_file_already_exists');
  return absolute;
}

function writeProofAtomically(file, proof) {
  const target = safeProofOutput(file);
  const temp = `${target}.${process.pid}.tmp`;
  let fd;
  try {
    fd = fs.openSync(temp, 'wx', 0o600);
    fs.writeFileSync(fd, JSON.stringify(proof));
    fs.fsyncSync(fd);
  } finally {
    if (fd !== undefined) fs.closeSync(fd);
  }
  fs.renameSync(temp, target);
  const parent = fs.openSync(path.dirname(target), 'r');
  try { fs.fsyncSync(parent); } finally { fs.closeSync(parent); }
}

function gatewayUri(oracleData) {
  if (!oracleData || !oracleData.gatewayUri) fail('gateway_uri_unavailable');
  return Buffer.from(oracleData.gatewayUri).toString('utf8').replace(/\0+$/, '');
}

async function main(env = process.env, fetchImpl = globalThis.fetch) {
  const configFile = env.ALASHI_CRANK_CONFIG_FILE;
  const rpcUrl = env.ALASHI_RPC;
  const payer = env.ALASHI_REVEAL_PAYER_PUBKEY;
  const output = env.ALASHI_REVEAL_PROOF_FILE;
  if (!configFile || !rpcUrl || !payer || !output) fail('reveal_proof_config_missing');
  const rpc = safeHttpsUrl(rpcUrl, 'unsafe_rpc_url');
  const outputPath = safeProofOutput(output); // Gate before contacting the gateway.
  const web3 = spikeRequire('@solana/web3.js');
  const sb = spikeRequire('@switchboard-xyz/on-demand');
  const bs58 = spikeRequire('bs58');
  const connection = new web3.Connection(rpc.toString(), 'confirmed');
  if (await connection.getGenesisHash() !== core.DEVNET_GENESIS) fail('not_solana_devnet');
  const providerAccount = await connection.getAccountInfo(sb.ON_DEMAND_DEVNET_PID, 'confirmed');
  if (!providerAccount?.executable) fail('provider_program_not_executable');
  const clockAccount = await connection.getAccountInfo(web3.SYSVAR_CLOCK_PUBKEY, 'confirmed');
  const clock = crank.parseClock(clockAccount);
  const config = readConfig(configFile);
  const snapshot = await crank.loadSnapshot(connection, web3, sb, config, clock);
  if (!snapshot.source) fail('reveal_source_unavailable');
  const plan = core.planStep(snapshot.game, snapshot.oracle, snapshot.source,
    config.faction_pdas, clock.unixTime, clock.slot);
  if (plan.kind !== 'reveal_and_resolve' || !snapshot.randomness ||
      !Buffer.isBuffer(snapshot.source.seedSlothash) || snapshot.source.seedSlothash.length !== 32) {
    fail('reveal_not_ready');
  }
  const program = await sb.AnchorUtils.loadProgramFromConnection(connection, undefined, sb.ON_DEMAND_DEVNET_PID);
  const oracleData = await new sb.Oracle(program, key(web3, snapshot.source.oracle)).loadData();
  const raw = await fetchGatewayProof(fetchImpl, gatewayUri(oracleData), {
    randomness_key: snapshot.source.address,
    slothash: bs58.encode(snapshot.source.seedSlothash),
    slot: Number(snapshot.source.seedSlot),
    rpc: rpc.toString(),
  });
  const proof = encodeGatewayProof({ web3, sb, program, source: snapshot.source, payer, raw });
  // One final public read prevents writing a proof for a changed/revealed source.
  const finalClock = crank.parseClock(await connection.getAccountInfo(web3.SYSVAR_CLOCK_PUBKEY, 'confirmed'));
  const finalSnapshot = await crank.loadSnapshot(connection, web3, sb, config, finalClock);
  if (!finalSnapshot.source || finalSnapshot.source.address !== snapshot.source.address ||
      finalSnapshot.source.seedSlot !== snapshot.source.seedSlot || finalSnapshot.source.revealSlot !== 0n ||
      core.planStep(finalSnapshot.game, finalSnapshot.oracle, finalSnapshot.source,
        config.faction_pdas, finalClock.unixTime, finalClock.slot).kind !== 'reveal_and_resolve') {
    fail('reveal_source_changed');
  }
  writeProofAtomically(outputPath, proof);
  return { status: 'proof_ready', game_pda: config.game_pda, source: snapshot.source.address,
    round: snapshot.game.round };
}

if (require.main === module) {
  main().then((result) => console.log(JSON.stringify(result))).catch((error) => {
    const known = new Set(['reveal_proof_config_missing', 'unsafe_rpc_url', 'not_solana_devnet',
      'provider_program_not_executable', 'proof_file_inside_checkout', 'proof_directory_must_be_0700',
      'proof_file_already_exists', 'reveal_source_unavailable', 'reveal_not_ready', 'unsafe_gateway_url',
      'gateway_uri_unavailable', 'gateway_proof_timeout_or_unavailable', 'invalid_gateway_proof',
      'gateway_proof_source_mismatch', 'pinned_sdk_reveal_encoder_invalid', 'reveal_source_changed']);
    const result = { ok: false, error: known.has(error.message) ? error.message : 'reveal_proof_preflight_failed' };
    if (error?.gateway_diagnostic) result.gateway_diagnostic = error.gateway_diagnostic;
    console.error(JSON.stringify(result));
    process.exitCode = 1;
  });
}

module.exports = { GATEWAY_TIMEOUT_MS, PUBLIC_DEVNET_RPC, gatewayEndpoint, fetchGatewayProof, gatewayDiagnostic, encodeGatewayProof,
  safeProofOutput, writeProofAtomically, main };

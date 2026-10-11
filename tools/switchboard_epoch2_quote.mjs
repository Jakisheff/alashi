#!/usr/bin/env node
// Read-only Switchboard epoch-2 quote. It never signs, sends, funds, or
// persists a keypair. Source public keys must be supplied by the operator.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const SCHEMA = 'alashi.switchboard_epoch2.quote.v1';
const SDK_NAME = '@switchboard-xyz/on-demand';
const SDK_VERSION = '3.10.6';
const SDK_INTEGRITY = 'sha512-5tP7eEQkJr5aK+5JOZ09Vbsx0jT9jQNRpDTJHLKMc1Ro5jSUfm/9Q5e6voau/U9rPVgFU5S/lUa5/MUfWozURQ==';
const CANDIDATE_SHA256 = '31117a3084318f5d9c8dbc3c3f291168545e474728d981f3089a7554c698978d';
const ROLLBACK_SHA256 = 'de492e03624449a36e10a9267563ed46bbaf4eb26204f60306f2c5e10020cde2';
const SDK_LOCK_SHA256 = '630381a34b75758075c3623f628897968751604b23dabe1857c8c8c73f3fc23c';
const DEVNET_RPC = 'https://api.devnet.solana.com';
const DEVNET_GENESIS = 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG';
const ALASHI_PROGRAM_ID = '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC';
const CLOCK_SYSVAR = 'SysvarC1ock11111111111111111111111111111111';
const COST_FIELDS = [
  'source_rent_lamports', 'escrow_rent_lamports', 'lut_rent_lamports',
  'provider_fee_cap_lamports', 'provider_prefund_lamports',
  'aggregate_tx_fee_cap_lamports', 'temporary_buffer_lamports',
];
const LIMIT_FIELDS = [
  'tx_count_max', 'signatures_max', 'retries_max', 'bootstrap_attempts_max',
  'compute_units_per_tx_max', 'priority_fee_micro_lamports_per_cu_max',
];

function fail(code) { throw new Error(code); }
function isDecimal(value) { return typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value); }
function isSha256(value) { return typeof value === 'string' && /^[0-9a-f]{64}$/i.test(value); }
function isObject(value) { return value !== null && typeof value === 'object' && !Array.isArray(value); }
function normalizeRpc(url) { return url.endsWith('/') ? url.slice(0, -1) : url; }
function toPublicKey(web3, value, field) {
  if (typeof value !== 'string') fail(`missing_${field}`);
  try { return new web3.PublicKey(value); } catch { fail(`invalid_${field}`); }
}

function readInput(file) {
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink()) fail('unsafe_input_file');
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function validateInput(input, web3) {
  if (!isObject(input) || input.schema !== SCHEMA) fail('invalid_schema');
  const url = new URL(input.rpc_url);
  if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash ||
      normalizeRpc(url.toString()) !== DEVNET_RPC) fail('rpc_url_must_be_public_devnet');
  const payer = toPublicKey(web3, input.payer, 'payer');
  const alashiProgram = toPublicKey(web3, input.alashi_program_id, 'alashi_program_id');
  if (alashiProgram.toBase58() !== ALASHI_PROGRAM_ID) fail('alashi_program_id_not_pinned');
  const providerProgram = toPublicKey(web3, input.program_id, 'program_id');
  const queue = toPublicKey(web3, input.queue, 'queue');
  const queueAuthority = toPublicKey(web3, input.queue_authority, 'queue_authority');
  if (!Array.isArray(input.sources) || input.sources.length !== 6) fail('six_sources_required');
  const sources = input.sources.map((value) => toPublicKey(web3, value, 'source'));
  if (new Set(sources.map((source) => source.toBase58())).size !== 6) fail('sources_must_be_distinct');
  if (!isObject(input.release) || input.release.candidate_sha256 !== CANDIDATE_SHA256 ||
      input.release.rollback_sha256 !== ROLLBACK_SHA256 ||
      !isSha256(input.release.candidate_sha256) || !isSha256(input.release.rollback_sha256)) {
    fail('release_hashes_required');
  }
  if (!isObject(input.pinned_sdk) || input.pinned_sdk.lock_sha256 !== SDK_LOCK_SHA256 ||
      !isSha256(input.pinned_sdk.lock_sha256)) fail('sdk_lock_hash_required');
  if (input.simulate_unsigned_create_commit !== undefined && input.simulate_unsigned_create_commit !== true) fail('invalid_simulation_flag');
  const measured = isObject(input.measured_costs) ? input.measured_costs : {};
  for (const [field, value] of Object.entries(measured)) {
    if (!COST_FIELDS.includes(field) || !isDecimal(value)) fail('invalid_measured_cost');
  }
  const limits = isObject(input.limits) ? input.limits : {};
  for (const [field, value] of Object.entries(limits)) {
    if (!LIMIT_FIELDS.includes(field) || !Number.isSafeInteger(value) || value < 0) fail('invalid_limit');
  }
  return { url: DEVNET_RPC, payer, alashiProgram, providerProgram, queue, queueAuthority, sources, measured, limits, simulate: input.simulate_unsigned_create_commit === true };
}

function unavailableResult(input, checked, extra = {}, missing = []) {
  const suppliedCosts = {};
  for (const field of COST_FIELDS) {
    if (checked.measured[field] !== undefined) suppliedCosts[field] = checked.measured[field];
  }
  return {
    schema: SCHEMA,
    status: 'unavailable',
    observed_at_utc: new Date().toISOString(),
    alashi_program_id: input.alashi_program_id,
    program_id: input.program_id,
    queue: input.queue,
    queue_authority: input.queue_authority,
    source_authority: input.payer,
    sources: checked.sources.map((source) => source.toBase58()),
    pinned_sdk: { name: SDK_NAME, version: SDK_VERSION, integrity: SDK_INTEGRITY, lock_sha256: SDK_LOCK_SHA256 },
    release: input.release,
    // These values are retained for operator comparison only. They never make
    // this read-only quote complete because the helper cannot attest provenance.
    operator_supplied_costs_unverified: suppliedCosts,
    operator_supplied_limits_unverified: checked.limits,
    ...extra,
    missing: [...new Set([
      ...COST_FIELDS.map((field) => `verified_costs.${field}`),
      ...LIMIT_FIELDS.map((field) => `verified_limits.${field}`),
      'gateway_health',
      'deployed_seed_slot_compatibility',
      ...missing,
    ])],
  };
}

const RANDOMNESS_INIT_ACCOUNTS = ['randomness', 'reward_escrow', 'authority', 'queue', 'payer', 'system_program', 'token_program', 'associated_token_program', 'wrapped_sol_mint', 'program_state', 'lut_signer', 'lut', 'address_lookup_table_program'];

function requiredSignerKeys(message) {
  if (!Array.isArray(message.staticAccountKeys) || !Number.isInteger(message.header?.numRequiredSignatures)) {
    fail('unsigned_message_shape_invalid');
  }
  return message.staticAccountKeys.slice(0, message.header.numRequiredSignatures).map((key) => key.toBase58());
}

export async function quote(input, deps) {
  const { web3, sb, connection, now = () => new Date(), sdkVersion } = deps;
  const checked = validateInput(input, web3);
  if (input.program_id !== sb.ON_DEMAND_DEVNET_PID.toBase58()) fail('program_id_not_pinned_devnet');
  const installedSdkVersion = sdkVersion ?? sb.__package_version;
  if (installedSdkVersion && installedSdkVersion !== SDK_VERSION) fail('pinned_sdk_version_mismatch');

  const [genesisHash, observedSlot, program, queueAccount, latest] = await Promise.all([
    connection.getGenesisHash(), connection.getSlot('confirmed'),
    connection.getAccountInfo(checked.providerProgram, 'confirmed'),
    connection.getAccountInfo(checked.queue, 'confirmed'), connection.getLatestBlockhash('confirmed'),
  ]);
  if (genesisHash !== DEVNET_GENESIS) fail('devnet_genesis_mismatch');
  if (!program?.executable) fail('switchboard_program_not_executable');
  if (!queueAccount || queueAccount.owner?.toBase58?.() !== input.program_id) {
    return unavailableResult(input, checked, {
      observed_at_utc: now().toISOString(), observed_slot: observedSlot, genesis_hash: genesisHash,
      evidence: [{ kind: 'queue_owner', status: 'unavailable_or_wrong_owner' }],
    }, ['live_queue_account']);
  }

  const sbProgram = await sb.AnchorUtils.loadProgramFromConnection(connection, undefined, checked.providerProgram);
  const queueData = await new sb.Queue(sbProgram, checked.queue).loadData();
  if (!queueData?.authority?.toBase58 || queueData.authority.toBase58() !== input.queue_authority) {
    return unavailableResult(input, checked, {
      observed_at_utc: now().toISOString(), observed_slot: observedSlot, genesis_hash: genesisHash,
      evidence: [{ kind: 'queue_decode', status: 'authority_mismatch' }],
    }, ['queue_authority_binding']);
  }
  const oracleCount = Number(queueData.oracleKeysLen);
  if (!Number.isSafeInteger(oracleCount) || oracleCount < 1) {
    return unavailableResult(input, checked, {
      observed_at_utc: now().toISOString(), observed_slot: observedSlot, genesis_hash: genesisHash,
      evidence: [{ kind: 'queue_decode', status: 'no_oracles' }],
    }, ['queue_oracle_readiness']);
  }

  const creations = [];
  const nullFees = [];
  const randomSources = [];
  for (const source of checked.sources) {
    // Randomness.create accepts only the source public key here. It constructs
    // an unsigned instruction requiring the future payer and source signatures.
    const [randomness, createIx] = await sb.Randomness.create(sbProgram, { publicKey: source }, checked.queue, checked.payer);
    randomSources.push({ randomness, createIx, source });
    const message = new web3.TransactionMessage({
      payerKey: checked.payer, recentBlockhash: latest.blockhash, instructions: [createIx],
    }).compileToV0Message([]);
    const signers = requiredSignerKeys(message);
    if (!signers.includes(checked.payer.toBase58()) || !signers.includes(source.toBase58())) {
      fail('unsigned_creation_missing_required_signer');
    }
    const fee = await connection.getFeeForMessage(message, 'confirmed');
    if (fee.value === null) nullFees.push(source.toBase58());
    creations.push({
      source: source.toBase58(), account_metas: createIx.keys.length,
      required_signer_pubkeys: signers, serialized_message_bytes: message.serialize().length,
      fee_lamports: fee.value === null ? null : String(fee.value),
    });
  }
  let simulation = null;
  const simulationMissing = [];
  if (checked.simulate) {
    try {
      const first = randomSources[0];
      const oracle = queueData.oracleKeys?.find((key) => key?.toBase58?.());
      if (!oracle) throw new Error('queue_has_no_decodable_oracle');
      const commitIx = await first.randomness.commitIx(checked.queue, checked.payer, oracle);
      const finalized = await connection.getLatestBlockhash('finalized');
      const message = new web3.TransactionMessage({
        payerKey: checked.payer, recentBlockhash: finalized.blockhash, instructions: [first.createIx, commitIx],
      }).compileToV0Message([]);
      const initMetas = first.createIx.keys.slice(0, RANDOMNESS_INIT_ACCOUNTS.length);
      const initMeta = (name) => initMetas[RANDOMNESS_INIT_ACCOUNTS.indexOf(name)]?.pubkey;
      const rewardEscrow = initMeta('reward_escrow');
      const lut = initMeta('lut');
      if (!rewardEscrow || !lut) throw new Error('randomness_init_account_shape_changed');
      const accountAddresses = [first.source.toBase58(), rewardEscrow.toBase58(), lut.toBase58(), checked.payer.toBase58(), CLOCK_SYSVAR];
      const beforePayer = await connection.getBalance(checked.payer, 'finalized');
      const simulated = await connection.simulateTransaction(new web3.VersionedTransaction(message), {
        sigVerify: false, commitment: 'finalized', accounts: { encoding: 'base64', addresses: accountAddresses },
      });
      const value = simulated.value;
      if (value.err) throw new Error(`simulation_failed:${JSON.stringify(value.err)}`);
      const sourceAccount = value.accounts?.[0];
      const escrowAccount = value.accounts?.[1];
      const lutAccount = value.accounts?.[2];
      const payerAfter = value.accounts?.[3]?.lamports;
      const clockAccount = value.accounts?.[4];
      if (!sourceAccount?.data?.[0] || !clockAccount?.data?.[0] || !Number.isSafeInteger(payerAfter)) throw new Error('simulation_account_data_missing');
      const clockSlot = Buffer.from(clockAccount.data[0], 'base64').readBigUInt64LE(0);
      const sourceData = await sbProgram.coder.accounts.decode('randomnessAccountData', Buffer.from(sourceAccount.data[0], 'base64'));
      const sourceDataBytes = Buffer.from(sourceAccount.data[0], 'base64').length;
      const accountBytes = (account) => account?.data?.[0] ? Buffer.from(account.data[0], 'base64').length : null;
      const seedSlot = sourceData.seedSlot?.toString?.();
      const initSlot = Number(sbProgram.coder.instruction.decode(first.createIx.data).data.params.recentSlot.toString());
      const seedSlotDelta = seedSlot === undefined ? null : (BigInt(seedSlot) - clockSlot).toString();
      simulation = {
        status: seedSlotDelta === '1' ? 'compatible' : 'incompatible', source: first.source.toBase58(), oracle: oracle.toBase58(),
        context_slot: simulated.context.slot, clock_slot: String(clockSlot), sdk_init_slot: initSlot, seed_slot: seedSlot ?? null, seed_slot_delta: seedSlotDelta,
        reveal_slot: sourceData.revealSlot?.toString?.() ?? null,
        source_authority: sourceData.authority?.toBase58?.() ?? null,
        // The provider can allocate more bytes than the decoded Anchor struct.
        // Preserve the actual returned allocation so the launch packet prices
        // rent from the transaction, rather than from a guessed struct size.
        source_data_bytes: sourceDataBytes,
        source_rent_lamports: String(sourceAccount.lamports),
        escrow: { address: rewardEscrow.toBase58(), data_bytes: accountBytes(escrowAccount), lamports: escrowAccount?.lamports === undefined ? null : String(escrowAccount.lamports) },
        lut: { address: lut.toBase58(), data_bytes: accountBytes(lutAccount), lamports: lutAccount?.lamports === undefined ? null : String(lutAccount.lamports) },
        payer_debit_lamports: String(beforePayer - payerAfter),
        required_signer_pubkeys: requiredSignerKeys(message), serialized_message_bytes: message.serialize().length,
        account_metas: first.createIx.keys.slice(0, RANDOMNESS_INIT_ACCOUNTS.length).map((meta, index) => ({
          name: RANDOMNESS_INIT_ACCOUNTS[index], pubkey: meta.pubkey.toBase58(), signer: meta.isSigner, writable: meta.isWritable,
        })),
      };
    } catch (error) {
      simulation = { status: 'unavailable', error: String(error.message || error) };
      simulationMissing.push('unsigned_create_commit_simulation');
    }
  }
  if (simulation?.status === 'incompatible') simulationMissing.push('deployed_seed_slot_plus_one_compatibility');
  return unavailableResult(input, checked, {
    observed_at_utc: now().toISOString(), observed_slot: observedSlot, genesis_hash: genesisHash,
    queue_oracle_count: oracleCount,
    operations: { source_initializations: 6, provider_requests_max: 6, creation_messages: creations, ...(simulation ? { unsigned_create_commit_simulation: simulation } : {}) },
    evidence: [
      { kind: 'sdk_unsigned_randomness_init', version: SDK_VERSION, source_authority: input.payer },
      { kind: 'rpc_get_fee_for_message', commitment: 'confirmed' },
      { kind: 'queue_decode', status: 'verified', owner: input.program_id, authority: input.queue_authority, oracle_count: oracleCount },
      { kind: 'source_seed_slot_gate', status: simulation?.status === 'compatible' ? 'simulated_plus_one_not_deployed_receipt' : simulation?.status === 'incompatible' ? 'simulated_clock_minus_one_incompatible' : 'unverified_without_deployed_receipt_or_simulation' },
    ],
  }, [...(nullFees.length ? ['source_creation_fee_lamports'] : []), ...simulationMissing]);
}

function loadDeps() {
  const here = path.dirname(fileURLToPath(import.meta.url));
  const sdkRoot = process.env.ALASHI_SWITCHBOARD_SDK_ROOT || path.resolve(here, '../spike');
  const lockPath = path.join(sdkRoot, 'package-lock.json');
  const lockHash = createHash('sha256').update(fs.readFileSync(lockPath)).digest('hex');
  if (lockHash !== SDK_LOCK_SHA256) fail('pinned_sdk_lock_hash_mismatch');
  const req = createRequire(path.resolve(sdkRoot, 'package.json'));
  const web3 = req('@solana/web3.js');
  const sb = req(SDK_NAME);
  const pkg = req(`${SDK_NAME}/package.json`);
  if (pkg.version !== SDK_VERSION) fail('pinned_sdk_version_mismatch');
  return { web3, sb, sdkVersion: pkg.version };
}

async function main(argv) {
  if (argv.length !== 3) fail('usage: switchboard_epoch2_quote.mjs <public-input.json>');
  const input = readInput(argv[2]);
  const deps = loadDeps();
  process.stdout.write(`${JSON.stringify(await quote(input, {
    ...deps, connection: new deps.web3.Connection(DEVNET_RPC, 'confirmed'),
  }))}\n`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main(process.argv).catch((error) => {
    process.stdout.write(`${JSON.stringify({ schema: SCHEMA, status: 'invalid', error: error.message })}\n`);
    process.exitCode = 2;
  });
}

#!/usr/bin/env node
'use strict';

// Read-only budget preflight. No key loading, signing or transaction submission.
const fs = require('node:fs');
const crypto = require('node:crypto');

const GENESIS = 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG';
const PROGRAM = '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC';
const AUTHORITY = '64WA7wFVJK3GkfSAkfiAWR37bgjCw1RYcj17qEDj4AVJ';
const LOADER = 'BPFLoaderUpgradeab1e11111111111111111111111';
const SOURCE_SHA = '31117a3084318f5d9c8dbc3c3f291168545e474728d981f3089a7554c698978d';
const ROLLBACK_SHA = 'de492e03624449a36e10a9267563ed46bbaf4eb26204f60306f2c5e10020cde2';
const SDK_LOCK_SHA = '630381a34b75758075c3623f628897968751604b23dabe1857c8c8c73f3fc23c';
const SDK_INTEGRITY = 'sha512-5tP7eEQkJr5aK+5JOZ09Vbsx0jT9jQNRpDTJHLKMc1Ro5jSUfm/9Q5e6voau/U9rPVgFU5S/lUa5/MUfWozURQ==';
const BINARY_BYTES = 530256;
const OLD_BINARY_BYTES = 464392;
const ENTRY_FEE = 50000000n; // selected two-player acceptance plan, not protocol minimum
const SIZES = Object.freeze({ programDataNew: BINARY_BYTES + 45,
  programDataOld: OLD_BINARY_BYTES + 45, buffer: BINARY_BYTES + 37,
  source: 408, game: 909, faction: 246, oracle: 500 });

function fail(message) { throw new Error(message); }
function decimal(value, name) {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value)) fail(`invalid_${name}`);
  return BigInt(value);
}
function sha256(path) { return crypto.createHash('sha256').update(fs.readFileSync(path)).digest('hex'); }
function alphabet58(bytes) {
  const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  let value = BigInt(`0x${Buffer.from(bytes).toString('hex')}`);
  let out = '';
  while (value > 0n) { out = alphabet[Number(value % 58n)] + out; value /= 58n; }
  for (const byte of bytes) { if (byte !== 0) break; out = '1' + out; }
  return out || '1';
}
function parseProgramData(value, expectedProgram) {
  if (!value || value.owner !== LOADER || !Array.isArray(value.data) || value.data[1] !== 'base64') fail('program_data_owner_or_encoding');
  const data = Buffer.from(value.data[0], 'base64');
  if (data.length < 45 || data.readUInt32LE(0) !== 3 || data[12] !== 1) fail('program_data_layout');
  return { bytes: data.length, slot: data.readBigUInt64LE(4).toString(),
    authority: alphabet58(data.subarray(13, 45)),
    binary_sha256: crypto.createHash('sha256').update(data.subarray(45)).digest('hex'), expectedProgram };
}
function parseProgram(value) {
  if (!value || value.owner !== LOADER || value.executable !== true || !Array.isArray(value.data) || value.data[1] !== 'base64') fail('program_owner_or_encoding');
  const data = Buffer.from(value.data[0], 'base64');
  if (data.length < 36 || data.readUInt32LE(0) !== 2) fail('program_layout');
  return alphabet58(data.subarray(4, 36));
}
function makeRpc(url, fetchFn = fetch) {
  let parsed;
  try { parsed = new URL(url); } catch { fail('https_rpc_required'); }
  if (parsed.protocol !== 'https:' || parsed.hostname !== 'api.devnet.solana.com' ||
      parsed.pathname !== '/' || parsed.username || parsed.password || parsed.search || parsed.hash) fail('public_devnet_rpc_required');
  let id = 0;
  return async (method, params = []) => {
    const res = await fetchFn(url, { method: 'POST', signal: AbortSignal.timeout(10000), headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method, params }) });
    if (!res.ok) fail(`rpc_http_${res.status}`);
    const body = await res.json();
    if (body.error || body.result === undefined) fail(`rpc_${method}_failed`);
    return body.result;
  };
}
async function preflight(input, rpc, verifyFile = (path, bytes, hash) => {
  const stat = fs.statSync(path);
  return stat.isFile() && stat.size === bytes && sha256(path) === hash;
}, expectedDeployedHash = ROLLBACK_SHA) {
  if (!input || input.schema !== 'alashi.epoch2.launch_input.v1') fail('input_schema');
  if (input.program_id !== PROGRAM || input.expected_authority !== AUTHORITY) fail('program_or_authority_mismatch');
  if (!input.binary_path || !input.rollback_path) fail('binary_paths_required');
  if (!verifyFile(input.binary_path, BINARY_BYTES, SOURCE_SHA)) fail('candidate_binary_mismatch');
  if (!verifyFile(input.rollback_path, OLD_BINARY_BYTES, ROLLBACK_SHA)) fail('rollback_binary_mismatch');
  const genesis = await rpc('getGenesisHash');
  if (genesis !== GENESIS) fail('devnet_genesis_mismatch');
  const programResult = await rpc('getAccountInfo', [PROGRAM, { encoding: 'base64', commitment: 'finalized' }]);
  const programDataAddress = parseProgram(programResult.value);
  const dataResult = await rpc('getAccountInfo', [programDataAddress, { encoding: 'base64', commitment: 'finalized' }]);
  const programData = parseProgramData(dataResult.value, PROGRAM);
  if (programData.authority !== AUTHORITY || programData.bytes !== SIZES.programDataOld ||
      programData.binary_sha256 !== expectedDeployedHash) fail('deployed_program_drift');
  const rents = {};
  for (const [name, size] of Object.entries(SIZES)) {
    const result = await rpc('getMinimumBalanceForRentExemption', [size, { commitment: 'finalized' }]);
    if (!Number.isSafeInteger(result) || result < 0) fail(`rent_${name}_invalid`);
    rents[name] = BigInt(result);
  }
  const extension = rents.programDataNew - BigInt(dataResult.value.lamports);
  if (extension < 0n) fail('program_data_balance_exceeds_new_rent');
  const fixed = extension + 6n * rents.source + rents.game + 2n * rents.faction + rents.oracle + 2n * ENTRY_FEE;
  const inputQuote = input.switchboard_quote || null;
  let dynamic = null;
  let quoteStatus = 'missing';
  if (inputQuote) {
    if (inputQuote.schema !== 'alashi.switchboard_epoch2.quote.v1' ||
        inputQuote.genesis_hash !== GENESIS || inputQuote.program_id !== PROGRAM ||
        inputQuote.release?.candidate_sha256 !== SOURCE_SHA ||
        inputQuote.release?.rollback_sha256 !== ROLLBACK_SHA ||
        inputQuote.pinned_sdk?.name !== '@switchboard-xyz/on-demand' ||
        inputQuote.pinned_sdk?.version !== '3.10.6' ||
        inputQuote.pinned_sdk?.integrity !== SDK_INTEGRITY ||
        inputQuote.pinned_sdk?.lock_sha256 !== SDK_LOCK_SHA) fail('switchboard_quote_binding');
    quoteStatus = inputQuote.status;
    if (quoteStatus === 'ok') {
      const publicId = s => typeof s === 'string' && /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(s);
      if (!input.switchboard || inputQuote.queue !== input.switchboard.queue ||
          inputQuote.authority !== input.switchboard.authority ||
          !publicId(inputQuote.queue) || !publicId(inputQuote.authority) ||
          !Array.isArray(input.switchboard.sources) || input.switchboard.sources.length !== 6 ||
          new Set(input.switchboard.sources).size !== 6 || !input.switchboard.sources.every(publicId) ||
          JSON.stringify(inputQuote.sources) !== JSON.stringify(input.switchboard.sources)) fail('switchboard_quote_source_binding');
      const currentSlot = await rpc('getSlot', [{ commitment:'finalized' }]);
      if (!Number.isSafeInteger(currentSlot) || !Number.isSafeInteger(inputQuote.observed_slot) ||
          inputQuote.observed_slot > currentSlot || currentSlot-inputQuote.observed_slot > 150) fail('switchboard_quote_stale_slot');
      const observedAt = Date.parse(inputQuote.observed_at_utc);
      if (!Number.isFinite(observedAt) || observedAt > Date.now()+60000 || Date.now()-observedAt > 1800000) fail('switchboard_quote_stale_time');
      if (!Array.isArray(inputQuote.evidence) || inputQuote.evidence.length < 1 ||
          (inputQuote.missing || []).length !== 0) fail('switchboard_quote_evidence_incomplete');
      const fields = ['escrow_rent_lamports', 'lut_rent_lamports', 'provider_fee_cap_lamports',
        'provider_prefund_lamports', 'aggregate_tx_fee_cap_lamports'];
      dynamic = Object.fromEntries(fields.map(name => [name, decimal(inputQuote.costs?.[name], name)]));
      const limits = inputQuote.limits || {};
      for (const [name, max] of Object.entries({tx_count_max:1000, signatures_max:2000,
        retries_max:20, bootstrap_attempts_max:6, compute_units_per_tx_max:1400000,
        priority_fee_micro_lamports_per_cu_max:1000000})) {
        if (!Number.isSafeInteger(limits[name]) || limits[name] < 0 || limits[name] > max) fail(`invalid_${name}`);
      }
      if (limits.tx_count_max < 1 || limits.signatures_max < limits.tx_count_max || limits.bootstrap_attempts_max < 1) fail('invalid_quote_limits');
    } else if (quoteStatus !== 'unavailable' && quoteStatus !== 'invalid') fail('quote_status');
  }
  // Provider fees spend out of a pre-funded balance; count the larger liquidity
  // requirement once, rather than adding both the prefund and its spending cap.
  const dynamicTotal = dynamic && dynamic.escrow_rent_lamports + dynamic.lut_rent_lamports +
    dynamic.aggregate_tx_fee_cap_lamports +
    (dynamic.provider_prefund_lamports > dynamic.provider_fee_cap_lamports ?
      dynamic.provider_prefund_lamports : dynamic.provider_fee_cap_lamports);
  const optionalBalances = {};
  for (const [role, pubkey] of Object.entries(input.public_wallets || {})) {
    if (!/^[A-Za-z0-9_]{1,32}$/.test(role) || typeof pubkey !== 'string' || pubkey.length < 32 || pubkey.length > 44) fail('public_wallet_input');
    optionalBalances[role] = { pubkey, lamports: String(await rpc('getBalance', [pubkey, { commitment: 'finalized' }]).then(x => x.value)) };
  }
  return {
    schema: 'alashi.epoch2.launch_preflight.v1', status: dynamic ? 'priced_for_review' : 'fixed_only',
    genesis_hash: genesis, program_id: PROGRAM, program_data: programDataAddress,
    upgrade_authority: programData.authority, deployed_slot: programData.slot,
    candidate_sha256: SOURCE_SHA, rollback_sha256: ROLLBACK_SHA,
    rent_lamports: Object.fromEntries(Object.entries(rents).map(([k,v]) => [k, v.toString()])),
    fixed: { extension_lamports: extension.toString(), six_source_rent_lamports: (6n*rents.source).toString(),
      game_rent_lamports: rents.game.toString(), two_faction_rent_lamports: (2n*rents.faction).toString(),
      oracle_config_rent_lamports: rents.oracle.toString(), chosen_two_entry_escrow_lamports: (2n*ENTRY_FEE).toString(),
      subtotal_lamports: fixed.toString(), temporary_buffer_rent_lamports: rents.buffer.toString(),
      sequential_upgrade_stage_before_buffer_drain_lamports: (extension+rents.buffer).toString(),
      sequential_fixed_peak_if_buffer_drain_confirmed_lamports: (extension+rents.buffer > fixed ? extension+rents.buffer : fixed).toString(),
      peak_with_separate_buffer_before_dynamic_lamports: (fixed+rents.buffer).toString() },
    switchboard_quote_status: quoteStatus,
    switchboard_quote_slot: inputQuote?.observed_slot ?? null,
    switchboard_quote_limits: dynamic ? inputQuote.limits : null,
    dynamic_lamports: dynamic && Object.fromEntries(Object.entries(dynamic).map(([k,v])=>[k,v.toString()])),
    priced_total_excluding_buffer_lamports: dynamicTotal === null ? null : (fixed + dynamicTotal).toString(),
    priced_peak_with_buffer_lamports: dynamicTotal === null ? null : (fixed + rents.buffer + dynamicTotal).toString(),
    public_wallets: optionalBalances,
    gates: ['provider_sdk_seed_slot_compatibility', 'six_configured_funded_sources',
      'human_local_upgrade_authority', 'real_provider_roundtrip', 'program_upgrade_review']
  };
}
async function main(argv = process.argv.slice(2)) {
  if (argv.length !== 2 || argv[0] !== '--input') fail('usage: node tools/epoch2_launch_preflight.cjs --input <public-json-file>');
  const input = JSON.parse(fs.readFileSync(argv[1], 'utf8'));
  const rpc = makeRpc(input.rpc_url);
  const result = await preflight(input, rpc);
  process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
}
if (require.main === module) main().catch(err => { process.stderr.write(`preflight: ${err.message}\n`); process.exitCode = 1; });
module.exports = { preflight, makeRpc, parseProgram, parseProgramData, alphabet58, SIZES, GENESIS, PROGRAM, AUTHORITY };

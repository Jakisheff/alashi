'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const { preflight, makeRpc, parseProgramData, GENESIS, PROGRAM, AUTHORITY, SIZES } = require('./epoch2_launch_preflight.cjs');

const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const SWITCHBOARD_PROGRAM = 'Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2';
const QUEUE = 'EYiAmGSdsQTuCw413V5BzaruWuCCSDgTPtBGvLkXHbe7';
const QUEUE_AUTHORITY = '2KgowxogBrGqRcgXQEmqFvC3PGtCu66qERNJevYW8Ajh';
const SOURCE_AUTHORITY = '9jwphYLsGNr9wpewaSMfPVwCa3g8hgq9naQUzgdmjvMo';
const SOURCES = ['1','2','3','4','5','6'].map(c=>c.repeat(32));
function decode58(s) {
  let value = 0n;
  for (const c of s) { const n = alphabet.indexOf(c); if (n < 0) throw new Error('base58'); value = value*58n + BigInt(n); }
  let h = value.toString(16); if (h.length % 2) h = '0'+h;
  const raw = Buffer.from(h, 'hex');
  const zeros = s.match(/^1*/)[0].length;
  return Buffer.concat([Buffer.alloc(zeros), raw]).subarray(-32);
}
const programDataAddress = '11111111111111111111111111111111';
function account(tag, size, tail = Buffer.alloc(0), lamports = 0) {
  const data = Buffer.alloc(size); data.writeUInt32LE(tag, 0); tail.copy(data, 4);
  return { owner:'BPFLoaderUpgradeab1e11111111111111111111111', executable: tag === 2,
    lamports, data:[data.toString('base64'),'base64'] };
}
function fixture() {
  const program = account(2, 36, Buffer.alloc(32));
  const data = account(3, SIZES.programDataOld, Buffer.alloc(0), 2359990200);
  const decoded = Buffer.from(data.data[0], 'base64');
  decoded.writeBigUInt64LE(507743508n, 4); decoded[12] = 1; decode58(AUTHORITY).copy(decoded, 13);
  data.data[0] = decoded.toString('base64');
  const rents = { [SIZES.programDataNew]:2694579320, [SIZES.programDataOld]:2359990200,
    [SIZES.buffer]:2694538680, [SIZES.source]:3088640, [SIZES.game]:5267960,
    [SIZES.faction]:1899920, [SIZES.oracle]:3190240 };
  const rpc = async (method, params=[]) => {
    if (method === 'getGenesisHash') return GENESIS;
    if (method === 'getSlot') return 509973600;
    if (method === 'getAccountInfo') return { value: params[0] === PROGRAM ? program : data };
    if (method === 'getMinimumBalanceForRentExemption') return rents[params[0]];
    throw new Error(`unexpected_${method}`);
  };
  const input = { schema:'alashi.epoch2.launch_input.v1', program_id:PROGRAM,
    expected_authority:AUTHORITY, binary_path:'candidate', rollback_path:'rollback' };
  return { input, rpc, verifyFile:()=>true, data,
    expectedHash:crypto.createHash('sha256').update(decoded.subarray(45)).digest('hex') };
}
test('fixed subtotal and separate-buffer peak are exact and read-only', async () => {
  const {input,rpc,verifyFile,expectedHash} = fixture();
  const result = await preflight(input,rpc,verifyFile,expectedHash);
  assert.equal(result.status,'fixed_only');
  assert.equal(result.fixed.subtotal_lamports,'465379000');
  assert.equal(result.fixed.peak_with_separate_buffer_before_dynamic_lamports,'3159917680');
  assert.equal(result.program_data,programDataAddress);
  assert.equal(result.upgrade_authority,AUTHORITY);
  assert.equal(result.deployed_binary_sha256,expectedHash);
});
test('mismatched genesis and deployed authority fail closed', async () => {
  const {input,rpc,verifyFile,data,expectedHash} = fixture();
  await assert.rejects(preflight(input, async (method,args)=>method==='getGenesisHash'?'wrong':rpc(method,args),verifyFile,expectedHash), /devnet_genesis_mismatch/);
  const raw = Buffer.from(data.data[0], 'base64'); raw[13] ^= 1; data.data[0] = raw.toString('base64');
  await assert.rejects(preflight(input,rpc,verifyFile,expectedHash), /deployed_program_drift/);
});
test('unknown dynamic costs cannot become zero or a priced total', async () => {
  const {input,rpc,verifyFile,expectedHash} = fixture();
  input.switchboard={program_id:SWITCHBOARD_PROGRAM,queue:QUEUE,
    queue_authority:QUEUE_AUTHORITY,source_authority:SOURCE_AUTHORITY,sources:SOURCES};
  input.switchboard_quote={schema:'alashi.switchboard_epoch2.quote.v1',status:'unavailable',
    genesis_hash:GENESIS,alashi_program_id:PROGRAM,program_id:SWITCHBOARD_PROGRAM,
    queue:QUEUE,queue_authority:QUEUE_AUTHORITY,source_authority:SOURCE_AUTHORITY,sources:SOURCES,
    release:{candidate_sha256:'31117a3084318f5d9c8dbc3c3f291168545e474728d981f3089a7554c698978d',
      rollback_sha256:'de492e03624449a36e10a9267563ed46bbaf4eb26204f60306f2c5e10020cde2'},
    pinned_sdk:{name:'@switchboard-xyz/on-demand',version:'3.10.6',
      integrity:'sha512-5tP7eEQkJr5aK+5JOZ09Vbsx0jT9jQNRpDTJHLKMc1Ro5jSUfm/9Q5e6voau/U9rPVgFU5S/lUa5/MUfWozURQ==',
      lock_sha256:'630381a34b75758075c3623f628897968751604b23dabe1857c8c8c73f3fc23c'},
    missing:['provider_fee_cap']};
  const result = await preflight(input,rpc,verifyFile,expectedHash);
  assert.equal(result.priced_total_excluding_buffer_lamports,null);
  assert.equal(result.switchboard_quote_status,'unavailable');
  input.switchboard_quote.status='ok';
  await assert.rejects(preflight(input,rpc,verifyFile,expectedHash), /complete_quote_not_supported/);
});
test('caller-authored complete quote cannot turn partial observations into an all-in budget', async () => {
  const {input,rpc,verifyFile,expectedHash} = fixture();
  input.switchboard={program_id:SWITCHBOARD_PROGRAM,queue:QUEUE,
    queue_authority:QUEUE_AUTHORITY,source_authority:SOURCE_AUTHORITY,sources:SOURCES};
  input.switchboard_quote={schema:'alashi.switchboard_epoch2.quote.v1',status:'ok',
    genesis_hash:GENESIS,alashi_program_id:PROGRAM,program_id:SWITCHBOARD_PROGRAM,
    queue:QUEUE,queue_authority:QUEUE_AUTHORITY,source_authority:SOURCE_AUTHORITY,
    sources:[...input.switchboard.sources],
    release:{candidate_sha256:'31117a3084318f5d9c8dbc3c3f291168545e474728d981f3089a7554c698978d',
      rollback_sha256:'de492e03624449a36e10a9267563ed46bbaf4eb26204f60306f2c5e10020cde2'},
    pinned_sdk:{name:'@switchboard-xyz/on-demand',version:'3.10.6',
      integrity:'sha512-5tP7eEQkJr5aK+5JOZ09Vbsx0jT9jQNRpDTJHLKMc1Ro5jSUfm/9Q5e6voau/U9rPVgFU5S/lUa5/MUfWozURQ==',
      lock_sha256:'630381a34b75758075c3623f628897968751604b23dabe1857c8c8c73f3fc23c'},
    observed_slot:509973550,observed_at_utc:new Date().toISOString(),
    costs:{escrow_rent_lamports:'100',lut_rent_lamports:'200',provider_fee_cap_lamports:'500',
      provider_prefund_lamports:'1000',aggregate_tx_fee_cap_lamports:'300'},
    limits:{tx_count_max:10,signatures_max:20,retries_max:2,bootstrap_attempts_max:6,
      compute_units_per_tx_max:1400000,priority_fee_micro_lamports_per_cu_max:0},
    evidence:['mock sdk instructions and fee quote'],missing:[]};
  await assert.rejects(preflight(input,rpc,verifyFile,expectedHash),/complete_quote_not_supported/);
  input.switchboard_quote.pinned_sdk.lock_sha256='630381a34b75758075c3623f628897968751604b23c';
  await assert.rejects(preflight(input,rpc,verifyFile,expectedHash),/switchboard_quote_binding/);
});
test('program data parser rejects malformed account', () => {
  assert.throws(()=>parseProgramData(null,PROGRAM),/owner_or_encoding/);
});
test('preflight RPC refuses credential-bearing or nonpublic endpoints', () => {
  for (const url of ['http://api.devnet.solana.com','https://name:secret@api.devnet.solana.com',
    'https://api.devnet.solana.com/?api-key=secret','https://private.example/rpc']) {
    assert.throws(()=>makeRpc(url),/public_devnet_rpc_required/);
  }
});

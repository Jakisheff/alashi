import assert from 'node:assert/strict';
import test from 'node:test';
import { quote } from './switchboard_epoch2_quote.mjs';

class PublicKey {
  constructor(value) { if (!/^K[0-9]{2}$/.test(value) && value !== '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC') throw new Error('key'); this.value = value; }
  toBase58() { return this.value; }
}
class TransactionMessage {
  constructor({ payerKey, instructions }) { this.payerKey = payerKey; this.instructions = instructions; }
  compileToV0Message() { return { header: { numRequiredSignatures: 2 }, staticAccountKeys: [this.payerKey, this.instructions[0].keys[0].pubkey], serialize: () => Buffer.alloc(100) }; }
}
class VersionedTransaction { constructor(message) { this.message = message; } }
const web3 = { PublicKey, TransactionMessage, VersionedTransaction };
class Queue {
  constructor(_program, _key) {}
  async loadData() { return { authority: new PublicKey('K03'), oracleKeysLen: 1, oracleKeys: [new PublicKey('K04')] }; }
}
const sb = {
  ON_DEMAND_DEVNET_PID: new PublicKey('K00'), __package_version: '3.10.6', Queue,
  AnchorUtils: { loadProgramFromConnection: async () => ({ coder: {
    accounts: { decode: async () => ({ seedSlot: { toString: () => '99' }, revealSlot: { toString: () => '0' }, authority: new PublicKey('K01') }) },
    instruction: { decode: () => ({ data: { params: { recentSlot: { toString: () => '98' } } } }) },
  } }) },
  Randomness: { create: async (_program, kp) => [{ commitIx: async () => ({ keys: [] }) }, { keys: [
    { pubkey: kp.publicKey, isSigner: true, isWritable: true },
    { pubkey: new PublicKey('K01'), isSigner: true, isWritable: true },
    ...Array.from({ length: 11 }, (_, index) => ({ pubkey: new PublicKey(`K${String(index + 2).padStart(2, '0')}`), isSigner: false, isWritable: true })),
  ], data: Buffer.alloc(8) }] },
};
const connection = {
  getGenesisHash: async () => 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG', getSlot: async () => 42,
  getAccountInfo: async () => ({ executable: true, owner: new PublicKey('K00') }), getLatestBlockhash: async () => ({ blockhash: 'blockhash' }),
  getFeeForMessage: async () => ({ value: 5000 }),
};
const base = () => ({
  schema: 'alashi.switchboard_epoch2.quote.v1', rpc_url: 'https://api.devnet.solana.com', payer: 'K01',
  alashi_program_id: '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC', program_id: 'K00', queue: 'K02', queue_authority: 'K03',
  sources: ['K04','K05','K06','K07','K08','K09'],
  release: { candidate_sha256: '31117a3084318f5d9c8dbc3c3f291168545e474728d981f3089a7554c698978d', rollback_sha256: 'de492e03624449a36e10a9267563ed46bbaf4eb26204f60306f2c5e10020cde2' },
  pinned_sdk: { lock_sha256: '630381a34b75758075c3623f628897968751604b23dabe1857c8c8c73f3fc23c' },
});

test('builds six unsigned init messages but remains unavailable without machine-bound lifecycle evidence', async () => {
  const result = await quote(base(), { web3, sb, connection, now: () => new Date('2026-10-11T00:00:00Z') });
  assert.equal(result.status, 'unavailable');
  assert.equal(result.alashi_program_id, base().alashi_program_id);
  assert.equal(result.queue_authority, 'K03');
  assert.equal(result.source_authority, 'K01');
  assert.equal(result.operations.creation_messages.length, 6);
  assert.deepEqual(result.operations.creation_messages[0].required_signer_pubkeys, ['K01', 'K04']);
  assert.equal(result.operations.creation_messages[0].fee_lamports, '5000');
  assert.ok(result.missing.includes('verified_costs.aggregate_tx_fee_cap_lamports'));
  assert.ok(result.missing.includes('gateway_health'));
});

test('caller-supplied costs and limits cannot turn an unverified quote into ok', async () => {
  const input = base();
  input.measured_costs = Object.fromEntries(['source_rent_lamports','escrow_rent_lamports','lut_rent_lamports','provider_fee_cap_lamports','provider_prefund_lamports','aggregate_tx_fee_cap_lamports','temporary_buffer_lamports'].map((field) => [field, '1']));
  input.limits = { tx_count_max: 18, signatures_max: 24, retries_max: 0, bootstrap_attempts_max: 6, compute_units_per_tx_max: 1400000, priority_fee_micro_lamports_per_cu_max: 1000000 };
  const result = await quote(input, { web3, sb, connection });
  assert.equal(result.status, 'unavailable');
  assert.equal(result.operator_supplied_costs_unverified.source_rent_lamports, '1');
  assert.ok(result.missing.includes('verified_costs.source_rent_lamports'));
  assert.ok(result.missing.includes('verified_limits.tx_count_max'));
});

test('records missing source creation fee when RPC cannot price an unsigned message', async () => {
  const nullFee = { ...connection, getFeeForMessage: async () => ({ value: null }) };
  const result = await quote(base(), { web3, sb, connection: nullFee });
  assert.equal(result.status, 'unavailable');
  assert.ok(result.missing.includes('source_creation_fee_lamports'));
});

test('rejects an arbitrary HTTPS endpoint before RPC work', async () => {
  const input = base(); input.rpc_url = 'https://rpc.example';
  await assert.rejects(() => quote(input, { web3, sb, connection }), /rpc_url_must_be_public_devnet/);
});

test('rejects a non-devnet genesis hash', async () => {
  const badGenesis = { ...connection, getGenesisHash: async () => 'wrong-genesis' };
  await assert.rejects(() => quote(base(), { web3, sb, connection: badGenesis }), /devnet_genesis_mismatch/);
});

test('rejects duplicate source ids before RPC work', async () => {
  const input = base(); input.sources[5] = input.sources[0];
  await assert.rejects(() => quote(input, { web3, sb, connection }), /sources_must_be_distinct/);
});

test('fails closed when queue owner, decode, or authority binding is invalid', async () => {
  const badOwner = { ...connection, getAccountInfo: async (key) => key.toBase58() === 'K00' ?
    ({ executable: true, owner: new PublicKey('K00') }) : ({ owner: new PublicKey('K01') }) };
  const ownerResult = await quote(base(), { web3, sb, connection: badOwner });
  assert.ok(ownerResult.missing.includes('live_queue_account'));
  class WrongQueue { async loadData() { return { authority: new PublicKey('K01'), oracleKeysLen: 1 }; } }
  const authorityResult = await quote(base(), { web3, sb: { ...sb, Queue: WrongQueue }, connection });
  assert.ok(authorityResult.missing.includes('queue_authority_binding'));
});

test('records a simulated clock-minus-one seed as an incompatible deployment gate', async () => {
  const input = base(); input.simulate_unsigned_create_commit = true;
  const clock = Buffer.alloc(8); clock.writeBigUInt64LE(100n);
  const simulated = {
    ...connection,
    getLatestBlockhash: async () => ({ blockhash: 'blockhash', lastValidBlockHeight: 1 }),
    getBalance: async () => 5_000_000_000,
    simulateTransaction: async () => ({ context: { slot: 101 }, value: { err: null, accounts: [
      { lamports: 3_088_640, data: [Buffer.alloc(480).toString('base64'), 'base64'] },
      { lamports: 1_488_440, data: [Buffer.alloc(165).toString('base64'), 'base64'] },
      { lamports: 1_422_400, data: [Buffer.alloc(80).toString('base64'), 'base64'] },
      { lamports: 4_993_990_520, data: [Buffer.alloc(8).toString('base64'), 'base64'] },
      { lamports: 1, data: [clock.toString('base64'), 'base64'] },
    ] } }),
  };
  const result = await quote(input, { web3, sb, connection: simulated });
  const simulation = result.operations.unsigned_create_commit_simulation;
  assert.equal(simulation.status, 'incompatible', simulation.error);
  assert.equal(simulation.seed_slot_delta, '-1');
  assert.equal(simulation.source_data_bytes, 480);
  assert.equal(simulation.escrow.data_bytes, 165);
  assert.equal(simulation.lut.data_bytes, 80);
  assert.ok(result.missing.includes('deployed_seed_slot_plus_one_compatibility'));
});

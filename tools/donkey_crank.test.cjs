'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { createRequire } = require('node:module');
const core = require('./donkey_crank_core.cjs');
const cli = require('./donkey_crank.cjs');
const spikeRequire = createRequire(path.resolve(__dirname, '../spike/package.json'));
const web3 = spikeRequire('@solana/web3.js');

const u8 = (n) => Buffer.from([n]);
const u16 = (n) => { const b = Buffer.alloc(2); b.writeUInt16LE(n); return b; };
const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32LE(n); return b; };
const u64 = (n) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt(n)); return b; };
const i64 = (n) => { const b = Buffer.alloc(8); b.writeBigInt64LE(BigInt(n)); return b; };
const k = (n) => Buffer.alloc(32, n);
const hex = (bytes) => bytes.toString('hex');

function gameBytes() {
  return Buffer.concat([
    core.discriminator('account', 'Game'), k(9), u64(42), u8(2), u8(2), i64(100), u8(2),
    u16(0), u16(0), u64(50), u16(0), i64(20), u8(0), u32(0), u32(0),
    u8(1), u8(255), u8(0), u8(0), k(8),
    u16(0), u8(0), u8(0), u8(0), u16(0), u8(0), u8(0), u8(0),
    u8(0), u8(1), u8(0), u8(2), u8(0), u8(0), u8(0),
    u64(0), u64(0), k(0), u8(0), u32(0), u64(0), k(4), u64(201), u8(0), u64(0),
  ]);
}

function oracleBytes() {
  return Buffer.concat([
    core.discriminator('account', 'DonkeyOracleConfig'), k(1), k(2), k(3),
    ...[10, 11, 12, 13, 14, 15].map(k), u8(2), u8(2),
    ...[20, 21, 0, 0, 0, 0].map(k), u64(199), u8(0), u8(254),
  ]);
}

test('account decoders preserve epoch2 phase, future pin and ordered tickets', () => {
  const game = core.decodeGame(gameBytes(), hex);
  const oracle = core.decodeOracle(oracleBytes(), hex);
  assert.equal(game.phase, 'Action');
  assert.equal(game.epoch, 2);
  assert.equal(game.lawCard, 255);
  assert.equal(game.gameId, 42n);
  assert.equal(game.commitSlot, 201n);
  assert.equal(oracle.ticketCount, 2);
  assert.deepEqual(oracle.tickets, [hex(k(20)), hex(k(21))]);
  assert.throws(() => core.decodeGame(gameBytes().subarray(0, 100), hex), /truncated_account/);
});

test('one-shot plan never recommits or reveals a changed source', () => {
  const game = { phase: 'Action', phaseEndsAt: 100n, round: 2, factionCount: 2,
    epoch: 2, entropyMode: 1, lawCard: 255, vrfAccount: 's2', commitSlot: 201n };
  const oracle = { game: 'g', queue: 'q', authority: 'a',
    accounts: ['s1', 's2', 's3', 's4', 's5', 's6'], round: 2,
    resolved: false, maxAttemptSlot: 199n, tickets: ['f1'] };
  const source = { game: 'g', address: 's2', queue: 'q', authority: 'a',
    seedSlot: 0n, revealSlot: 0n };
  assert.equal(core.planStep(game, oracle, source, ['f1', 'f2'], 99n, 200n).kind,
    'wait_action_deadline');
  assert.equal(core.planStep(game, oracle, source, ['f1', 'f2'], 100n, 200n).kind,
    'commit_and_advance');
  assert.throws(() => core.planStep(game, oracle,
    { ...source, seedSlot: 201n }, ['f1', 'f2'], 100n, 200n),
  /source_precommitted_or_revealed/);
  const law = { ...game, phase: 'Law' };
  assert.equal(core.planStep(law, oracle,
    { ...source, seedSlot: 201n }, ['f1', 'f2'], 101n, 201n).kind,
  'wait_seed_slot');
  assert.deepEqual(core.planStep(law, oracle,
    { ...source, seedSlot: 201n }, ['f1', 'f2'], 101n, 202n),
  { kind: 'reveal_and_resolve', tickets: ['f1'] });
  assert.throws(() => core.planStep(law, oracle,
    { ...source, seedSlot: 203n }, ['f1', 'f2'], 101n, 204n),
  /pinned_commit_changed/);
  assert.throws(() => core.planStep(law, oracle,
    { ...source, seedSlot: 201n, revealSlot: 202n }, ['f1', 'f2'], 101n, 203n),
  /provider_revealed_without_alashi/);
  assert.throws(() => core.planStep(law,
    { ...oracle, tickets: ['f2', 'f3'] },
    { ...source, seedSlot: 201n }, ['f1', 'f2'], 101n, 202n),
  /invalid_ticket_set/);
});

test('technical abort is epoch2-only and starts after fixed Action grace', () => {
  const game = { epoch: 2, entropyMode: 1, phase: 'Action', phaseEndsAt: 100n };
  assert.equal(core.abortUncommittedEligible(game, 129n), false);
  assert.equal(core.abortUncommittedEligible(game, 130n), true);
  assert.equal(core.abortUncommittedEligible({ ...game, epoch: 1 }, 130n), false);
  assert.equal(core.abortUncommittedEligible({ ...game, phase: 'Law' }, 130n), false);
  assert.equal(core.planWithoutSource(game, 129n).kind, 'wait_provider_source');
  assert.equal(core.planWithoutSource(game, 130n).kind, 'abort_uncommitted');
  assert.equal(core.planWithoutSource({ ...game, phase: 'Market' }, 130n).kind,
    'no_operator_step');
  assert.equal(core.planWithoutSource({ ...game, phase: 'Aborted' }, 130n).kind,
    'terminal');
});

test('provider and Alashi instructions are always in one ordered bundle', () => {
  const providerIx = { programId: { toString: () => 'switchboard' } };
  const gameIx = { programId: { toString: () => 'alashi' } };
  assert.deepEqual(core.orderedBundle('commit_and_advance', providerIx, gameIx,
    'switchboard', 'alashi'), [providerIx, gameIx]);
  assert.deepEqual(core.orderedBundle('reveal_and_resolve', providerIx, gameIx,
    'switchboard', 'alashi'), [providerIx, gameIx]);
  assert.throws(() => core.orderedBundle('commit_and_advance', gameIx, providerIx,
    'switchboard', 'alashi'), /invalid_atomic_bundle/);
});

test('mock RPC preserves exact signed transaction across lost responses', async () => {
  const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'alashi-crank-test-'));
  fs.chmodSync(parent, 0o700);
  const stateFile = cli.privateStatePath(path.join(parent, 'pending.json'));
  try {
    const signer = web3.Keypair.generate();
    const blockhash = web3.Keypair.generate().publicKey.toBase58();
    const sent = [];
    const connection = {
      getLatestBlockhash: async () => ({ blockhash }),
      sendRawTransaction: async (raw) => {
        assert.equal(fs.statSync(stateFile).mode & 0o777, 0o600);
        sent.push(Buffer.from(raw));
        if (sent.length === 1) throw new Error('mock_lost_response');
        return 'ignored_mock_receipt';
      },
      getSignatureStatuses: async () => ({ value: [null] }),
      isBlockhashValid: async () => ({ value: true }),
    };
    const ix = new web3.TransactionInstruction({
      programId: web3.SystemProgram.programId, keys: [], data: Buffer.alloc(0),
    });
    const first = await cli.sendRecorded(connection, web3,
      { game_pda: 'mock-game' }, signer, 'commit_and_advance', [ix, ix], stateFile);
    assert.equal(first.status, 'submission_ambiguous');
    assert.equal(await cli.resumePending(connection, web3, stateFile, 'mock-game'),
      'same_signature_resubmitted_or_pending');
    assert.deepEqual(sent[0], sent[1]);
    assert.equal(JSON.parse(fs.readFileSync(stateFile)).signature, first.signature);
    connection.getSignatureStatuses = async () =>
      ({ value: [{ err: null, confirmationStatus: 'confirmed' }] });
    assert.equal(await cli.resumePending(connection, web3, stateFile, 'mock-game'), 'confirmed');
    assert.equal(fs.existsSync(stateFile), false);
  } finally {
    fs.rmSync(parent, { recursive: true, force: true });
  }
});

test('bounded worker polls, stops at terminal phase, and never exceeds budget', async () => {
  let time = 0;
  let calls = 0;
  const clock = { now: () => time, sleep: async (ms) => { time += ms; },
    maxMs: 5000, stopped: () => false };
  const result = await cli.runBounded(async () => {
    calls++;
    return { status: 'no_operator_step', phase: calls === 3 ? 'Finished' : 'Market',
      game_pda: 'mock-game' };
  }, clock);
  assert.equal(result.phase, 'Finished');
  assert.equal(calls, 3);
  assert.equal(time, 4000);
  calls = 0;
  time = 0;
  const expired = await cli.runBounded(async () => {
    calls++;
    return { status: 'wait_action_deadline', game_pda: 'mock-game' };
  }, { ...clock, maxMs: 2500 });
  assert.equal(expired.status, 'time_budget_exhausted');
  assert.equal(calls, 2);
  assert.equal(time, 2500);
});

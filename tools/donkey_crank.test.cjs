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
    ...[10, 11, 12, 13, 14, 15].map(k),
    ...[30, 31, 32, 33, 34, 35].map(k), u8(0), u8(2), u8(2),
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

test('snapshot loader accepts pristine Action oracle and bound Law oracle only', async () => {
  const program = new web3.PublicKey('3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC');
  const provider = new web3.PublicKey('Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2');
  const gameId = Buffer.alloc(8);
  gameId.writeBigUInt64LE(42n);
  const game = web3.PublicKey.findProgramAddressSync([Buffer.from('game'), gameId], program)[0];
  const configKey = web3.PublicKey.findProgramAddressSync(
    [Buffer.from('donkey-oracles'), game.toBuffer()], program)[0];
  const queue = new web3.PublicKey(k(2));
  const sources = [10, 11, 12, 13, 14, 15].map((n) => new web3.PublicKey(k(n)));
  const oracles = [30, 31, 32, 33, 34, 35].map((n) => new web3.PublicKey(k(n)));
  const wallets = [40, 41].map((n) => new web3.PublicKey(k(n)));
  const factions = wallets.map((wallet) => web3.PublicKey.findProgramAddressSync(
    [Buffer.from('faction'), game.toBuffer(), wallet.toBuffer()], program)[0]);
  const gameData = Buffer.from(gameBytes());
  const configData = Buffer.from(oracleBytes());
  game.toBuffer().copy(configData, 8);
  queue.toBuffer().copy(configData, 40);
  configKey.toBuffer().copy(configData, 72);
  const accounts = new Map([
    [game.toBase58(), { owner: program, data: gameData }],
    [configKey.toBase58(), { owner: program, data: configData }],
    [sources[1].toBase58(), { owner: provider, data: Buffer.alloc(480) }],
    ...factions.map((faction, i) => [faction.toBase58(), { owner: program,
      data: Buffer.concat([core.discriminator('account', 'Faction'), game.toBuffer(),
        wallets[i].toBuffer()]) }]),
  ]);
  const connection = { getAccountInfo: async (pubkey) => accounts.get(pubkey.toBase58()) };
  let sourceOracle = web3.PublicKey.default;
  let seedSlot = 0n;
  const sb = { ON_DEMAND_DEVNET_PID: provider,
    AnchorUtils: { loadProgramFromConnection: async () => ({}) },
    Randomness: class { async loadData() { return { queue, authority: configKey,
      oracle: sourceOracle, seedSlot, revealSlot: 0n }; } } };
  const request = { game_pda: game.toBase58(), faction_pdas: factions.map(String),
    round_sources: sources.map(String), round_oracles: oracles.map(String) };
  const clock = { slot: 200n, unixTime: 100n };
  const snapshot = () => cli.loadSnapshot(connection, web3, sb, request, clock);
  assert.equal((await snapshot()).source.oracle, core.UNASSIGNED_ORACLE);
  sourceOracle = oracles[1];
  await assert.rejects(snapshot(), /source_binding_invalid/);
  gameData[48] = 3; // Law after a confirmed Alashi-owned commit.
  sources[1].toBuffer().copy(gameData, gameData.length - 49); // Pinned source.
  configData[488] = 2; // Round-two committed bit.
  seedSlot = 201n;
  assert.equal((await snapshot()).source.oracle, oracles[1].toBase58());
  sourceOracle = web3.PublicKey.default;
  await assert.rejects(snapshot(), /source_binding_invalid/);
});

test('one-shot plan never recommits or reveals a changed source', () => {
  const game = { phase: 'Action', phaseEndsAt: 100n, round: 2, factionCount: 2,
    epoch: 2, entropyMode: 1, lawCard: 255, vrfAccount: 's2', commitSlot: 201n };
  const oracle = { game: 'g', queue: 'q', authority: 'a',
    accounts: ['s1', 's2', 's3', 's4', 's5', 's6'],
    oracles: ['o1', 'o2', 'o3', 'o4', 'o5', 'o6'], committedRoundsMask: 0, round: 2,
    resolved: false, maxAttemptSlot: 199n, tickets: ['f1'] };
  const source = { game: 'g', address: 's2', queue: 'q', authority: 'a',
    oracle: core.UNASSIGNED_ORACLE,
    seedSlot: 0n, revealSlot: 0n };
  assert.equal(core.planStep(game, oracle, source, ['f1', 'f2'], 99n, 200n).kind,
    'wait_action_deadline');
  assert.equal(core.planStep(game, oracle, source, ['f1', 'f2'], 100n, 200n).kind,
    'advance_with_alashi_commit');
  assert.throws(() => core.planStep(game, oracle,
    { ...source, seedSlot: 201n }, ['f1', 'f2'], 100n, 200n),
  /source_precommitted_or_revealed/);
  assert.throws(() => core.planStep(game, oracle,
    { ...source, oracle: 'o2' }, ['f1', 'f2'], 100n, 200n),
  /source_oracle_not_unassigned/);
  const law = { ...game, phase: 'Law' };
  assert.equal(core.planStep(law, oracle,
    { ...source, oracle: 'o2', seedSlot: 201n }, ['f1', 'f2'], 101n, 201n).kind,
  'wait_seed_slot');
  assert.deepEqual(core.planStep(law, oracle,
    { ...source, oracle: 'o2', seedSlot: 201n }, ['f1', 'f2'], 101n, 202n),
  { kind: 'reveal_and_resolve', tickets: ['f1'] });
  assert.throws(() => core.planStep(law, oracle,
    { ...source, oracle: 'o2', seedSlot: 203n }, ['f1', 'f2'], 101n, 204n),
  /pinned_commit_changed/);
  assert.throws(() => core.planStep(law, oracle,
    { ...source, oracle: 'o2', seedSlot: 201n, revealSlot: 202n }, ['f1', 'f2'], 101n, 203n),
  /provider_revealed_without_alashi/);
  assert.throws(() => core.planStep(law,
    { ...oracle, tickets: ['f2', 'f3'] },
    { ...source, oracle: 'o2', seedSlot: 201n }, ['f1', 'f2'], 101n, 202n),
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

test('reveal plan waits for a future committed source before one Alashi CPI instruction', () => {
  const game = { phase: 'Law', epoch: 2, entropyMode: 1, round: 1, factionCount: 1,
    lawCard: 255, vrfAccount: 'source', commitSlot: 8n };
  const oracle = { game: 'game', queue: 'queue', authority: 'authority',
    accounts: ['source', 's2', 's3', 's4', 's5', 's6'],
    oracles: ['oracle', 'o2', 'o3', 'o4', 'o5', 'o6'], committedRoundsMask: 1,
    round: 1, resolved: false, maxAttemptSlot: 0n, tickets: [] };
  const source = { game: 'game', address: 'source', queue: 'queue', authority: 'authority',
    oracle: 'oracle', seedSlot: 8n, revealSlot: 0n };
  assert.equal(core.planStep(game, oracle, source, ['faction'], 1n, 9n).kind,
    'reveal_and_resolve');
});

test('epoch2 reveal accepts only one Alashi instruction, never an external provider reveal', () => {
  const gameIx = { programId: { toString: () => 'alashi' } };
  const providerIx = { programId: { toString: () => 'switchboard' } };
  assert.deepEqual(core.singleAlashiReveal('reveal_and_resolve', gameIx, 'alashi'), [gameIx]);
  assert.throws(() => core.singleAlashiReveal('advance_with_alashi_commit', gameIx, 'alashi'),
    /invalid_alashi_reveal/);
  assert.throws(() => core.singleAlashiReveal('reveal_and_resolve', providerIx, 'alashi'),
    /invalid_alashi_reveal/);
});

test('RevealDonkeyLaw uses the frozen Alashi CPI account order and proof payload', () => {
  const signer = web3.Keypair.generate().publicKey;
  const game = web3.Keypair.generate().publicKey;
  const source = web3.Keypair.generate().publicKey;
  const config = web3.Keypair.generate().publicKey;
  const queue = web3.Keypair.generate().publicKey;
  const oracle = web3.Keypair.generate().publicKey;
  const factions = [web3.Keypair.generate().publicKey, web3.Keypair.generate().publicKey];
  const provider = new web3.PublicKey('Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2');
  const snapshot = {
    gameKey: game,
    sourceKey: source,
    game: { round: 1 },
    source: { address: source.toBase58() },
    oracleKey: config,
    oracle: { queue: queue.toBase58(), oracles: [oracle.toBase58()],
      tickets: [factions[1].toBase58(), factions[0].toBase58()] },
    factionKeys: factions,
  };
  const proof = { signature: Buffer.alloc(64, 7), recoveryId: 1, value: Buffer.alloc(32, 9) };
  const ix = cli.revealDonkeyLawIx(web3, signer, snapshot, proof, provider);
  const stats = web3.PublicKey.findProgramAddressSync(
    [Buffer.from('OracleRandomnessStats'), oracle.toBuffer()], provider)[0];
  const state = web3.PublicKey.findProgramAddressSync([Buffer.from('STATE')], provider)[0];
  const escrow = web3.PublicKey.findProgramAddressSync([
    source.toBuffer(), new web3.PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA').toBuffer(),
    new web3.PublicKey('So11111111111111111111111111111111111111112').toBuffer(),
  ], new web3.PublicKey('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'))[0];
  assert.equal(ix.programId.toBase58(), '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC');
  assert.deepEqual(ix.keys.map((meta) => meta.pubkey.toBase58()), [
    signer, game, source, oracle, queue, stats, web3.SYSVAR_SLOT_HASHES_PUBKEY,
    web3.SystemProgram.programId, escrow,
    new web3.PublicKey('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'),
    new web3.PublicKey('So11111111111111111111111111111111111111112'), state, provider, config,
    factions[1], factions[0],
  ].map((pubkey) => pubkey.toBase58()));
  assert.deepEqual(ix.keys.map((meta) => [meta.isSigner, meta.isWritable]), [
    [true, true], [false, true], [false, true], [false, false], [false, false],
    [false, true], [false, false], [false, false], [false, true], [false, false],
    [false, false], [false, false], [false, false], [false, true], [false, true], [false, true],
  ]);
  assert.equal(ix.data.length, 105);
  assert.deepEqual(ix.data.subarray(0, 8), core.discriminator('global', 'reveal_donkey_law'));
  assert.deepEqual(ix.data.subarray(8, 72), proof.signature);
  assert.equal(ix.data[72], 1);
  assert.deepEqual(ix.data.subarray(73), proof.value);
});

test('reveal proof file is exact base64 and owner-only', () => {
  const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'alashi-proof-test-'));
  fs.chmodSync(parent, 0o700);
  const file = path.join(parent, 'proof.json');
  try {
    fs.writeFileSync(file, JSON.stringify({
      signature_base64: Buffer.alloc(64, 3).toString('base64'), recovery_id: 2,
      value_base64: Buffer.alloc(32, 4).toString('base64'),
    }), { mode: 0o600 });
    const proof = cli.readRevealProof(file);
    assert.equal(proof.recoveryId, 2);
    assert.equal(proof.signature.length, 64);
    fs.chmodSync(file, 0o644);
    assert.throws(() => cli.readRevealProof(file), /reveal_proof_file_must_be_0600/);
    fs.chmodSync(file, 0o600);
    fs.writeFileSync(file, JSON.stringify({ signature_base64: 'bad', recovery_id: 0,
      value_base64: Buffer.alloc(32).toString('base64') }));
    assert.throws(() => cli.readRevealProof(file), /invalid_reveal_proof/);
  } finally {
    fs.rmSync(parent, { recursive: true, force: true });
  }
});


test('Alashi Advance owns epoch2 commit with the exact frozen remaining-account order', () => {
  const signer = web3.Keypair.generate().publicKey;
  const game = web3.Keypair.generate().publicKey;
  const factions = [web3.Keypair.generate().publicKey, web3.Keypair.generate().publicKey];
  const config = web3.Keypair.generate().publicKey;
  const source = web3.Keypair.generate().publicKey;
  const queue = web3.Keypair.generate().publicKey;
  const selectedOracle = web3.Keypair.generate().publicKey;
  const provider = new web3.PublicKey('Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2');
  const ix = cli.advanceIx(web3, signer, game, factions, config, source, queue, selectedOracle, provider);
  assert.deepEqual(ix.keys.map((meta) => meta.pubkey.toBase58()), [
    signer, game, web3.SYSVAR_SLOT_HASHES_PUBKEY, ...factions, config, source, queue,
    selectedOracle, provider,
  ].map((pubkey) => pubkey.toBase58()));
  assert.equal(ix.keys[5].isWritable, true); // config
  assert.equal(ix.keys[6].isWritable, true); // source
  assert.equal(ix.keys[7].isWritable, false); // queue
  assert.equal(ix.keys[8].isWritable, true); // selected oracle
  assert.equal(ix.keys[9].isWritable, false); // provider executable
  assert.throws(() => cli.advanceIx(web3, signer, game, factions, config, source),
    /incomplete_alashi_commit_accounts/);
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
      { game_pda: 'mock-game' }, signer, 'reveal_donkey_law', [ix], stateFile);
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

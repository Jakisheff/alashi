#!/usr/bin/env node
'use strict';

// Bounded operator steps. No wallet, account creation, faucet, or implicit
// retry with a new signature is built into this tool.
const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const core = require('./donkey_crank_core.cjs');
const spikeRequire = createRequire(path.resolve(__dirname, '../spike/package.json'));

const ALASHI_PID = '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC';
const TOKEN_PROGRAM_ID = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';
const NATIVE_MINT = 'So11111111111111111111111111111111111111112';
const ATA_PROGRAM_ID = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';
const MODES = new Set(['inspect', 'step', 'worker']);

function fail(code) { throw new Error(code); }
function key(web3, value) { return new web3.PublicKey(value); }
function same(a, b) { return String(a) === String(b); }
function bignum(value) { return BigInt(value.toString()); }

function readJson(file) {
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink()) fail('unsafe_config_file');
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function readSigner(web3, file) {
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink() || (stat.mode & 0o077) !== 0) {
    fail('signer_file_must_be_0600');
  }
  const bytes = JSON.parse(fs.readFileSync(file, 'utf8'));
  if (!Array.isArray(bytes) || bytes.length !== 64 ||
      bytes.some((value) => !Number.isInteger(value) || value < 0 || value > 255)) {
    fail('invalid_signer_file');
  }
  return web3.Keypair.fromSecretKey(Uint8Array.from(bytes));
}

function strictBase64(value, length) {
  if (typeof value !== 'string' || value.length === 0) fail('invalid_reveal_proof');
  const decoded = Buffer.from(value, 'base64');
  if (decoded.length !== length || decoded.toString('base64') !== value) {
    fail('invalid_reveal_proof');
  }
  return decoded;
}

function readRevealProof(file) {
  const stat = fs.lstatSync(file);
  if (!stat.isFile() || stat.isSymbolicLink() || (stat.mode & 0o077) !== 0) {
    fail('reveal_proof_file_must_be_0600');
  }
  let parsed;
  try { parsed = JSON.parse(fs.readFileSync(file, 'utf8')); }
  catch { fail('invalid_reveal_proof'); }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed) ||
      Object.keys(parsed).sort().join(',') !== 'recovery_id,signature_base64,value_base64' ||
      !Number.isInteger(parsed.recovery_id) || parsed.recovery_id < 0 || parsed.recovery_id > 255) {
    fail('invalid_reveal_proof');
  }
  return {
    signature: strictBase64(parsed.signature_base64, 64),
    recoveryId: parsed.recovery_id,
    value: strictBase64(parsed.value_base64, 32),
  };
}

function privateStatePath(file) {
  const absolute = path.resolve(file);
  const checkout = path.resolve(__dirname, '..') + path.sep;
  if (absolute.startsWith(checkout)) fail('state_file_inside_checkout');
  const parent = fs.lstatSync(path.dirname(absolute));
  if (!parent.isDirectory() || parent.isSymbolicLink() || (parent.mode & 0o077) !== 0) {
    fail('state_directory_must_be_0700');
  }
  if (fs.existsSync(absolute)) {
    const stat = fs.lstatSync(absolute);
    if (!stat.isFile() || stat.isSymbolicLink() || (stat.mode & 0o077) !== 0) {
      fail('state_file_must_be_0600');
    }
  }
  return absolute;
}

function writePrivateState(file, state) {
  const temp = `${file}.${process.pid}.tmp`;
  const fd = fs.openSync(temp, 'wx', 0o600);
  try {
    fs.writeFileSync(fd, JSON.stringify(state));
    fs.fsyncSync(fd);
  } finally { fs.closeSync(fd); }
  fs.renameSync(temp, file);
  const parent = fs.openSync(path.dirname(file), 'r');
  try { fs.fsyncSync(parent); } finally { fs.closeSync(parent); }
}

function clearPrivateState(file) {
  if (fs.existsSync(file)) fs.unlinkSync(file);
}

function parseClock(account) {
  if (!account || account.data.length < 40) fail('clock_unavailable');
  return { slot: account.data.readBigUInt64LE(0),
    unixTime: account.data.readBigInt64LE(32) };
}

function instruction(web3, name, keys) {
  return new web3.TransactionInstruction({
    programId: key(web3, ALASHI_PID),
    keys,
    data: core.discriminator('global', name),
  });
}

function advanceIx(web3, signer, game, factions, oracle, source, queue, selectedOracle, provider) {
  const keys = [
    { pubkey: signer, isSigner: true, isWritable: false },
    { pubkey: game, isSigner: false, isWritable: true },
    { pubkey: web3.SYSVAR_SLOT_HASHES_PUBKEY, isSigner: false, isWritable: false },
    ...factions.map((pubkey) => ({ pubkey, isSigner: false, isWritable: true })),
    { pubkey: oracle, isSigner: false, isWritable: true },
  ];
  if (source || queue || selectedOracle || provider) {
    if (!source || !queue || !selectedOracle || !provider) fail('incomplete_alashi_commit_accounts');
    keys.push(
      { pubkey: source, isSigner: false, isWritable: true },
      { pubkey: queue, isSigner: false, isWritable: false },
      { pubkey: selectedOracle, isSigner: false, isWritable: true },
      { pubkey: provider, isSigner: false, isWritable: false },
    );
  }
  return instruction(web3, 'advance', keys);
}

function revealDonkeyLawIx(web3, signer, snapshot, proof, provider) {
  if (!snapshot.sourceKey || !snapshot.source || !Array.isArray(snapshot.oracle.tickets) ||
      !Buffer.isBuffer(proof.signature) || proof.signature.length !== 64 ||
      !Buffer.isBuffer(proof.value) || proof.value.length !== 32 ||
      !Number.isInteger(proof.recoveryId) || proof.recoveryId < 0 || proof.recoveryId > 255) {
    fail('invalid_reveal_instruction');
  }
  const providerKey = key(web3, provider);
  const oracle = key(web3, snapshot.oracle.oracles[snapshot.game.round - 1]);
  const queue = key(web3, snapshot.oracle.queue);
  const stats = web3.PublicKey.findProgramAddressSync(
    [Buffer.from('OracleRandomnessStats'), oracle.toBuffer()], providerKey)[0];
  const providerState = web3.PublicKey.findProgramAddressSync([Buffer.from('STATE')], providerKey)[0];
  const escrow = web3.PublicKey.findProgramAddressSync([
    snapshot.sourceKey.toBuffer(), key(web3, TOKEN_PROGRAM_ID).toBuffer(), key(web3, NATIVE_MINT).toBuffer(),
  ], key(web3, ATA_PROGRAM_ID))[0];
  const factions = new Map(snapshot.factionKeys.map((faction) => [faction.toBase58(), faction]));
  const due = snapshot.oracle.tickets.map((ticket) => factions.get(ticket));
  if (due.some((faction) => !faction)) fail('invalid_reveal_instruction');
  const keys = [
    { pubkey: signer, isSigner: true, isWritable: true },
    { pubkey: snapshot.gameKey, isSigner: false, isWritable: true },
    { pubkey: snapshot.sourceKey, isSigner: false, isWritable: true },
    { pubkey: oracle, isSigner: false, isWritable: false },
    { pubkey: queue, isSigner: false, isWritable: false },
    { pubkey: stats, isSigner: false, isWritable: true },
    { pubkey: web3.SYSVAR_SLOT_HASHES_PUBKEY, isSigner: false, isWritable: false },
    { pubkey: web3.SystemProgram.programId, isSigner: false, isWritable: false },
    { pubkey: escrow, isSigner: false, isWritable: true },
    { pubkey: key(web3, TOKEN_PROGRAM_ID), isSigner: false, isWritable: false },
    { pubkey: key(web3, NATIVE_MINT), isSigner: false, isWritable: false },
    { pubkey: providerState, isSigner: false, isWritable: false },
    { pubkey: providerKey, isSigner: false, isWritable: false },
    { pubkey: snapshot.oracleKey, isSigner: false, isWritable: true },
    ...due.map((pubkey) => ({ pubkey, isSigner: false, isWritable: true })),
  ];
  return new web3.TransactionInstruction({
    programId: key(web3, ALASHI_PID), keys,
    data: Buffer.concat([
      core.discriminator('global', 'reveal_donkey_law'), proof.signature,
      Buffer.from([proof.recoveryId]), proof.value,
    ]),
  });
}

async function account(connection, pubkey, owner, code) {
  const value = await connection.getAccountInfo(pubkey, 'confirmed');
  if (!value || !same(value.owner, owner)) fail(code);
  return value;
}

async function loadSnapshot(connection, web3, sb, config, clock) {
  const gameKey = key(web3, config.game_pda);
  const programKey = key(web3, ALASHI_PID);
  const oracleKey = web3.PublicKey.findProgramAddressSync(
    [Buffer.from('donkey-oracles'), gameKey.toBuffer()], programKey)[0];
  const mapKey = (bytes) => key(web3, bytes).toBase58();
  const game = core.decodeGame((await account(connection, gameKey, programKey,
    'game_missing_or_wrong_owner')).data, mapKey);
  const gameIdBytes = Buffer.alloc(8);
  gameIdBytes.writeBigUInt64LE(game.gameId);
  const expectedGame = web3.PublicKey.findProgramAddressSync(
    [Buffer.from('game'), gameIdBytes], programKey)[0];
  if (!gameKey.equals(expectedGame) || game.epoch !== 2 || game.entropyMode !== 1) {
    fail('not_canonical_epoch2_game');
  }
  const oracle = core.decodeOracle((await account(connection, oracleKey, programKey,
    'oracle_missing_or_wrong_owner')).data, mapKey);
  if (oracle.game !== config.game_pda || oracle.authority !== oracleKey.toBase58() ||
      !Array.isArray(config.round_sources) || config.round_sources.length !== 6 ||
      !Array.isArray(config.round_oracles) || config.round_oracles.length !== 6 ||
      oracle.accounts.some((value, i) => value !== config.round_sources[i]) ||
      oracle.oracles.some((value, i) => value !== config.round_oracles[i])) {
    fail('configured_sources_mismatch');
  }
  const factionNames = config.faction_pdas;
  if (!Array.isArray(factionNames) || factionNames.length !== game.factionCount ||
      new Set(factionNames).size !== factionNames.length) fail('faction_set_invalid');
  for (const factionName of factionNames) {
    const factionKey = key(web3, factionName);
    const identity = core.decodeFactionIdentity((await account(connection, factionKey,
      programKey, 'faction_missing_or_wrong_owner')).data, mapKey);
    const expected = web3.PublicKey.findProgramAddressSync([
      Buffer.from('faction'), gameKey.toBuffer(), key(web3, identity.wallet).toBuffer(),
    ], programKey)[0];
    if (!factionKey.equals(expected) || identity.game !== config.game_pda) {
      fail('faction_binding_invalid');
    }
  }
  if (game.round < 1 || game.round > 6) fail('invalid_game_state');
  if (game.phase === 'Finished' || game.phase === 'Aborted') {
    return { gameKey, game, oracleKey, oracle, source: null,
      factionKeys: factionNames.map((name) => key(web3, name)) };
  }
  if (game.phase !== 'Action' && game.phase !== 'Law') {
    return { gameKey, game, oracleKey, oracle, source: null,
      factionKeys: factionNames.map((name) => key(web3, name)) };
  }
  const abortEligible = core.abortUncommittedEligible(game, clock.unixTime);
  if (abortEligible) {
    return { gameKey, game, oracleKey, oracle, source: null,
      factionKeys: factionNames.map((name) => key(web3, name)) };
  }
  const sourceKey = key(web3, oracle.accounts[game.round - 1]);
  let randomness;
  let providerData;
  try {
    await account(connection, sourceKey, sb.ON_DEMAND_DEVNET_PID,
      'source_missing_or_wrong_owner');
    const program = await sb.AnchorUtils.loadProgramFromConnection(
      connection, undefined, sb.ON_DEMAND_DEVNET_PID);
    randomness = new sb.Randomness(program, sourceKey);
    providerData = await randomness.loadData();
  } catch (error) {
    if (game.phase !== 'Action') throw error;
    // No provider data can authorize a commit. Keep observing until the
    // bounded grace elapses, then use the source-free technical abort.
    return { gameKey, game, oracleKey, oracle, source: null,
      factionKeys: factionNames.map((name) => key(web3, name)) };
  }
  const source = {
    game: config.game_pda,
    address: sourceKey.toBase58(),
    queue: providerData.queue.toBase58(),
    authority: providerData.authority.toBase58(),
    seedSlot: bignum(providerData.seedSlot),
    revealSlot: bignum(providerData.revealSlot),
    oracle: providerData.oracle.toBase58(),
  };
  const expectedOracle = oracle.oracles[game.round - 1];
  const sourceOracleValid = game.phase === 'Action'
    ? source.oracle === core.UNASSIGNED_ORACLE
    : source.oracle === expectedOracle;
  if (source.queue !== oracle.queue || source.authority !== oracle.authority ||
      source.authority !== oracleKey.toBase58() || !sourceOracleValid) {
    fail('source_binding_invalid');
  }
  return { gameKey, game, oracleKey, oracle, sourceKey, source,
    factionKeys: factionNames.map((name) => key(web3, name)), randomness };
}

async function resumePending(connection, web3, stateFile, gamePda) {
  if (!fs.existsSync(stateFile)) return false;
  const state = readJson(stateFile);
  if (state.game_pda !== gamePda ||
      !['advance_with_alashi_commit', 'abort_uncommitted', 'reveal_donkey_law'].includes(state.kind)) {
    fail('pending_state_binding_invalid');
  }
  const status = (await connection.getSignatureStatuses([state.signature],
    { searchTransactionHistory: true })).value[0];
  if (status?.err) fail('recorded_transaction_rejected');
  if (status?.confirmationStatus === 'confirmed' || status?.confirmationStatus === 'finalized') {
    clearPrivateState(stateFile);
    return 'confirmed';
  }
  const valid = await connection.isBlockhashValid(state.blockhash, 'confirmed');
  if (!valid.value) fail('pending_signature_unresolved_expired');
  const raw = Buffer.from(state.signed_tx, 'base64');
  const tx = web3.VersionedTransaction.deserialize(raw);
  if (tx.signatures[0].length !== 64 ||
      spikeRequire('bs58').encode(tx.signatures[0]) !== state.signature) {
    fail('pending_signature_mismatch');
  }
  try { await connection.sendRawTransaction(raw, { maxRetries: 0 }); }
  catch { /* The same signed transaction remains the only retryable capability. */ }
  return 'same_signature_resubmitted_or_pending';
}

async function sendRecorded(connection, web3, config, signer, kind, instructions, stateFile) {
  const lookupTables = [];
  if (config.lookup_table) {
    const table = await connection.getAddressLookupTable(key(web3, config.lookup_table));
    if (!table.value || table.value.state.deactivationSlot !== 18446744073709551615n) {
      fail('lookup_table_unavailable');
    }
    lookupTables.push(table.value);
  }
  const { blockhash } = await connection.getLatestBlockhash('confirmed');
  const message = new web3.TransactionMessage({
    payerKey: signer.publicKey,
    recentBlockhash: blockhash,
    instructions: [web3.ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 }),
      ...instructions],
  }).compileToV0Message(lookupTables);
  const tx = new web3.VersionedTransaction(message);
  tx.sign([signer]);
  let raw;
  try { raw = tx.serialize(); }
  catch { fail('atomic_transaction_too_large'); }
  const signature = spikeRequire('bs58').encode(tx.signatures[0]);
  writePrivateState(stateFile, { game_pda: config.game_pda, kind, signature,
    blockhash, signed_tx: Buffer.from(raw).toString('base64') });
  try { await connection.sendRawTransaction(raw, { maxRetries: 0 }); }
  catch { return { status: 'submission_ambiguous', signature }; }
  return { status: 'submitted_pending_receipt', signature };
}

async function main(argv) {
  const mode = argv[2] || 'inspect';
  if (!MODES.has(mode) || argv.slice(3).some((arg) => arg !== '--execute') ||
      (mode === 'step' && !argv.includes('--execute')) ||
      (mode === 'inspect' && argv.includes('--execute'))) fail('usage_inspect_or_step_execute');
  const configFile = process.env.ALASHI_CRANK_CONFIG_FILE;
  const rpcUrl = process.env.ALASHI_RPC;
  if (!configFile || !rpcUrl) fail('crank_config_or_rpc_missing');
  const parsedUrl = new URL(rpcUrl);
  if (parsedUrl.protocol !== 'https:' || parsedUrl.username || parsedUrl.password ||
      parsedUrl.search || parsedUrl.hash) fail('unsafe_rpc_url');
  const config = readJson(configFile);
  const web3 = spikeRequire('@solana/web3.js');
  const sb = spikeRequire('@switchboard-xyz/on-demand');
  const connection = new web3.Connection(rpcUrl, 'confirmed');
  if (await connection.getGenesisHash() !== core.DEVNET_GENESIS) fail('not_solana_devnet');
  const program = await connection.getAccountInfo(key(web3, ALASHI_PID), 'confirmed');
  if (!program?.executable) fail('program_not_executable');
  let stateFile;
  if (mode === 'step') {
    if (!process.env.ALASHI_CRANK_STATE_FILE) fail('crank_state_file_missing');
    stateFile = privateStatePath(process.env.ALASHI_CRANK_STATE_FILE);
    const pendingStatus = await resumePending(connection, web3, stateFile, config.game_pda);
    if (pendingStatus) return { status: pendingStatus, game_pda: config.game_pda };
  }
  const clock = parseClock(await connection.getAccountInfo(web3.SYSVAR_CLOCK_PUBKEY, 'confirmed'));
  const snapshot = await loadSnapshot(connection, web3, sb, config, clock);
  let plan;
  if (!snapshot.source) {
    plan = core.planWithoutSource(snapshot.game, clock.unixTime);
  } else {
    try {
      plan = core.planStep(snapshot.game, snapshot.oracle, snapshot.source,
        config.faction_pdas, clock.unixTime, clock.slot);
    } catch (error) {
      if (snapshot.game.phase !== 'Action' ||
          error.message !== 'source_precommitted_or_revealed') throw error;
      plan = { kind: 'wait_provider_source' };
    }
  }
  if (mode === 'inspect') {
    return { status: 'read_only', step: plan.kind, game_pda: config.game_pda,
      round: snapshot.game.round, source: snapshot.source?.address || null };
  }
  if (!['advance_with_alashi_commit', 'abort_uncommitted', 'reveal_and_resolve'].includes(plan.kind)) {
    return { status: plan.kind, game_pda: config.game_pda, round: snapshot.game.round,
      phase: snapshot.game.phase };
  }
  const signerFile = process.env.ALASHI_CRANK_KEY_FILE;
  if (!signerFile) fail('crank_signer_missing');
  const signer = readSigner(web3, signerFile);
  // The source authority is the Alashi config PDA. The operator only pays/signs
  // outer transactions; it cannot substitute a provider authority key.
  let gameIx;
  if (plan.kind === 'abort_uncommitted') {
    gameIx = advanceIx(web3, signer.publicKey, snapshot.gameKey,
      snapshot.factionKeys, snapshot.oracleKey, null);
    return sendRecorded(connection, web3, config, signer,
      'abort_uncommitted', [gameIx], stateFile);
  }
  if (plan.kind === 'advance_with_alashi_commit') {
    gameIx = advanceIx(web3, signer.publicKey, snapshot.gameKey,
      snapshot.factionKeys, snapshot.oracleKey, snapshot.sourceKey,
      key(web3, snapshot.oracle.queue), key(web3, snapshot.oracle.oracles[snapshot.game.round - 1]),
      sb.ON_DEMAND_DEVNET_PID);
    return sendRecorded(connection, web3, config, signer, plan.kind, [gameIx], stateFile);
  }
  if (plan.kind === 'reveal_and_resolve') {
    const proofFile = process.env.ALASHI_REVEAL_PROOF_FILE;
    if (!proofFile) fail('reveal_proof_file_missing');
    const revealIx = revealDonkeyLawIx(web3, signer.publicKey, snapshot,
      readRevealProof(proofFile), sb.ON_DEMAND_DEVNET_PID);
    const instructions = core.singleAlashiReveal(plan.kind, revealIx, ALASHI_PID);
    return sendRecorded(connection, web3, config, signer,
      'reveal_donkey_law', instructions, stateFile);
  }
  fail('invalid_crank_plan');
}

async function runBounded(step, options) {
  const { now, sleep, maxMs, stopped } = options;
  const deadline = now() + maxMs;
  let last = { status: 'time_budget_exhausted' };
  while (!stopped() && now() < deadline) {
    last = await step();
    if (last.phase === 'Finished' || last.phase === 'Aborted') return last;
    if (stopped()) break;
    await sleep(Math.min(2000, Math.max(0, deadline - now())));
  }
  return { status: stopped() ? 'stopped' : 'time_budget_exhausted',
    game_pda: last.game_pda };
}

async function command(argv) {
  const mode = argv[2] || 'inspect';
  if (mode === 'inspect') return main(argv);
  let seconds;
  if (mode === 'worker') {
    seconds = Number(argv[5]);
    if (argv.length !== 6 || argv[3] !== '--execute' || argv[4] !== '--max-seconds' ||
        !Number.isInteger(seconds) || seconds < 1 || seconds > 720) {
      fail('usage_worker_execute_max_seconds_1_to_720');
    }
  }
  const stateFile = process.env.ALASHI_CRANK_STATE_FILE;
  if (!stateFile) fail('crank_state_file_missing');
  const lock = `${privateStatePath(stateFile)}.lock`;
  let fd;
  try { fd = fs.openSync(lock, 'wx', 0o600); }
  catch { fail('crank_already_running_or_stale_lock'); }
  let stop = false;
  const onStop = () => { stop = true; };
  if (mode === 'worker') {
    process.on('SIGINT', onStop);
    process.on('SIGTERM', onStop);
  }
  try {
    if (mode !== 'worker') return await main(argv);
    const result = await runBounded(() => main([argv[0], argv[1], 'step', '--execute']),
      { now: Date.now, sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
        maxMs: seconds * 1000, stopped: () => stop });
    return result;
  } finally {
    if (mode === 'worker') {
      process.off('SIGINT', onStop);
      process.off('SIGTERM', onStop);
    }
    fs.closeSync(fd);
    fs.unlinkSync(lock);
  }
}

if (require.main === module) {
  command(process.argv).then((result) => console.log(JSON.stringify(result)))
    .catch((error) => {
      // No raw SDK/RPC error objects: they can contain endpoint or gateway data.
      const codes = new Set(['usage_inspect_or_step_execute',
        'usage_worker_execute_max_seconds_1_to_720',
        'crank_already_running_or_stale_lock', 'crank_config_or_rpc_missing',
        'unsafe_rpc_url', 'not_solana_devnet', 'program_not_executable',
        'game_missing_or_wrong_owner', 'oracle_missing_or_wrong_owner',
        'not_canonical_epoch2_game', 'configured_sources_mismatch',
        'faction_set_invalid', 'faction_missing_or_wrong_owner', 'faction_binding_invalid',
        'source_missing_or_wrong_owner', 'source_binding_invalid',
        'epoch2_binding_invalid', 'invalid_ticket_set', 'source_precommitted_or_revealed',
        'source_round_already_committed', 'selected_oracle_mismatch', 'incomplete_alashi_commit_accounts',
        'invalid_crank_plan',
        'attempt_slot_not_before_commit', 'pinned_commit_changed',
        'provider_revealed_without_alashi', 'pending_state_binding_invalid',
        'recorded_transaction_rejected', 'pending_signature_unresolved_expired',
        'pending_signature_mismatch', 'lookup_table_unavailable',
        'atomic_transaction_too_large', 'crank_signer_missing',
        'reveal_proof_file_missing', 'reveal_proof_file_must_be_0600',
        'invalid_reveal_proof', 'invalid_reveal_instruction', 'invalid_alashi_reveal',
        'signer_file_must_be_0600', 'invalid_signer_file', 'state_file_inside_checkout',
        'state_directory_must_be_0700', 'state_file_must_be_0600',
        'crank_state_file_missing',
        'unsafe_config_file', 'clock_unavailable', 'invalid_game_state',
        'invalid_oracle_state', 'invalid_offer_count', 'truncated_account',
        'invalid_Game_account', 'invalid_DonkeyOracleConfig_account',
        'invalid_Faction_account', 'invalid_atomic_bundle']);
      console.error(JSON.stringify({ ok: false, error: codes.has(error.message) ? error.message : 'crank_preflight_or_rpc_failed' }));
      process.exitCode = 1;
    });
}

module.exports = { main, command, runBounded, parseClock, advanceIx, revealDonkeyLawIx,
  readRevealProof, privateStatePath, sendRecorded, resumePending, loadSnapshot };

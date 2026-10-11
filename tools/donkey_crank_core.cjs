'use strict';

const { createHash } = require('node:crypto');

const PHASES = ['Lobby', 'Market', 'Action', 'Law', 'Finished', 'Aborted'];
const DEVNET_GENESIS = 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG';
const COMMIT_GRACE_SECONDS = 30n;

function abortUncommittedEligible(game, unixTime) {
  return game.epoch === 2 && game.entropyMode === 1 && game.phase === 'Action' &&
    BigInt(unixTime) >= game.phaseEndsAt + COMMIT_GRACE_SECONDS;
}

function planWithoutSource(game, unixTime) {
  if (game.phase === 'Finished' || game.phase === 'Aborted') return { kind: 'terminal' };
  if (abortUncommittedEligible(game, unixTime)) return { kind: 'abort_uncommitted' };
  if (game.phase === 'Action') return { kind: 'wait_provider_source' };
  return { kind: 'no_operator_step' };
}

function discriminator(kind, name) {
  return createHash('sha256').update(`${kind}:${name}`).digest().subarray(0, 8);
}

class Reader {
  constructor(data, accountName) {
    if (!Buffer.isBuffer(data) || data.length < 8 ||
        !data.subarray(0, 8).equals(discriminator('account', accountName))) {
      throw new Error(`invalid_${accountName}_account`);
    }
    this.data = data;
    this.pos = 8;
  }
  take(count) {
    if (this.pos + count > this.data.length) throw new Error('truncated_account');
    const out = this.data.subarray(this.pos, this.pos + count);
    this.pos += count;
    return out;
  }
  u8() { return this.take(1)[0]; }
  u16() { return this.take(2).readUInt16LE(); }
  u32() { return this.take(4).readUInt32LE(); }
  u64() { return this.take(8).readBigUInt64LE(); }
  i64() { return this.take(8).readBigInt64LE(); }
  key(mapKey) { return mapKey(this.take(32)); }
  skip(count) { this.take(count); }
}

// Mirrors the append-only Game account layout in rules/src/state.rs. It reads
// only the fields required by the operator; a truncated or different layout
// stops before any signing.
function decodeGame(data, mapKey) {
  const r = new Reader(data, 'Game');
  const admin = r.key(mapKey);
  const gameId = r.u64();
  const phaseNumber = r.u8();
  const round = r.u8();
  const phaseEndsAt = r.i64();
  const factionCount = r.u8();
  r.skip(2 + 2 + 8 + 2 + 8 + 1 + 4 + 4 + 1); // through bump
  const lawCard = r.u8();
  r.skip(1 + 1 + 32); // laws_used_mask, veto_pending, president
  r.skip(2 + 1 + 1 + 1 + 2 + 1 + 1 + 1);
  const settled = r.u8();
  const entropyMode = r.u8();
  r.skip(1); // vote_weight_mode
  const epoch = r.u8();
  r.skip(1 + 1 + 1 + 8 + 8 + 32 + 1);
  const offerCount = r.u32();
  if (offerCount > 8) throw new Error('invalid_offer_count');
  r.skip(offerCount * (8 + 32 + 32 + 2 + 8));
  r.skip(8); // barter_next_id
  const vrfAccount = r.key(mapKey);
  const commitSlot = r.u64();
  const vrfRetries = r.u8();
  r.u64(); // vrf_spent; forces complete tail validation
  const phase = PHASES[phaseNumber];
  if (!phase || settled > 1 || factionCount > 6 || round > 6) {
    throw new Error('invalid_game_state');
  }
  return { admin, gameId, phase, round, phaseEndsAt, factionCount, lawCard,
    settled: Boolean(settled), entropyMode, epoch, vrfAccount, commitSlot, vrfRetries };
}

function decodeOracle(data, mapKey) {
  const r = new Reader(data, 'DonkeyOracleConfig');
  const game = r.key(mapKey);
  const queue = r.key(mapKey);
  const authority = r.key(mapKey);
  const accounts = Array.from({ length: 6 }, () => r.key(mapKey));
  const round = r.u8();
  const ticketCount = r.u8();
  const tickets = Array.from({ length: 6 }, () => r.key(mapKey));
  const maxAttemptSlot = r.u64();
  const resolved = r.u8();
  r.u8(); // bump
  if (round > 6 || ticketCount > 6 || resolved > 1 ||
      new Set(accounts).size !== 6) throw new Error('invalid_oracle_state');
  return { game, queue, authority, accounts, round, ticketCount,
    tickets: tickets.slice(0, ticketCount), maxAttemptSlot, resolved: Boolean(resolved) };
}

function decodeFactionIdentity(data, mapKey) {
  const r = new Reader(data, 'Faction');
  return { game: r.key(mapKey), wallet: r.key(mapKey) };
}

function dueTickets(game, oracle, factions) {
  const due = oracle.round === game.round && !oracle.resolved;
  const tickets = due ? oracle.tickets : [];
  if (tickets.length !== new Set(tickets).size ||
      tickets.some((key) => !factions.includes(key))) {
    throw new Error('invalid_ticket_set');
  }
  return tickets;
}

function planStep(game, oracle, source, factions, unixTime, slot) {
  if (game.epoch !== 2 || game.entropyMode !== 1 || game.round < 1 || game.round > 6 ||
      oracle.game !== source.game || oracle.accounts[game.round - 1] !== source.address ||
      oracle.queue !== source.queue || oracle.authority !== source.authority ||
      factions.length !== game.factionCount || new Set(factions).size !== factions.length) {
    throw new Error('epoch2_binding_invalid');
  }
  const tickets = dueTickets(game, oracle, factions);
  if (game.phase === 'Action') {
    if (BigInt(unixTime) < game.phaseEndsAt) return { kind: 'wait_action_deadline' };
    if (source.revealSlot !== 0n || source.seedSlot !== 0n) {
      throw new Error('source_precommitted_or_revealed');
    }
    if (oracle.maxAttemptSlot >= BigInt(slot) + 1n) {
      throw new Error('attempt_slot_not_before_commit');
    }
    return { kind: 'commit_and_advance', tickets };
  }
  if (game.phase === 'Law' && game.lawCard === 255 &&
      game.vrfAccount === source.address && game.commitSlot > 0n) {
    if (source.seedSlot !== game.commitSlot) throw new Error('pinned_commit_changed');
    if (source.revealSlot !== 0n) {
      throw new Error('provider_revealed_without_alashi');
    }
    if (game.commitSlot >= BigInt(slot)) return { kind: 'wait_seed_slot' };
    return { kind: 'reveal_and_resolve', tickets };
  }
  return { kind: 'no_operator_step' };
}

function orderedBundle(kind, providerIx, gameIx, providerProgram, gameProgram) {
  if (!['commit_and_advance', 'reveal_and_resolve'].includes(kind) ||
      providerIx.programId.toString() !== providerProgram ||
      gameIx.programId.toString() !== gameProgram) {
    throw new Error('invalid_atomic_bundle');
  }
  return [providerIx, gameIx];
}

module.exports = { DEVNET_GENESIS, COMMIT_GRACE_SECONDS,
  abortUncommittedEligible, planWithoutSource, discriminator, decodeGame, decodeOracle,
  decodeFactionIdentity, dueTickets, planStep, orderedBundle };

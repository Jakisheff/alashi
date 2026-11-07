import { ChainApiError, type ChainEvent, type ChainSnapshot } from '../client.ts'
import { decodeCandidateDonkeySnapshot } from './candidate.ts'
import { projectDonkeyRisk, type DonkeyReceipt, type DonkeyRiskInput } from './model.ts'

export type PublicDonkeySnapshot = ChainSnapshot & {
  donkeyReceipts: DonkeyReceipt[]
  donkeyContract: 'candidate-only'
}

/** Anonymous snapshot DTO, using the exact handed-off source candidate. */
export function decodePublicDonkeySnapshot(value: unknown, gamePda: string): PublicDonkeySnapshot {
  const candidate = decodeCandidateDonkeySnapshot(value, gamePda)
  // Reject contradictory complete journals before any transcript can label a result.
  if (candidate.snapshot.complete) {
    const terminal = candidate.snapshot.settled || candidate.snapshot.phase === 'Finished' || candidate.snapshot.phase === 'Aborted'
    for (const receipt of candidate.receipts) {
      const input: DonkeyRiskInput = { source: 'confirmed-chain', contract: 'candidate',
        game: gamePda, faction: receipt.faction, epoch: candidate.snapshot.epoch,
        round: receipt.round, phase: null, cashUnits: null, goods: null, alive: null,
        actionUsed: null, oracleReady: null, marketPriceUnits: null,
        journalComplete: true, journalThroughSlot: candidate.snapshot.journalThroughSlot,
        events: candidate.receipts }
      const view = projectDonkeyRisk(input)
      // Validate the full fetched journal here; a valid frozen replay prefix may
      // still be pending before its later resolution, even for a finished Game.
      if (view.kind === 'unavailable' || (terminal && view.kind === 'pending')) throw new ChainApiError('invalid')
    }
  }
  const byId = new Map(candidate.receipts.map((receipt) => [receipt.id, receipt]))
  return { ...candidate.snapshot, donkeyContract: candidate.contract,
    donkeyReceipts: candidate.receipts,
    events: candidate.snapshot.events.map((event) => {
      const receipt = byId.get(event.id)
      return receipt ? { ...event, fee: receipt.feeUnits,
        ...(receipt.kind === 'resolution' ? { success: receipt.success, reason: receipt.reason } : {}) } : event
    }),
  }
}

/** The full frozen journal prefix, including non-selected actors; no future poll. */
export function replayJournalPrefix(snapshot: ChainSnapshot, selected: readonly ChainEvent[], position: number): ChainEvent[] {
  if (!Number.isSafeInteger(position) || position < 0 || !selected.length) return []
  const anchor = selected[Math.min(position, selected.length - 1)]
  const end = snapshot.events.findIndex((event) => event.id === anchor.id)
  return end < 0 ? [] : snapshot.events.slice(0, end + 1)
}

/** No account-state reconstruction or speculative eligibility from a replay. */
export function donkeySnapshotInput(snapshot: PublicDonkeySnapshot, factionPda: string,
  prefix: readonly ChainEvent[] | null = null): DonkeyRiskInput {
  const faction = snapshot.factions.find((f) => f.pda === factionPda)
  if (!faction) throw new ChainApiError('invalid')
  const replay = prefix !== null
  const events = prefix ?? snapshot.events
  // Prefix must be an actual ordered prefix of the same frozen public snapshot.
  if (replay && events.some((e, i) => snapshot.events[i]?.id !== e.id)) throw new ChainApiError('invalid')
  const lastRound = [...events].reverse().find((event) => event.round !== undefined)?.round
  const lastPhase = [...events].reverse().find((event) => event.type === 'phase_advanced')?.phase
  const round = replay ? lastRound ?? 0 : snapshot.round
  const ids = new Set(events.map((event) => event.id))
  return { source: 'confirmed-chain', contract: 'candidate', game: snapshot.pda,
    faction: faction.pda, epoch: snapshot.epoch, round,
    phase: replay ? lastPhase ?? null : snapshot.phase,
    cashUnits: replay ? null : faction.cash, goods: replay ? null : faction.goods,
    alive: replay ? null : faction.alive,
    // Replay cannot use current account state; oracle readiness and quote remain unknown.
    actionUsed: replay ? null : faction.acted ?? null, oracleReady: null, marketPriceUnits: null,
    journalComplete: snapshot.complete, journalThroughSlot: snapshot.journalThroughSlot,
    events: snapshot.donkeyReceipts.filter((receipt) => ids.has(receipt.id)),
  }
}

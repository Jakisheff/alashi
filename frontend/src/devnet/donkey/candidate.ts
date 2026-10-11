import { ChainApiError, decodeSnapshot } from '../client.ts'
import { DONKEY_TERMS, type DonkeyReceipt } from './model.ts'

export const DONKEY_CANDIDATE_SOURCE = '058caab85e22e00e44426d3684f8c4d065c57662'
export const DONKEY_CANDIDATE_SPEC_SHA256 = '73fc5c01fa4c5b77d9f7acd3df3aebfae595917f5fcd83c371fc5f2d33763eea'
const invalid = (): never => { throw new ChainApiError('invalid') }

/** Source-candidate JSON normalization. Chain confirmation is supplied by the indexer, not independently proven here. */
export function decodeCandidateDonkeySnapshot(value: unknown, gamePda: string) {
  // Keep the existing program/cluster/commitment/identity/order/slot checks.
  const snapshot = decodeSnapshot(value, gamePda)
  const raw = (value as { events: Record<string, unknown>[] }).events
  const receipts: DonkeyReceipt[] = []
  snapshot.events.forEach((event, position) => {
    if (snapshot.epoch === 2 && event.type === 'donkey_bought') invalid()
    if (event.type !== 'donkey_attempted' && event.type !== 'donkey_resolved') return
    const e = raw[position]
    if (snapshot.epoch !== 2 || !event.faction || event.round === undefined ||
        event.round < 1 || event.round > 5 || e.fee !== DONKEY_TERMS.feeUnits) invalid()
    const common = {
      epoch: 2 as const, game: gamePda, faction: event.faction!, round: event.round!,
      // Final candidate spec associates one attempt by this tuple; never invent a wire ID.
      attemptKey: `${gamePda}:${event.faction}:${event.round}`,
      id: event.id, signature: event.signature, slot: event.slot, logIndex: event.logIndex,
      // event_index restarts in each transaction. Preserve full public array position.
      journalIndex: position, feeUnits: DONKEY_TERMS.feeUnits,
    }
    if (event.type === 'donkey_attempted') receipts.push({ ...common, kind: 'attempt' })
    else {
      if (typeof e.success !== 'boolean' || (e.reason !== 0 && e.reason !== 1) ||
          event.goods !== (e.success ? DONKEY_TERMS.goods : 0) ||
          (e.reason === 1 && e.success)) invalid()
      receipts.push({ ...common, kind: 'resolution', reason: e.reason as 0 | 1, success: e.success as boolean, goods: event.goods! })
    }
  })
  // Parsing source-candidate bytes is not a deployment gate or oracle proof.
  return { contract: 'candidate-only' as const, source: DONKEY_CANDIDATE_SOURCE, snapshot, receipts }
}

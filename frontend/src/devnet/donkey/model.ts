import { validBase58, type Phase } from '../client.ts'

// Internal presentation inputs only. These names are NOT the draft program/API wire.
export const DONKEY_TERMS = { feeUnits: '5000000', goods: 5, successPercent: 60, emptyPercent: 40 } as const
type Receipt = {
  epoch: 2; game: string; faction: string; round: number; attemptKey: string
  id: string; signature: string; slot: number; logIndex: number; journalIndex: number
  feeUnits: string
}
export type DonkeyReceipt = Receipt & (
  { kind: 'attempt' } |
  { kind: 'resolution'; reason: 0 | 1; success: boolean; goods: number }
)
export type DonkeyRiskInput = {
  source: 'confirmed-chain' | 'local-fixture'
  contract: 'verified' | 'draft'
  game: string; faction: string; epoch: number | null; round: number
  phase: Phase | null; cashUnits: string | null; goods: number | null
  alive: boolean | null; actionUsed: boolean | null; oracleReady: boolean | null
  marketPriceUnits: string | null
  journalComplete: boolean; journalThroughSlot: number
  events: readonly DonkeyReceipt[]
}
export const ELIGIBILITY = {
  phase: 'Available only during Action.',
  finalRound: 'Final round: no following Market. A donkey attempt is unavailable.',
  cash: 'At least 5 in-game alashi are required.',
  overflow: 'Not enough inventory capacity for 5 goods.',
  acted: 'This Action has already been used. No same-turn retry.',
  inactive: 'This faction is no longer active.',
  oracle: 'Oracle unavailable. No attempt can start.',
  unknown: 'Current eligibility could not be verified.',
} as const
export type EligibilityReason = keyof typeof ELIGIBILITY
export type DonkeyRiskView = {
  kind: 'legacy' | 'unavailable' | 'eligible' | 'ineligible' | 'pending' | 'success' | 'empty' | 'abort'
  reasons: EligibilityReason[]
  detail: string
  attempt: DonkeyReceipt | null
  resolution: DonkeyReceipt | null
  marketPriceUnits: string | null
}
const integer = (n: unknown): n is number => Number.isSafeInteger(n) && (n as number) >= 0
function money(value: string | null): bigint | null {
  if (typeof value !== 'string' || !/^(0|[1-9][0-9]{0,19})$/.test(value)) return null
  const n = BigInt(value)
  return n <= 18446744073709551615n ? n : null
}
function eligibility(input: DonkeyRiskInput): EligibilityReason[] {
  const reasons: EligibilityReason[] = []
  if (input.phase !== 'Action') reasons.push('phase')
  if (input.round >= 6) reasons.push('finalRound')
  const cash = money(input.cashUnits)
  if (cash !== null && cash < BigInt(DONKEY_TERMS.feeUnits)) reasons.push('cash')
  if (input.goods !== null && integer(input.goods) && input.goods > 65535 - DONKEY_TERMS.goods) reasons.push('overflow')
  if (input.actionUsed === true) reasons.push('acted')
  if (input.alive === false) reasons.push('inactive')
  if (input.oracleReady === false) reasons.push('oracle')
  if (cash === null || !integer(input.goods) || input.goods > 65535 || input.phase === null ||
      typeof input.alive !== 'boolean' || typeof input.actionUsed !== 'boolean' ||
      typeof input.oracleReady !== 'boolean') reasons.push('unknown')
  return reasons
}
function validReceipt(e: DonkeyReceipt, through: number): boolean {
  if (e.epoch !== 2 || !validBase58(e.game) || !validBase58(e.faction) ||
      !integer(e.round) || e.round < 1 || e.round >= 6 ||
      typeof e.attemptKey !== 'string' || !e.attemptKey || e.attemptKey.length > 160 ||
      !validBase58(e.signature, 64) || !integer(e.slot) || e.slot > through ||
      !integer(e.logIndex) || !integer(e.journalIndex) || e.id !== `${e.signature}:${e.logIndex}` ||
      e.feeUnits !== DONKEY_TERMS.feeUnits) return false
  if (e.kind === 'attempt') return true
  if (e.kind !== 'resolution' || typeof e.success !== 'boolean' || !integer(e.goods)) return false
  return e.reason === 0 ? e.goods === (e.success ? DONKEY_TERMS.goods : 0)
    : e.reason === 1 && e.success === false && e.goods === 0
}
function fingerprint(e: DonkeyReceipt): string {
  return JSON.stringify([e.epoch, e.game, e.faction, e.round, e.attemptKey, e.id,
    e.signature, e.slot, e.logIndex, e.journalIndex, e.feeUnits, e.kind,
    e.kind === 'resolution' ? [e.reason, e.success, e.goods] : null])
}

// Pure prefix projection: no clock, random draw, network, storage or owner action.
export function projectDonkeyRisk(input: DonkeyRiskInput): DonkeyRiskView {
  const price = money(input.marketPriceUnits) === null ? null : input.marketPriceUnits
  const view: DonkeyRiskView = { kind: 'unavailable', reasons: [], detail: '', attempt: null, resolution: null, marketPriceUnits: price }
  const unavailable = (detail: string): DonkeyRiskView => ({ ...view, detail, attempt: null, resolution: null })
  if (input.source !== 'confirmed-chain' && input.source !== 'local-fixture')
    return unavailable('The event source could not be verified.')
  if (!validBase58(input.game) || !validBase58(input.faction) || !integer(input.round) || input.round < 1 || input.round > 6)
    return unavailable('Game, player or round could not be verified.')
  if (input.epoch === 0 || input.epoch === 1)
    return { ...view, kind: 'legacy', detail: 'Historical rules: 1 good for 1 in-game alashi. No lottery.' }
  if (input.epoch !== 2) return unavailable('Donkey rules for this game are unavailable.')
  if (input.source === 'confirmed-chain' && input.contract !== 'verified')
    return unavailable('Waiting for the verified public event contract. No outcome substituted.')
  if (input.journalComplete !== true || !integer(input.journalThroughSlot))
    return unavailable('Confirmed history is incomplete. No outcome inferred.')
  view.reasons = eligibility(input)
  const selected = input.events.filter((e) => e.game === input.game && e.faction === input.faction && e.round === input.round)
  const seen = new Map<string, string>()
  let previous: DonkeyReceipt | null = null
  for (const e of selected) {
    if (!validReceipt(e, input.journalThroughSlot)) return unavailable('Event provenance or outcome could not be verified.')
    const mark = fingerprint(e), duplicate = seen.get(e.id)
    if (duplicate !== undefined) {
      if (duplicate !== mark) return unavailable('Conflicting copies of an event. No outcome inferred.')
      continue
    }
    seen.set(e.id, mark)
    if (previous && (e.slot < previous.slot || e.journalIndex <= previous.journalIndex ||
        (e.signature === previous.signature && (e.slot !== previous.slot || e.logIndex <= previous.logIndex))))
      return unavailable('Event order could not be verified. No outcome inferred.')
    previous = e
    if (e.kind === 'attempt') {
      if (view.attempt || view.resolution) return unavailable('More than one attempt in this Action. No outcome inferred.')
      view.attempt = e
    } else {
      if (!view.attempt || view.resolution || e.attemptKey !== view.attempt.attemptKey)
        return unavailable('Resolution is not linked to exactly one attempt. No outcome inferred.')
      view.resolution = e
    }
  }
  if (view.resolution?.kind === 'resolution') {
    const e = view.resolution
    return { ...view, kind: e.reason === 1 ? 'abort' : e.success ? 'success' : 'empty', detail: e.reason === 1
      ? 'Oracle abort or expiry. Zero goods; no random draw. The fee and Action remain spent.'
      : e.success ? 'Verified draw: +5 goods after Law reveal. The fee and Action remain spent.'
      : 'Verified empty draw: 0 goods. No refund or same-turn retry.' }
  }
  if (view.attempt) return { ...view, kind: 'pending', detail: 'Fee and Action committed. Awaiting verified Law reveal; goods are pending.' }
  return { ...view, kind: view.reasons.length ? 'ineligible' : 'eligible', detail: view.reasons.length
    ? 'A donkey attempt is unavailable for this Action.' : 'Eligible under the supplied current game state. No attempt submitted.' }
}

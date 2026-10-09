export const CHAIN_PROGRAM = '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC'
export const PHASES = ['Lobby', 'Market', 'Action', 'Law', 'Finished', 'Aborted'] as const
export type Phase = typeof PHASES[number]
export type ChainFaction = { pda: string; wallet: string; name: string; cash: string; hard: string; alive: boolean; goods: number; influence: number; vote: 'Yes' | 'No' | 'Abstain' }
export type ChainEvent = { id: string; signature: string; slot: number; index: number; logIndex: number; time: number | null; type: string; faction?: string; from?: string; wallet?: string; rank?: number; round?: number; phase?: Phase; units?: number; amount?: string; choice?: number }
export type ChainSnapshot = { pda: string; gameId: string; phase: Phase; round: number; endsAt: number; settled: boolean; epoch: number; snapshotSlot: number; journalThroughSlot: number; factions: ChainFaction[]; events: ChainEvent[]; complete: boolean; fetchedAt: string }
const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
export function validBase58(value: string, size = 32): boolean {
  if (typeof value !== 'string' || value.length < size || value.length > size * 1.4) return false
  let n = 0n
  for (const c of value) { const digit = alphabet.indexOf(c); if (digit < 0) return false; n = n * 58n + BigInt(digit) }
  let length = value.match(/^1*/)?.[0].length ?? 0
  while (n > 0n) { length++; n >>= 8n }
  return length === size
}
export class ChainApiError extends Error {
  retryMs: number
  code: 'invalid' | 'unavailable' | 'network' | 'rate_limit' | 'not_found'
  constructor(code: 'invalid' | 'unavailable' | 'network' | 'rate_limit' | 'not_found', retryMs = 0) { super(code); this.code = code; this.retryMs = retryMs }
}
type Json = Record<string, unknown>
const object = (v: unknown): Json => { if (!v || typeof v !== 'object' || Array.isArray(v)) throw new ChainApiError('invalid'); return v as Json }
const text = (v: unknown, max = 128): string => { if (typeof v !== 'string' || !v.length || v.length > max) throw new ChainApiError('invalid'); return v }
const integer = (v: unknown): number => { if (!Number.isSafeInteger(v) || (v as number) < 0) throw new ChainApiError('invalid'); return v as number }
const money = (v: unknown): string => { const s = text(v, 20); if (!/^(0|[1-9][0-9]*)$/.test(s) || BigInt(s) > 18446744073709551615n) throw new ChainApiError('invalid'); return s }
const address = (v: unknown): string => { const s = text(v, 44); if (!validBase58(s)) throw new ChainApiError('invalid'); return s }
const array = (v: unknown, max: number): unknown[] => { if (!Array.isArray(v) || v.length > max) throw new ChainApiError('invalid'); return v }
const phase = (v: unknown): Phase => { if (!PHASES.includes(v as Phase)) throw new ChainApiError('invalid'); return v as Phase }
const boolean = (v: unknown): boolean => { if (typeof v !== 'boolean') throw new ChainApiError('invalid'); return v }
export function decodeSnapshot(value: unknown, pda: string): ChainSnapshot {
  const v = object(value), game = object(v.game)
  if (v.ok !== true || v.cluster !== 'devnet' || v.program_id !== CHAIN_PROGRAM || v.game_pda !== pda || !validBase58(pda)) throw new ChainApiError('invalid')
  const factions = array(v.factions, 64).map((raw): ChainFaction => {
    const f = object(raw); if (!['Yes', 'No', 'Abstain'].includes(String(f.vote))) throw new ChainApiError('invalid')
    return { pda: address(f.pda), wallet: address(f.wallet), name: text(f.name, 64), cash: money(f.cash), hard: money(f.hard), alive: boolean(f.alive), goods: integer(f.goods), influence: integer(f.influence), vote: f.vote as ChainFaction['vote'] }
  })
  if (new Set(factions.map((f) => f.pda)).size !== factions.length || new Set(factions.map((f) => f.wallet)).size !== factions.length) throw new ChainApiError('invalid')
  const actors = new Set(factions.map((f) => f.pda)), wallets = new Set(factions.map((f) => f.wallet))
  const ids = new Set<string>(), closedTransactions = new Set<string>()
  let lastSignature = ''
  if (v.commitment !== 'confirmed') throw new ChainApiError('invalid')
  const snapshotSlot = integer(v.snapshot_slot), journalThroughSlot = integer(v.journal_through_slot)
  if (journalThroughSlot > snapshotSlot) throw new ChainApiError('invalid')
  const events = array(v.events, 2000).map((raw): ChainEvent => {
    const e = object(raw), signature = text(e.signature, 90), id = text(e.id, 160)
    if (e.game !== pda || !validBase58(signature, 64) || ids.has(id) || id !== `${signature}:${integer(e.log_index)}`) throw new ChainApiError('invalid')
    ids.add(id)
    if (signature !== lastSignature) {
      if (closedTransactions.has(signature)) throw new ChainApiError('invalid')
      if (lastSignature) closedTransactions.add(lastSignature)
      lastSignature = signature
    }
    const event: ChainEvent = { id, signature, slot: integer(e.slot), index: integer(e.event_index), logIndex: integer(e.log_index), time: e.block_time === null || e.block_time === undefined ? null : integer(e.block_time), type: text(e.type, 64) }
    for (const key of ['faction', 'from'] as const) if (e[key] !== undefined) { event[key] = address(e[key]); if (!actors.has(event[key])) throw new ChainApiError('invalid') }
    if (e.wallet !== undefined) { event.wallet = address(e.wallet); if (!wallets.has(event.wallet)) throw new ChainApiError('invalid') }
    for (const key of ['rank', 'round', 'units', 'choice'] as const) if (e[key] !== undefined) event[key] = integer(e[key])
    if (e.phase !== undefined) event.phase = typeof e.phase === 'number' ? phase(PHASES[integer(e.phase)]) : phase(e.phase)
    if (e.amount !== undefined) event.amount = money(e.amount)
    return event
  })
  // Preserve backend transaction order within a slot. Do not sort equal-slot transactions by signature.
  if (events.some((e, i) => i > 0 && (e.slot < events[i - 1].slot || (e.signature === events[i - 1].signature && (e.index <= events[i - 1].index || e.logIndex <= events[i - 1].logIndex))))) throw new ChainApiError('invalid')
  if (events.some((e) => e.slot > journalThroughSlot)) throw new ChainApiError('invalid')
  const fetchedAt = text(v.fetched_at, 64); if (!Number.isFinite(Date.parse(fetchedAt))) throw new ChainApiError('invalid')
  return { pda, gameId: money(game.id), phase: phase(game.phase), round: integer(game.round), endsAt: integer(game.phase_ends_at), settled: boolean(game.settled), epoch: integer(game.epoch), snapshotSlot, journalThroughSlot, factions, events, complete: boolean(v.history_complete), fetchedAt }
}
export async function chainSnapshot(pda: string, signal?: AbortSignal, fetcher: typeof fetch = fetch): Promise<ChainSnapshot> {
  if (!validBase58(pda)) throw new ChainApiError('invalid')
  let response: Response
  try { response = await fetcher(`/chain/devnet/games/${encodeURIComponent(pda)}`, { credentials: 'omit', cache: 'no-store', redirect: 'error', referrerPolicy: 'no-referrer', headers: { Accept: 'application/json' }, signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(20_000)]) : AbortSignal.timeout(20_000) }) }
  catch { if (signal?.aborted) throw new DOMException('Aborted', 'AbortError'); throw new ChainApiError('network') }
  if (response.status === 429) {
    const header = response.headers.get('Retry-After'), seconds = header === null ? NaN : Number(header)
    const delay = Number.isFinite(seconds) ? seconds * 1000 : Date.parse(header ?? '') - Date.now()
    throw new ChainApiError('rate_limit', Math.max(15_000, Number.isFinite(delay) ? delay : 0))
  }
  if (response.status === 404) throw new ChainApiError('not_found')
  if (!response.ok) throw new ChainApiError('unavailable')
  let value: unknown
  try { value = await response.json() } catch { throw new ChainApiError('invalid') }
  return decodeSnapshot(value, pda)
}
export function chainFailure(error: unknown): string {
  if (error instanceof ChainApiError && error.code === 'rate_limit') return 'Devnet is busy. Retrying after a short pause.'
  if (error instanceof ChainApiError && error.code === 'not_found') return 'This game history is not available yet. Retrying…'
  if (error instanceof ChainApiError && error.code === 'invalid') return 'The chain snapshot could not be verified. Retrying…'
  return 'Connection interrupted. Retrying…'
}
export function formatAmount(value: string) { const n = BigInt(value); const fraction = (n % 1_000_000n).toString().padStart(6, '0').replace(/0+$/, ''); return `${n / 1_000_000n}${fraction ? `.${fraction}` : ''}` }

export function formatCash(value: string) { return `${formatAmount(value)} alashi` }

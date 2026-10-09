import { createLiveApi, LiveApiError, object, validRecord } from '../live/api/client.ts'
import { validBase58, type ChainFaction } from './client.ts'
export const CHAIN_INTENTS = ['produce', 'sell_one', 'buy_one', 'vote_yes', 'vote_no'] as const
export type ChainIntent = typeof CHAIN_INTENTS[number]
const statuses = ['received', 'consumed', 'deferred', 'declined', 'expired', 'unconfirmed', 'confirmed'] as const
export type ChainWish = { id: string; seq: number; statusSeq: number; text: string; intent: ChainIntent; status: typeof statuses[number]; reply: string | null; acceptedAt: number; signature: string | null; slot: number | null; eventId: string | null }
export type ChainPending = { game_pda: string; client_wish_id: string; intent: ChainIntent; text: string }
const invalid = () => { throw new LiveApiError('invalid_response') }
const number = (v: unknown): number => Number.isSafeInteger(v) && (v as number) >= 0 ? v as number : invalid()
const text = (v: unknown, max = 512): string => typeof v === 'string' && v.length <= max ? v : invalid()
const quota = (v: unknown): number => { const n = number(v); return n <= 3 ? n : invalid() }
export function projectChainWishes(raw: unknown, game: string, faction: ChainFaction) {
  const v = object(raw), binding = v.binding === null ? null : object(v.binding)
  if (v.ok !== true || v.game_pda !== game || (binding !== null && (binding.faction_pda !== faction.pda || binding.faction_wallet !== faction.wallet || typeof binding.active !== 'boolean')) || !Array.isArray(v.wishes) || v.wishes.length > 100) invalid()
  const cursor = number(v.next_cursor), lastSeq = number(v.last_seq), ids = new Set<string>()
  const wishes = (v.wishes as unknown[]).map((raw): ChainWish => {
    const w = object(raw), id = text(w.wish_id, 128)
    if (!id || ids.has(id) || !statuses.includes(w.status as ChainWish['status']) || !CHAIN_INTENTS.includes(w.intent as ChainIntent)) invalid()
    ids.add(id)
    const signature = w.signature == null ? null : text(w.signature, 90)
    const slot = w.slot == null ? null : number(w.slot), eventId = w.event_id == null ? null : text(w.event_id, 160)
    if (signature && !validBase58(signature, 64)) invalid()
    if (w.status === 'confirmed' && (!signature || slot === null || !eventId?.startsWith(signature + ':'))) invalid()
    return { id, seq: number(w.seq), statusSeq: number(w.status_seq), intent: w.intent as ChainIntent, text: text(w.text), status: w.status as ChainWish['status'], reply: w.reply == null ? null : text(w.reply, 4096), acceptedAt: number(w.accepted_at), signature, slot, eventId }
  })
  if (cursor > lastSeq || wishes.some((w, i) => w.statusSeq > cursor || (i > 0 && w.statusSeq <= wishes[i - 1].statusSeq))) invalid()
  return { wishes, cursor, lastSeq, remaining: quota(v.remaining), active: binding?.active === true }
}
export function mergeChainWishes(previous: ChainWish[], next: ChainWish[]) {
  const byId = new Map(previous.map((w) => [w.id, w]))
  for (const w of next) if (w.statusSeq >= (byId.get(w.id)?.statusSeq ?? -1)) byId.set(w.id, w)
  return [...byId.values()].sort((a, b) => a.seq - b.seq).slice(-3)
}
export function createChainOwnerApi(prefix = '', fetcher: typeof fetch = fetch) {
  const auth = createLiveApi(prefix, fetcher)
  const base = prefix.replace(/\/$/, '')
  async function request(record: string, suffix: string, signal?: AbortSignal, body?: ChainPending) {
    if (!validRecord(record)) throw new LiveApiError('invalid_agent')
    let response: Response
    try { response = await fetcher(`${base}/agents/${record}/owner/chain-wishes${suffix}`, { method: body ? 'POST' : 'GET', credentials: 'include', redirect: 'error', referrerPolicy: 'no-referrer', cache: 'no-store', headers: { Accept: 'application/json', ...(body ? { 'Content-Type': 'application/json' } : {}) }, signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(10_000)]) : AbortSignal.timeout(10_000), ...(body ? { body: JSON.stringify(body) } : {}) }) }
    catch { if (signal?.aborted) throw new DOMException('Aborted', 'AbortError'); throw new LiveApiError('network') }
    let v: Record<string, unknown>
    try { v = object(await response.json()) } catch { throw new LiveApiError('unavailable') }
    // Never render response bodies or arbitrary backend error strings.
    if (!response.ok || v.ok !== true) {
      const allowed = ['owner_session_invalid', 'owner_session_expired', 'owner_cookie_invalid', 'wish_quota_exhausted', 'no_active_game', 'idempotency_conflict', 'bad_wish']
      throw new LiveApiError(allowed.includes(String(v.error)) ? v.error as string : response.status === 401 ? 'owner_session_invalid' : 'unavailable')
    }
    return v
  }
  return { ...auth,
    wishes: async (record: string, game: string, faction: ChainFaction, after: number, signal?: AbortSignal) => {
      if (!validBase58(game)) invalid()
      return projectChainWishes(await request(record, `?game=${encodeURIComponent(game)}&after=${number(after)}&limit=100`, signal), game, faction)
    },
    submit: async (record: string, body: ChainPending, signal?: AbortSignal) => {
      if (!validBase58(body.game_pda) || !CHAIN_INTENTS.includes(body.intent) || !body.text.trim() || new TextEncoder().encode(body.text).length > 512 || !/^[a-zA-Z0-9-]{1,128}$/.test(body.client_wish_id) || Array.from(body.text).some((c) => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127)) throw new LiveApiError('bad_wish')
      const v = await request(record, '', signal, body)
      if (v.game_pda !== body.game_pda || v.status !== 'received') invalid()
      return { id: text(v.wish_id, 128), remaining: quota(v.remaining) }
    },
  }
}
export type ChainOwnerApi = ReturnType<typeof createChainOwnerApi>

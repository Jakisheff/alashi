import { ChainApiError, validBase58, formatAmount, type ChainSnapshot } from './client'
import type { ChainConversationEntry, ConversationDecline } from './ChainConversationStream'

// Wire: docs/CHAIN_CONVERSATIONS_V1.md at c38a3f4. No runner/owner POSTs here.
export type PublicConversationEntry = {
  entry_id: string; seq: string; game_pda: string
  kind: 'offer_confirmed' | 'accepted_confirmed' | 'declined_rule'
  source: 'onchain_event' | 'runner_reported'; created_at: string; round: number
  author_faction_pda: string; proposer_faction_pda: string; counterparty_faction_pda: string | null
  offer_id: string; goods: number | null; price: string | null; in_reply_to: string | null
  rule_code: ConversationDecline | null
  receipt: { event_type: 'barter_proposed' | 'barter_accepted'; signature: string; slot: string; event_id: string } | null
}
export type ConversationPage = {
  entries: PublicConversationEntry[]; next_cursor: string; latest_seq: string
  has_more: boolean; history_complete: boolean; recording_started_at: string | null
}
const fail = (): never => { throw new ChainApiError('invalid') }
const object = (v: unknown): Record<string, unknown> => v && typeof v === 'object' && !Array.isArray(v) ? v as Record<string, unknown> : fail()
const text = (v: unknown, max = 160): string => typeof v === 'string' && v.length > 0 && v.length <= max ? v : fail()
export function conversationU64(v: unknown): string {
  const s = text(v, 20)
  return /^(0|[1-9][0-9]*)$/.test(s) && BigInt(s) <= 18446744073709551615n ? s : fail()
}
const address = (v: unknown): string => { const s = text(v, 44); return validBase58(s) ? s : fail() }
const date = (v: unknown): string => { const s = text(v, 64); return /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|\+00:00)$/.test(s) && Number.isFinite(Date.parse(s)) ? s : fail() }
const boolean = (v: unknown): boolean => typeof v === 'boolean' ? v : fail()
const declines = ['insufficient_goods', 'insufficient_cash', 'outside_policy', 'expired_offer']
export function decodeConversations(value: unknown, game: string, after = '0'): ConversationPage {
  conversationU64(after)
  const v = object(value)
  if (!validBase58(game) || v.ok !== true || v.schema !== 'alashi.chain_conversations.v1' || v.game_pda !== game
    || !Array.isArray(v.entries) || v.entries.length > 100) return fail()
  let previous = BigInt(after)
  const ids = new Set<string>()
  const entries = v.entries.map((raw): PublicConversationEntry => {
    const e = object(raw), seq = conversationU64(e.seq), id = text(e.entry_id, 160)
    if (BigInt(seq) <= previous || ids.has(id) || e.game_pda !== game || !Number.isInteger(e.round)
      || (e.round as number) < 0 || (e.round as number) > 6) return fail()
    previous = BigInt(seq); ids.add(id)
    const proposer = address(e.proposer_faction_pda), author = address(e.author_faction_pda)
    const counterparty = e.counterparty_faction_pda === null ? null : address(e.counterparty_faction_pda)
    const common = { entry_id: id, seq, game_pda: game, created_at: date(e.created_at), round: e.round as number,
      author_faction_pda: author, proposer_faction_pda: proposer, counterparty_faction_pda: counterparty, offer_id: conversationU64(e.offer_id) }
    if (e.kind === 'declined_rule') {
      if (e.source !== 'runner_reported' || !counterparty || counterparty === proposer || author !== counterparty
        || e.receipt !== null || e.goods !== null || e.price !== null || !declines.includes(String(e.rule_code))) return fail()
      return { ...common, kind: e.kind, source: e.source, goods: null, price: null,
        in_reply_to: text(e.in_reply_to), rule_code: e.rule_code as ConversationDecline, receipt: null }
    }
    if (!['offer_confirmed', 'accepted_confirmed'].includes(String(e.kind)) || e.source !== 'onchain_event' || e.rule_code !== null) return fail()
    const r = object(e.receipt), signature = text(r.signature, 90), slot = conversationU64(r.slot), eventId = text(r.event_id)
    const type = e.kind === 'offer_confirmed' ? 'barter_proposed' : 'barter_accepted'
    if (r.event_type !== type || !validBase58(signature, 64) || !eventId.startsWith(`${signature}:`)
      || !/^(0|[1-9][0-9]*)$/.test(eventId.slice(signature.length + 1))) return fail()
    const receipt = { event_type: type as 'barter_proposed' | 'barter_accepted', signature, slot, event_id: eventId }
    if (e.kind === 'offer_confirmed') {
      if (author !== proposer || counterparty !== null || e.in_reply_to !== null || !Number.isInteger(e.goods)
        || (e.goods as number) < 1 || (e.goods as number) > 65535) return fail()
      return { ...common, kind: e.kind, source: e.source, goods: e.goods as number, price: conversationU64(e.price), in_reply_to: null, rule_code: null, receipt }
    }
    if (!counterparty || counterparty === proposer || author !== counterparty || e.goods !== null || e.price !== null) return fail()
    return { ...common, kind: 'accepted_confirmed', source: e.source, goods: null, price: null, in_reply_to: text(e.in_reply_to), rule_code: null, receipt }
  })
  const cursor = conversationU64(v.next_cursor), latest = conversationU64(v.latest_seq)
  const more = boolean(v.has_more), complete = boolean(v.history_complete)
  const started = v.recording_started_at === null ? null : date(v.recording_started_at)
  if (BigInt(cursor) !== previous || BigInt(latest) < previous || more !== (BigInt(latest) > previous)
    || (more && !entries.length) || (started === null && (entries.length || latest !== '0' || complete))) return fail()
  return { entries, next_cursor: cursor, latest_seq: latest, has_more: more, history_complete: complete, recording_started_at: started }
}
export async function conversationsPage(game: string, after: string, signal?: AbortSignal, fetcher: typeof fetch = fetch): Promise<ConversationPage> {
  if (!validBase58(game)) return fail()
  conversationU64(after)
  let response: Response
  try {
    response = await fetcher(`/chain/devnet/games/${encodeURIComponent(game)}/conversations?after=${after}&limit=100`, {
      method: 'GET', credentials: 'omit', cache: 'no-store', redirect: 'error', referrerPolicy: 'no-referrer', headers: { Accept: 'application/json' },
      signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(20_000)]) : AbortSignal.timeout(20_000),
    })
  } catch { if (signal?.aborted) throw new DOMException('Aborted', 'AbortError'); throw new ChainApiError('network') }
  if (response.status === 429) {
    const h = response.headers.get('Retry-After'), seconds = h === null ? NaN : Number(h)
    const delay = Number.isFinite(seconds) ? seconds * 1000 : Date.parse(h ?? '') - Date.now()
    throw new ChainApiError('rate_limit', Math.max(12_000, Number.isFinite(delay) ? delay : 0))
  }
  if (response.status === 404) throw new ChainApiError('not_found')
  if (!response.ok) throw new ChainApiError('unavailable')
  const body = await response.text()
  if (body.length > 256_000) return fail()
  let value: unknown
  try { value = JSON.parse(body) } catch { return fail() }
  return decodeConversations(value, game, after)
}

/** Align with the canonical chain journal. Exact barter fields/program success
 * are verified by the server; the existing browser projection omits those fields.
 * Do not claim a second cryptographic verification by this presentation adapter.
 */
export function conversationView(entries: readonly PublicConversationEntry[], snapshot: ChainSnapshot, replayEventId: string | null = null) {
  const actors = new Map(snapshot.factions.map((f) => [f.pda, { pda: f.pda, name: f.name }]))
  const events = new Map<string, { index: number; round: number }>()
  let round = 0
  snapshot.events.forEach((e, index) => { if (e.type === 'phase_advanced' && e.round !== undefined) round = e.round; events.set(e.id, { index, round }) })
  const stop = replayEventId === null ? snapshot.events.length - 1 : (events.get(replayEventId)?.index ?? -1)
  const rows: ChainConversationEntry[] = [], offers = new Map<string, PublicConversationEntry>()
  let unmatched = false
  for (const e of entries) {
    const proposer = actors.get(e.proposer_faction_pda), counterparty = e.counterparty_faction_pda ? actors.get(e.counterparty_faction_pda) : null
    if (e.game_pda !== snapshot.pda || !proposer || (e.counterparty_faction_pda && !counterparty)) { unmatched = true; continue }
    if (e.receipt) {
      const match = events.get(e.receipt.event_id), chain = match ? snapshot.events[match.index] : undefined
      if (!match || !chain || chain.signature !== e.receipt.signature || String(chain.slot) !== e.receipt.slot
        || chain.type !== e.receipt.event_type || match.round !== e.round
        || (chain.from !== undefined && chain.from !== e.proposer_faction_pda)) { unmatched = true; continue }
      if (match.index > stop) continue
    }
    const base = { id: e.entry_id, gamePda: e.game_pda, offerId: e.offer_id, proposer, counterparty: counterparty ?? null,
      round: e.round, recordedAt: e.created_at, source: e.source, ...(e.in_reply_to ? { inReplyTo: e.in_reply_to } : {}) }
    const receipt = e.receipt ? { gamePda: e.game_pda, offerId: e.offer_id, proposerPda: proposer.pda,
      counterpartyPda: counterparty?.pda ?? null, event: e.kind === 'offer_confirmed' ? 'BarterProposed' as const : 'BarterAccepted' as const,
      signature: e.receipt.signature, slot: e.receipt.slot, eventId: e.receipt.event_id } : undefined
    if (e.kind === 'offer_confirmed') {
      offers.set(e.entry_id, e)
      rows.push({ ...base, kind: 'offer', give: { quantity: String(e.goods), asset: 'goods' }, receive: { quantity: formatAmount(e.price!), asset: 'alashi' }, receipt })
    } else {
      const offer = offers.get(e.in_reply_to!)
      if (!offer) {
        if (!entries.some((candidate) => candidate.kind === 'offer_confirmed' && candidate.entry_id === e.in_reply_to)) unmatched = true
        continue
      }
      if (offer.offer_id !== e.offer_id || offer.proposer_faction_pda !== e.proposer_faction_pda
        || (e.kind === 'declined_rule' && offer.round !== e.round)) { unmatched = true; continue }
      rows.push(e.kind === 'accepted_confirmed' ? { ...base, kind: 'response', outcome: 'accepted', receipt }
        : { ...base, kind: 'response', outcome: 'declined_rule', reason: e.rule_code! })
    }
  }
  return { rows, unmatched }
}

// Exact public/owner contract: backend e114c615, release docs baa13b3.
// Only these projections cross into React. Never retain raw API objects or error bodies.
type Json = Record<string, unknown>
export const GESTURE_CUES = ['thumbsUp', 'realization', 'facepalm'] as const
export type GestureCue = typeof GESTURE_CUES[number]
export type PublicEvent = {
  id: string; seq: number; kind: 'agent_message' | 'game_action' | 'phase_changed' | 'final_result'
  createdAt: string; room: string; gameId?: number; round?: number; phase?: string
  author?: string; to?: string; replyTo?: string; text?: string; gestureCue?: GestureCue
  actor?: number; action?: string; phaseEndsAt?: number
}
export type PublicPage = { events: PublicEvent[]; cursor: number; hasMore: boolean; truncated: boolean; serverNow: number; presence: 'connected' | 'offline' }
export const WISH_STATUSES = ['received', 'consumed', 'replied', 'deferred', 'declined', 'expired'] as const
export type Wish = { id: string; seq: number; statusSeq: number; gameId: number; text: string; status: typeof WISH_STATUSES[number]; reply: string | null; acceptedAt: number }
export type WishPage = { wishes: Wish[]; cursor: number; lastSeq: number; remaining: Record<string, number> }
export type Challenge = { id: string; wallet: string; message: string; issuedAt: number; expiresAt: number }
export type OwnerSession = { wallet: string; expiresAt: number }
export type Admission = { id: string; remaining: number }
export type AgentSlot = { gameId: number; actor: number; phase: string; round: number }
const codes = ['cursor_expired', 'cursor_ahead', 'owner_session_invalid', 'owner_session_expired', 'owner_cookie_invalid', 'owner_signature_invalid', 'owner_origin_forbidden', 'owner_challenge_rate_limited', 'owner_challenge_expired', 'owner_challenge_invalid', 'owner_challenge_used', 'unknown_agent', 'registration_required', 'wish_quota_exhausted', 'no_active_game', 'idempotency_conflict', 'bad_wish', 'storage_failed']
export class LiveApiError extends Error {
  readonly code: string
  constructor(code: string) { super(code); this.name = 'LiveApiError'; this.code = code }
}
export function object(v: unknown): Json { if (!v || typeof v !== 'object' || Array.isArray(v)) throw new LiveApiError('invalid_response'); return v as Json }
function string(v: unknown, max = 4096): string { if (typeof v !== 'string' || v.length > max) throw new LiveApiError('invalid_response'); return v }
function number(v: unknown): number { if (typeof v !== 'number' || !Number.isSafeInteger(v) || v < 0) throw new LiveApiError('invalid_response'); return v }
function boolean(v: unknown): boolean { if (typeof v !== 'boolean') throw new LiveApiError('invalid_response'); return v }
function optionalNumber(v: unknown) { return v === undefined || v === null ? undefined : number(v) }
function optionalString(v: unknown, max = 256) { return v === undefined || v === null ? undefined : string(v, max) }
function optionalGestureCue(v: unknown): GestureCue | undefined { return typeof v === 'string' && v.length <= 32 && (GESTURE_CUES as readonly string[]).includes(v) ? v as GestureCue : undefined }
export function validRecord(id: string) { return /^[0-9a-f]{64}$/.test(id) }
function record(id: string) { if (!validRecord(id)) throw new LiveApiError('invalid_agent'); return id }
function array(v: unknown): unknown[] { if (!Array.isArray(v) || v.length > 200) throw new LiveApiError('invalid_response'); return v }
export function projectPublicPage(raw: unknown): PublicPage {
  const v = object(raw)
  const events = array(v.events).flatMap((rawEvent): PublicEvent[] => {
    const e = object(rawEvent)
    // Unknown/private events never become public text, even if a server regresses.
    if (e.visibility !== 'public' || !['agent_message', 'game_action', 'phase_changed', 'final_result'].includes(String(e.kind))) return []
    const createdAt = string(e.server_created_at, 64)
    if (!Number.isFinite(Date.parse(createdAt))) throw new LiveApiError('invalid_response')
    const out: PublicEvent = { id: string(e.event_id, 128), seq: number(e.seq), kind: e.kind as PublicEvent['kind'], createdAt, room: string(e.room_id, 128), gameId: optionalNumber(e.game_id), round: optionalNumber(e.round), phase: optionalString(e.phase, 32) }
    if (out.kind === 'agent_message') {
      out.author = record(string(e.author_agent_record_id, 64)); out.to = optionalString(e.to_agent_record_id, 64)
      out.replyTo = optionalString(e.reply_to_message_id); out.text = string(e.text, 4096); out.gestureCue = optionalGestureCue(e.gesture_cue)
    } else {
      if (e.finality !== 'final' || e.branch_id !== 'MAIN') return []
      if (out.kind === 'game_action') { const a = object(e.action_ref); out.actor = number(a.actor); out.action = string(a.action, 64) }
      if (out.kind === 'phase_changed') out.phaseEndsAt = optionalNumber(e.phase_ends_at)
    }
    return [out]
  })
  const cursor = number(v.next_cursor)
  if (events.some((e, i) => e.seq > cursor || (i > 0 && e.seq <= events[i - 1].seq))) throw new LiveApiError('invalid_response')
  if (v.presence !== 'connected' && v.presence !== 'offline') throw new LiveApiError('invalid_response')
  return { events, cursor, hasMore: boolean(v.has_more), truncated: boolean(v.history_truncated), serverNow: number(v.server_now), presence: v.presence }
}
export function mergePublic(previous: PublicEvent[], next: PublicEvent[]) {
  const byId = new Map(previous.map((e) => [e.id, e]))
  for (const e of next) byId.set(e.id, e)
  return [...byId.values()].sort((a, b) => a.seq - b.seq).slice(-400)
}
export function projectWishes(raw: unknown): WishPage {
  const v = object(raw), remaining: Record<string, number> = Object.create(null)
  for (const [game, value] of Object.entries(object(v.remaining_by_game))) {
    const n = number(value)
    if (!/^[1-9][0-9]*$/.test(game) || n > 3) throw new LiveApiError('invalid_response')
    remaining[game] = n
  }
  const wishes = array(v.wishes).map((rawWish): Wish => {
    const w = object(rawWish), status = string(w.status, 32)
    if (!WISH_STATUSES.includes(status as Wish['status'])) throw new LiveApiError('invalid_response')
    return { id: string(w.wish_id, 128), seq: number(w.seq), statusSeq: number(w.status_seq), gameId: number(w.game_id), text: string(w.text), status: status as Wish['status'], reply: w.reply === null || w.reply === undefined ? null : string(w.reply), acceptedAt: number(w.accepted_at) }
  })
  const cursor = number(v.next_cursor), lastSeq = number(v.last_seq)
  if (cursor > lastSeq || wishes.some((w, i) => w.statusSeq > cursor || (i > 0 && w.statusSeq <= wishes[i - 1].statusSeq))) throw new LiveApiError('invalid_response')
  return { wishes, cursor, lastSeq, remaining }
}
export function mergeWishes(previous: Wish[], next: Wish[]) {
  const byId = new Map(previous.map((w) => [w.id, w]))
  for (const w of next) if (w.statusSeq >= (byId.get(w.id)?.statusSeq ?? -1)) byId.set(w.id, w)
  return [...byId.values()].sort((a, b) => a.seq - b.seq).slice(-200)
}

export function createLiveApi(prefix = '', fetcher: typeof fetch = fetch) {
  if (prefix && !/^\/[a-zA-Z0-9/_-]+$/.test(prefix)) throw new LiveApiError('invalid_api_path')
  const base = prefix.replace(/\/$/, '')
  async function request(path: string, signal?: AbortSignal, body?: unknown, browserSession = false): Promise<Json> {
    let response: Response
    try {
      response = await fetcher(`${base}${path}`, { method: body === undefined ? 'GET' : 'POST', credentials: browserSession ? 'include' : 'omit', cache: 'no-store', redirect: 'error', referrerPolicy: 'no-referrer', signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(10_000)]) : AbortSignal.timeout(10_000), headers: { Accept: 'application/json', ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) })
    } catch { if (signal?.aborted) throw new DOMException('Aborted', 'AbortError'); throw new LiveApiError('network') }
    let v: Json
    try { v = object(await response.json()) } catch { throw new LiveApiError('unavailable') }
    if (!response.ok || v.ok !== true) throw new LiveApiError(typeof v.error === 'string' && codes.includes(v.error) ? v.error : 'unavailable')
    return v
  }
  const owner = (id: string) => `/agents/${record(id)}/owner`
  return {
    profile: async (id: string, signal?: AbortSignal): Promise<AgentSlot[]> => { const v = await request(`/agents/${record(id)}`, signal); if (v.agent_record_id !== id || v.registered !== true) throw new LiveApiError('invalid_response'); return array(v.active_slots).map((raw) => { const slot = object(raw); return { gameId: number(slot.game_id), actor: number(slot.faction_idx), phase: string(slot.phase, 32), round: number(slot.round) } }) },
    public: async (id: string, after: number, signal?: AbortSignal) => projectPublicPage(await request(`/agents/${record(id)}/live/events?after=${number(after)}&limit=100`, signal)),
    challenge: async (id: string, signal?: AbortSignal): Promise<Challenge> => { const v = await request(`${owner(id)}/challenge`, signal, {}); return { id: string(v.challenge_id, 128), wallet: string(v.wallet, 64), message: string(v.message, 1024), issuedAt: number(v.issued_at), expiresAt: number(v.expires_at) } },
    browserSession: async (id: string, challengeId: string, signature: string, signal?: AbortSignal): Promise<{ expiresAt: number }> => { const v = await request(`${owner(id)}/browser/session`, signal, { challenge_id: challengeId, signature }, true); return { expiresAt: number(v.expires_at) } },
    restoreBrowserSession: async (id: string, signal?: AbortSignal): Promise<OwnerSession> => { const v = await request(`${owner(id)}/browser/session`, signal, undefined, true); return { wallet: string(v.wallet, 64), expiresAt: number(v.expires_at) } },
    logoutBrowserSession: async (id: string, signal?: AbortSignal) => { await request(`${owner(id)}/browser/logout`, signal, {}, true) },
    wishes: async (id: string, after: number, signal?: AbortSignal) => projectWishes(await request(`${owner(id)}/wishes?after=${number(after)}&limit=100`, signal, undefined, true)),
    submit: async (id: string, wish: { game_id: number; client_wish_id: string; text: string }, signal?: AbortSignal): Promise<Admission> => { const v = await request(`${owner(id)}/wishes`, signal, wish, true); const remaining = number(v.remaining); if (remaining > 3) throw new LiveApiError('invalid_response'); return { id: string(v.wish_id, 128), remaining } },
  }
}
export type LiveApi = ReturnType<typeof createLiveApi>

export function friendlyError(error: unknown) {
  const code = error instanceof LiveApiError ? error.code : 'unknown'
  const messages: Record<string, string> = {
    network: 'Connection interrupted. Try again when the connection returns.', unavailable: 'The live service is unavailable. Please try again.', invalid_response: 'The live service returned an unexpected response.',
    invalid_agent: 'Enter a registered agent ID (64 lowercase hexadecimal characters).', unknown_agent: 'This agent is not registered.', registration_required: 'This agent needs a confirmed registration.',
    owner_origin_forbidden: 'Owner access is not enabled for this site yet.', owner_signature_invalid: 'The signature could not be verified.',
    owner_session_invalid: 'No active private browser session was found. Verify the registered wallet.', owner_session_expired: 'Your private browser session ended. Verify the registered wallet again.', owner_cookie_invalid: 'This private browser session is no longer valid. Verify the registered wallet again.', browser_cookie_missing: 'Browser sign-in could not be saved. Allow cookies for this site and try again.',
    owner_challenge_rate_limited: 'Too many verification attempts. Please wait a few minutes.', owner_challenge_expired: 'The verification request expired. Please try again.',
    wish_quota_exhausted: 'All three wishes for this game have been used.', no_active_game: 'This agent is no longer in that active game. Refresh its private journal.',
    idempotency_conflict: 'This submission ID was already used. Refresh the journal before trying a new wish.',
    wallet_missing: 'Open this page in a browser with a Solana wallet extension.', wallet_mismatch: 'Select the wallet registered to this agent.', wallet_rejected: 'Wallet verification was cancelled or unavailable.',
    challenge_mismatch: 'The signing request does not match this site, agent or wallet.',
  }
  return messages[code] ?? 'The request could not be completed. Please try again.'
}

export type ArenaGame = {
  gameId: number
  partyNo: number
  label: string
  phase: string
  round: number
  factions: number
  names: string[]
  endsIn: number
}

export type ArenaEvent = {
  id: string
  seq: number
  kind: 'agent_message' | 'game_action' | 'phase_changed' | 'final_result'
  createdAt: string
  round?: number
  phase?: string
  author?: string
  text?: string
  actor?: number
  action?: string
}

export type ArenaEventPage = { events: ArenaEvent[]; cursor: number; more: boolean; truncated: boolean }
export class ArenaApiError extends Error {
  readonly code: 'network' | 'unavailable' | 'invalid_response'
  constructor(code: 'network' | 'unavailable' | 'invalid_response') { super(code); this.code = code }
}

type Json = Record<string, unknown>
const object = (value: unknown): Json => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new ArenaApiError('invalid_response')
  return value as Json
}
const array = (value: unknown): unknown[] => {
  if (!Array.isArray(value) || value.length > 100) throw new ArenaApiError('invalid_response')
  return value
}
const text = (value: unknown, max = 256): string => {
  if (typeof value !== 'string' || value.length > max) throw new ArenaApiError('invalid_response')
  return value
}
const integer = (value: unknown, min = 0): number => {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < min) throw new ArenaApiError('invalid_response')
  return value
}
const optionalInteger = (value: unknown) => value === undefined || value === null ? undefined : integer(value)
const optionalText = (value: unknown, max = 256) => value === undefined || value === null ? undefined : text(value, max)
const nullableText = (value: unknown, max = 256) => value === null ? '' : text(value, max)
const record = (value: unknown) => { const id = text(value, 64); if (!/^[0-9a-f]{64}$/.test(id)) throw new ArenaApiError('invalid_response'); return id }

async function get(path: string, signal?: AbortSignal): Promise<Json> {
  let response: Response
  try {
    response = await fetch(path, {
      credentials: 'omit', cache: 'no-store', redirect: 'error', referrerPolicy: 'no-referrer',
      headers: { Accept: 'application/json' }, signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(10_000)]) : AbortSignal.timeout(10_000),
    })
  } catch {
    if (signal?.aborted) throw new DOMException('Aborted', 'AbortError')
    throw new ArenaApiError('network')
  }
  let value: Json
  try { value = object(await response.json()) } catch { throw new ArenaApiError('unavailable') }
  if (!response.ok || value.ok !== true) throw new ArenaApiError('unavailable')
  return value
}

export async function activeArenaGames(signal?: AbortSignal): Promise<ArenaGame[]> {
  const value = await get('/games', signal)
  return array(value.games).map((raw) => {
    const game = object(raw)
    return {
      gameId: integer(game.game_id, 1),
      partyNo: integer(game.party_no, 1),
      label: nullableText(game.label, 32),
      phase: text(game.phase, 32),
      round: integer(game.round, 0),
      factions: integer(game.factions),
      names: array(game.names).map((name) => text(name, 80)),
      // `ends_in` can be negative during a server transition; it is display-only.
      endsIn: typeof game.ends_in === 'number' && Number.isFinite(game.ends_in) ? game.ends_in : 0,
    }
  }).sort((a, b) => a.gameId - b.gameId)
}

export async function arenaEvents(gameId: number, after: number, signal?: AbortSignal): Promise<ArenaEventPage> {
  const value = await get(`/game/${integer(gameId, 1)}/live/events?after=${integer(after)}&limit=100`, signal)
  const events = array(value.events).flatMap((raw): ArenaEvent[] => {
    const event = object(raw)
    if (event.visibility !== 'public') return []
    const kind = text(event.kind, 32)
    if (kind !== 'agent_message' && kind !== 'game_action' && kind !== 'phase_changed' && kind !== 'final_result') return []
    const createdAt = text(event.server_created_at, 64)
    if (!Number.isFinite(Date.parse(createdAt))) throw new ArenaApiError('invalid_response')
    const output: ArenaEvent = { id: text(event.event_id, 128), seq: integer(event.seq, 1), kind, createdAt, round: optionalInteger(event.round), phase: optionalText(event.phase, 32) }
    if (kind === 'agent_message') { output.author = record(event.author_agent_record_id); output.text = text(event.text, 4096) }
    if (kind === 'game_action') {
      const action = object(event.action_ref)
      output.actor = integer(action.actor)
      output.action = text(action.action, 64)
    }
    return [output]
  })
  const cursor = integer(value.next_cursor)
  if (events.some((event, index) => event.seq > cursor || (index > 0 && event.seq <= events[index - 1].seq))) throw new ArenaApiError('invalid_response')
  return { events, cursor, more: value.has_more === true, truncated: value.history_truncated === true }
}

export const mergeArenaEvents = (previous: ArenaEvent[], next: ArenaEvent[]) => {
  const byId = new Map(previous.map((event) => [event.id, event]))
  for (const event of next) byId.set(event.id, event)
  return [...byId.values()].sort((a, b) => a.seq - b.seq).slice(-200)
}

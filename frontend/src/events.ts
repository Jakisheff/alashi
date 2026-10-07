// Arena public events -> what Degenie shows. Contract: team docs PUBLIC_EVENTS_FOR_DIN_2026-10-07.md
// (docs a3d8ed8, arena main 73e65e9). Pure functions only, checked by scripts/events-check.ts.
import type { GameResult } from './log/model'

export type ArenaEvent = {
  seq: number
  event_id: string
  round: number
  phase: string
  actor: number | null
  action: string
  by: string
  ok: boolean
  ts: number
}

export type ArenaState = {
  game_id: number
  party_no: number
  round: number
  phase: string
  factions: { idx: number; name: string }[]
  recent_actions: ArenaEvent[]
  recent_actions_range: { first_seq: number | null; last_seq: number | null; retained_first_seq: number | null; limit: number }
}

export type Cursor = { scope: string; lastSeq: number } | null

export type FeedUpdate = {
  cursor: Cursor
  history: ArenaEvent[] // first look at a game: context only, never animated
  fresh: ArenaEvent[] // new since the cursor, in seq order: animate these
  gap: boolean // the 12-event window moved past events we never saw
}

/** Advance the cursor over one GET /game/:id/state snapshot. */
export function advance(cursor: Cursor, state: ArenaState): FeedUpdate {
  const scope = `${state.game_id}:${state.party_no}`
  const events = [...state.recent_actions].sort((a, b) => a.seq - b.seq)
  const { first_seq, last_seq } = state.recent_actions_range
  if (!cursor || cursor.scope !== scope) {
    return { cursor: { scope, lastSeq: last_seq ?? 0 }, history: events, fresh: [], gap: false }
  }
  const gap = first_seq !== null && cursor.lastSeq + 1 < first_seq
  const fresh = events.filter((e) => e.seq > cursor.lastSeq)
  const lastSeq = fresh.length ? fresh[fresh.length - 1].seq : cursor.lastSeq
  return { cursor: { scope, lastSeq }, history: [], fresh, gap }
}

const VERBS: Record<string, [emoji: string, text: string]> = {
  produce: ['⚙️', 'produced goods'],
  sell: ['📦', 'sold goods'],
  buy: ['🛒', 'bought goods'],
  donkey: ['🫏', 'bought a donkey'],
  bribe: ['🤝', 'spent resources on influence'],
  vote: ['🗳️', 'voted'],
  veto: ['✋', 'used a veto'],
}

/** One line per event. No amounts, targets or vote choices: the public event does not carry them. */
export function eventText(e: ArenaEvent, factions: ArenaState['factions']) {
  const name = factions.find((f) => f.idx === e.actor)?.name ?? 'A faction'
  if (!e.ok) return `⚠️ ${name}'s move was rejected`
  const [emoji, text] = VERBS[e.action] ?? ['•', `did ${e.action}`]
  return `${emoji} ${name} ${text}`
}

// A finished game: GET /game/:id/state returns {ok, finished: true, result} and no `state`.
// ranks[place] is a faction index into agents (arena/src/api.rs settlement record).
export type { GameResult }

/** Final standings, best first. No balances: the scene stays free of amounts. */
export function standings(r: GameResult) {
  return r.ranks.map((fi) => r.agents[fi]?.name ?? `Faction ${fi + 1}`)
}

export function resultText(r: GameResult) {
  const [winner] = standings(r)
  return winner ? `🏁 Game over. ${winner} wins.` : '🏁 Game over.'
}

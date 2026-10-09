import type { ChainEvent, ChainFaction, ChainSnapshot } from './client.ts'
export type VisualAction = 'buy' | 'sell' | 'mule' | 'bribe' | 'vote' | 'victory'
export type LiveCursor = { ready: boolean; tailId: string | null; seen: Set<string> }
export function newLiveCursor(): LiveCursor { return { ready: false, tailId: null, seen: new Set() } }
export function liveAdditions(cursor: LiveCursor, events: ChainEvent[], complete: boolean): ChainEvent[] {
  const fresh = new Set(events.filter((e) => !cursor.seen.has(e.id)).map((e) => e.id))
  for (const event of events) cursor.seen.add(event.id)
  if (!complete) return []
  const prior = cursor.tailId === null ? -1 : events.findIndex((e) => e.id === cursor.tailId)
  const lostTail = cursor.tailId !== null && prior < 0
  cursor.tailId = events.at(-1)?.id ?? null
  if (!cursor.ready || lostTail) { cursor.ready = true; return [] }
  return events.slice(prior + 1).filter((e) => fresh.has(e.id))
}
export function visualAction(event: ChainEvent, faction: ChainFaction): VisualAction | null {
  if (event.type === 'payout' && event.wallet === faction.wallet && event.rank === 0) return 'victory'
  if (event.faction !== faction.pda && event.from !== faction.pda) return null
  if (event.type === 'goods_bought') return 'buy'
  if (event.type === 'sold' || event.type === 'sold_credit_ev') return 'sell'
  if (event.type === 'donkey_bought' || event.type === 'shuttled_ev') return 'mule'
  if (event.type === 'bribe_given') return 'bribe'
  if (event.type === 'vote_cast') return 'vote'
  return null
}
export function playerEvents(events: ChainEvent[], faction: ChainFaction): ChainEvent[] {
  return events.filter((e) => e.faction === faction.pda || e.from === faction.pda || e.wallet === faction.wallet || ['phase_advanced', 'law_drawn', 'law_result', 'settled', 'game_initialized', 'game_aborted'].includes(e.type))
}
export function eventLabel(event: ChainEvent, faction: ChainFaction): string {
  const verb = visualAction(event, faction)
  if (verb === 'victory') return `${faction.name} won · confirmed payout`
  if (verb === 'buy') return `${faction.name} bought ${event.units ?? 'goods'}`
  if (verb === 'sell') return `${faction.name} sold ${event.units ?? 'goods'}`
  if (verb === 'mule') return `${faction.name} received goods via Mule`
  if (verb === 'bribe') return `${faction.name} gave a bribe`
  if (verb === 'vote') return `${faction.name} voted ${['Yes', 'No', 'Abstain'][event.choice ?? -1] ?? ''}`.trim()
  if (event.type === 'produced') return `${faction.name} produced goods`
  if (event.type === 'phase_advanced') return `${event.phase ?? 'Phase change'} · round ${event.round ?? '—'}`
  if (event.type === 'settled') return 'Game settled on-chain'
  if (event.type === 'game_aborted') return 'Game aborted on-chain'
  return event.type.replaceAll('_', ' ')
}

export function defaultWinner(snapshot: ChainSnapshot): ChainFaction | undefined {
  if (!snapshot.settled || !snapshot.complete) return undefined
  const winners = snapshot.events.filter((e) => e.type === 'payout' && e.rank === 0)
  return winners.length === 1 ? snapshot.factions.find((f) => f.wallet === winners[0].wallet) : undefined
}

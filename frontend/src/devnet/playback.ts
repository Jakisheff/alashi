import { formatCash, type ChainEvent, type ChainFaction, type ChainSnapshot } from './client.ts'
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
// Card IDs mirror rules/src/constants.rs and arena/src/api.rs::law_name.
const lawNames = ['Status quo', 'Tax 10%', 'Tax 20%', 'Production subsidy', 'Subsidy for the poorest', 'Subsidy for the richest', 'Embargo', 'Boom', 'Mutual offset']
export function eventLabel(event: ChainEvent, faction: ChainFaction): string {
  if (event.type === 'law_drawn' || event.type === 'law_vetoed') return `Law ${event.type === 'law_drawn' ? 'drawn' : 'vetoed'} · ${event.card === undefined ? 'card unavailable' : lawNames[event.card] ?? `Card #${event.card}`}`
  if (event.type === 'law_result') return `Law ${event.passed === undefined ? 'result' : event.passed ? 'passed' : 'rejected'}${event.yes !== undefined && event.no !== undefined ? ` · Yes ${event.yes} / No ${event.no}` : ''}`
  const verb = visualAction(event, faction)
  if (verb === 'victory') return `${faction.name} won · confirmed payout`
  if (verb === 'buy') return `${faction.name} bought ${event.units !== undefined ? `${event.units} goods` : 'goods'}${event.cost !== undefined ? ` · ${formatCash(event.cost)}` : ''}`
  if (verb === 'sell') return `${faction.name} sold ${event.units !== undefined ? `${event.units} goods` : 'goods'}${event.revenue !== undefined ? ` · ${formatCash(event.revenue)}` : ''}`
  if (verb === 'mule') return `${faction.name} received ${event.type === 'shuttled_ev' ? 'grey goods via Shuttle' : 'goods via Mule'}`
  if (verb === 'bribe') return `${faction.name} gave a bribe${event.amount !== undefined ? ` · ${formatCash(event.amount)}` : ''}`
  if (verb === 'vote') return `${faction.name} voted ${['Yes', 'No', 'Abstain'][event.choice ?? -1] ?? ''}`.trim()
  if (event.type === 'produced') return `${faction.name} produced ${event.goods !== undefined ? `${event.goods} goods` : 'goods'}`
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

export function playerStandings(snapshot: ChainSnapshot): { rows: { faction: ChainFaction; rank: number | null }[]; final: boolean } {
  if (snapshot.settled && snapshot.complete) {
    const payouts = snapshot.events.filter((e) => e.type === 'payout')
    const ranks = new Map<number, ChainFaction>(), seenFactions = new Set<string>()
    for (const event of payouts) {
      const faction = snapshot.factions.find((f) => f.wallet === event.wallet)
      if (event.rank === undefined || event.rank < 0 || event.rank >= snapshot.factions.length || !faction || ranks.has(event.rank) || seenFactions.has(faction.pda)) break
      ranks.set(event.rank, faction)
      seenFactions.add(faction.pda)
    }
    if (ranks.size === payouts.length) {
      const rows: { faction: ChainFaction; rank: number | null }[] = [...ranks].sort(([a], [b]) => a - b).map(([rank, faction]) => ({ faction, rank }))
      rows.push(...snapshot.factions.filter((faction) => !seenFactions.has(faction.pda)).map((faction) => ({ faction, rank: null })))
      return { rows, final: true }
    }
    return { rows: snapshot.factions.map((faction) => ({ faction, rank: null })), final: true }
  }
  const cashOrder = [...snapshot.factions].sort((a, b) => {
    const cashA = BigInt(a.cash), cashB = BigInt(b.cash)
    return cashA === cashB ? a.pda.localeCompare(b.pda) : cashA > cashB ? -1 : 1
  })
  return { rows: cashOrder.map((faction) => ({ faction, rank: null })), final: false }
}

import type { ChainSnapshot } from './client.ts'
import type { MapTeam } from '../strategies/Map'
import type { Stage } from '../strategies/timeline.ts'
const station: Record<string, number> = { sold: 0, sold_credit_ev: 0, goods_bought: 0, produced: 1, donkey_bought: 1, shuttled_ev: 1, vote_cast: 2, bribe_given: 3 }
// Input has already passed decodeSnapshot's confirmed-program/event identity gates.
export function chainTradingTeams(snapshot: ChainSnapshot, observed: Map<string, number>, replay: boolean): MapTeam[] {
  const events = [...new Map(snapshot.events.map(e => [e.id, e])).values()]
  return snapshot.factions.slice(0, 2).map(f => {
    const stages: Stage[] = []
    for (const [index, event] of events.entries()) {
      if ((event.faction ?? event.from) !== f.pda || station[event.type] === undefined) continue
      if ((event.type === 'sold' || event.type === 'sold_credit_ev') && (event.units === undefined || event.revenue === undefined)) continue
      if (event.type === 'goods_bought' && (event.units === undefined || event.cost === undefined)) continue
      const route = station[event.type], last = stages.filter(s => s.route === route).at(-1)
      const start = Math.max(replay ? index * .45 : observed.get(event.id) ?? 0, last?.end ?? 0)
      stages.push({ task: event.type === 'sold' || event.type === 'sold_credit_ev' ? 'sell' : event.type === 'goods_bought' ? 'buy' : event.type === 'vote_cast' ? 'vote' : event.type === 'bribe_given' ? 'bribe' : 'produce', route, start, work: start + .8, end: start + 2.4, ok: true, tool: false })
    }
    return { id: `${snapshot.pda}:${f.pda}:${replay ? 'replay' : 'live'}`, stages, completed: false }
  })
}

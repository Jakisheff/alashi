import type { ArenaEvent } from './client'
import type { MapTeam } from '../strategies/Map'
import type { Stage } from '../strategies/timeline'
const station: Record<string, number> = { sell: 0, sell_credit: 0, buy: 0, barter_propose: 0, barter_accept: 0, produce: 1, donkey: 1, shuttle: 1, vote: 2, veto: 2, bribe: 3, buy_hard: 3, sell_hard: 3 }
export function tradingTeams(game: number, events: ArenaEvent[], received: Map<string, number>): MapTeam[] {
  const actions = [...new Map(events.filter(e => e.kind === 'game_action' && e.confirmed && e.changes && e.actor !== undefined && e.actor < 2 && station[e.action ?? ''] !== undefined).map(e => [e.id, e])).values()].sort((a, b) => a.seq - b.seq)
  return [0, 1].map(actor => {
    const stages: Stage[] = []
    for (const e of actions.filter(e => e.actor === actor)) {
      const route = station[e.action!], last = stages.filter(s => s.route === route).at(-1)
      const start = Math.max(received.get(e.id) ?? 0, last?.end ?? 0)
      stages.push({ task: e.action!, route, start, work: start + .8, end: start + 2.4, ok: true, tool: false })
    }
    return { id: `${game}:${actor}`, stages, completed: false }
  })
}
export const gameCash = (value: number) => (value / 1_000_000).toLocaleString('en-US', { maximumFractionDigits: 6 })
export function receiptText(e: ArenaEvent) {
  const change = e.changes?.find(c => c.actor === e.actor)
  if (!change) return e.changes ? 'Confirmed; no balance change for this actor' : 'Historical result unavailable'
  const delta = change.cash_after - change.cash_before
  return `Game cash ${gameCash(change.cash_before)} → ${gameCash(change.cash_after)} (${delta >= 0 ? '+' : ''}${gameCash(delta)}); goods ${change.goods_before} → ${change.goods_after}`
}

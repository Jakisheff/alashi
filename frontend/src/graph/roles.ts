import { symbol, symbolCircle, symbolDiamond, symbolSquare, symbolStar, symbolTriangle, type SymbolType } from 'd3'
import type { AgentRole, InteractionKind } from './types'

// Legend order and filters: rare meaningful roles first, the mass ones last (HackAlem roles.ts pattern).
export const ROLE_ORDER: AgentRole[] = ['president', 'broker', 'patron', 'client', 'trader', 'loner']

export const ROLE_TITLE: Record<AgentRole, string> = {
  president: 'President',
  broker: 'Broker',
  patron: 'Patron (pays bribes)',
  client: 'Client (takes bribes)',
  trader: 'Trader',
  loner: 'Loner / waiting',
}

// Colour is not the only carrier of meaning: every role also has its own shape (same in legend and graph).
export const ROLE_COLOR: Record<AgentRole, string> = {
  president: '#ffd86b',
  broker: '#8ec5ff',
  patron: '#f5a3c7',
  client: '#c9b6ff',
  trader: '#7fe0d4',
  loner: '#a1a1aa',
}

const ROLE_SYMBOL: Record<AgentRole, { type: SymbolType; rotate: number }> = {
  president: { type: symbolStar, rotate: 0 },
  broker: { type: symbolDiamond, rotate: 0 },
  patron: { type: symbolTriangle, rotate: 90 },
  client: { type: symbolTriangle, rotate: 180 },
  trader: { type: symbolCircle, rotate: 0 },
  loner: { type: symbolSquare, rotate: 0 },
}

/** SVG path of the role shape centred at 0,0 with the area of a circle of radius r; no role: circle. */
export function roleSymbolPath(role: AgentRole | null | undefined, r: number) {
  const s = role ? ROLE_SYMBOL[role] : { type: symbolCircle, rotate: 0 }
  return { d: symbol(s.type, Math.PI * r * r)() ?? '', rotate: s.rotate }
}

export const KIND_TITLE: Record<InteractionKind, string> = {
  trade: 'trade',
  barter: 'barter',
  bribe: 'bribe',
  vote_offer: 'vote offer',
  credit: 'credit',
}

/** Simulated game cash, compact: the log's formatter, so both pages round alike */
export { cash as formatCash } from '../log/model'

/** Dark graph-page tokens (the graph keeps its own dark canvas, like HackAlem). */
export const ui = {
  panel: 'border-[#2e2e36] bg-[#1f1f25]',
  muted: 'text-[#a1a1aa]',
  chip: 'border-[#2e2e36] bg-[#27272e]',
}

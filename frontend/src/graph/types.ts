// Agent network graph: data contract the page renders. Mock data today (mock.ts); the backend is expected to serve
// the same shape later (proposal on BOARD, frontend-agent-graph). Model: HackAlem "money graph" (D3 force), mapped to
// Alashi: node = agent, cluster = game party, edge = agent -> agent interactions aggregated, timeline = rounds.
//
// Public events today carry no targets or amounts (PUBLIC_EVENTS_FOR_DIN). This graph needs both, so it stays on mock
// data until the backend decides what may be public. All money here is simulated HTTP-game cash.

/** What an agent mostly does, derived by the backend from its interactions. Shape and colour on the graph. */
export type AgentRole = 'president' | 'broker' | 'patron' | 'client' | 'trader' | 'loner'

export type InteractionKind = 'trade' | 'barter' | 'bribe' | 'vote_offer' | 'credit'

export type GraphAgent = {
  /** agent_record_id (64 hex), stable across games */
  id: string
  name: string
  /** Self-reported harness/model, e.g. "codex/gpt-6-sol". Not attested. */
  model: string
  role: AgentRole | null
  /** Party the agent currently plays in (cluster id), null when waiting */
  party: number | null
  /** 0..1, node size: final or current rank-weighted score */
  score: number
  cash: number
  influence: number
  /** Simulated cash received from / sent to other agents, and counterpart counts */
  in_cash: number
  out_cash: number
  in_deg: number
  out_deg: number
  alive: boolean
}

/** All interactions from src to dst in the shown window, aggregated. Direction = who initiated / paid. */
export type GraphEdge = {
  src: string
  dst: string
  count: number
  /** Simulated cash moved src -> dst (0 for pure votes) */
  sum: number
  kinds: InteractionKind[]
}

/** One interaction with its round: feeds the timelapse and the per-round histogram. */
export type GraphInteraction = {
  src: string
  dst: string
  round: number
  kind: InteractionKind
  amount: number
  ok: boolean
}

export type GraphParty = {
  /** Cluster id used on the graph */
  id: number
  game_id: number
  party_no: number
  label: string
  n_agents: number
  round: number
  phase: string
  finished: boolean
}

export type GraphData = {
  agents: GraphAgent[]
  edges: GraphEdge[]
  interactions: GraphInteraction[]
  parties: GraphParty[]
  /** Agent ids whose labels are always shown */
  top: string[]
  rounds: number
}

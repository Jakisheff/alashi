import type { AgentRole, GraphAgent, GraphData, GraphEdge, GraphInteraction, GraphParty, InteractionKind } from './types'

// Labelled mock network for the graph page until the backend serves agent -> agent interactions.
// Seeded, so every load shows the same picture (screenshots and reviews stay comparable).

const HARNESS = ['codex/gpt-6-sol', 'opencode/glm-5.3-flash', 'claude-code/opus', 'dsh/qwen', 'muse/m1', 'grokbot/g4', 'hermes/h2']
const NAMES = ['CodexSol', 'OpenCodeGLM', 'ClaudeDegen', 'DshQwen', 'Muse', 'Grokbot', 'Hermes', 'Aitore', 'Aikorkem', 'Baige', 'Saryarka', 'Tulpar']
const KINDS: [InteractionKind, number][] = [['trade', 5], ['barter', 2], ['bribe', 2], ['vote_offer', 2], ['credit', 1]]
const ROUNDS = 12

function rng(seed: number) {
  // mulberry32
  return () => {
    seed |= 0
    seed = (seed + 0x6d2b79f5) | 0
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed)
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

/** large: ~3K agents in ~110 parties with a few big ones (HackAlem-scale stress test for the renderer). */
export function mockGraph(seed = 7, scale: 'small' | 'large' = 'small'): GraphData {
  const PARTIES = scale === 'large' ? 110 : 10
  const r = rng(seed)
  const pick = <T,>(a: T[]) => a[Math.floor(r() * a.length)]
  const kind = (): InteractionKind => {
    let x = r() * KINDS.reduce((s, [, w]) => s + w, 0)
    for (const [k, w] of KINDS) if ((x -= w) < 0) return k
    return 'trade'
  }
  const hex = () => Array.from({ length: 64 }, () => Math.floor(r() * 16).toString(16)).join('')

  const parties: GraphParty[] = []
  const agents: GraphAgent[] = []
  const interactions: GraphInteraction[] = []
  let n = 0
  const agent = (party: number | null): GraphAgent => {
    const name = `${NAMES[n % NAMES.length]}${n >= NAMES.length ? `-${Math.floor(n / NAMES.length) + 1}` : ''}`
    n++
    const a: GraphAgent = {
      id: hex(), name, model: pick(HARNESS), role: null, party, score: 0, cash: 0, influence: 1 + Math.floor(r() * 3),
      in_cash: 0, out_cash: 0, in_deg: 0, out_deg: 0, alive: r() > 0.06,
    }
    agents.push(a)
    return a
  }

  for (let p = 0; p < PARTIES; p++) {
    // large: mostly small parties, a long tail of big ones (up to ~120 agents)
    const size = scale === 'large' ? 3 + Math.floor(r() ** 4 * 110) : 3 + Math.floor(r() * 9)
    const finished = p < PARTIES * 0.7
    const round = finished ? ROUNDS : 3 + Math.floor(r() * 8)
    const game = Math.floor(p / 2) + 1
    parties.push({
      id: p + 1, game_id: game, party_no: 19 + p, label: `game ${game} · party ${19 + p}`, n_agents: size, round,
      phase: finished ? 'finished' : pick(['market', 'action', 'law']), finished,
    })
    const members = Array.from({ length: size }, () => agent(p + 1))
    // A busy core makes brokers and patrons visible: a few agents take most interactions.
    const weight = members.map(() => 0.3 + r() ** 2 * 3)
    const total = weight.reduce((s, w) => s + w, 0)
    const member = () => {
      let x = r() * total
      for (let i = 0; i < members.length; i++) if ((x -= weight[i]) < 0) return members[i]
      return members[0]
    }
    for (let rd = 1; rd <= round; rd++) {
      const count = Math.floor(r() * size * 0.9)
      for (let i = 0; i < count; i++) {
        const a = member()
        const b = member()
        if (a === b) continue
        const k = kind()
        const amount = k === 'vote_offer' ? 0 : Math.round((0.5 + r() * 6) * 1e6)
        interactions.push({ src: a.id, dst: b.id, round: rd, kind: k, amount, ok: r() > 0.1 })
      }
    }
  }
  for (let i = 0; i < 4; i++) agent(null) // registered, waiting for a game: isolated nodes

  // Aggregate accepted interactions into edges and per-agent totals.
  const byId = new Map(agents.map((a) => [a.id, a]))
  const edges = new Map<string, GraphEdge>()
  const bribesOut = new Map<string, number>()
  const bribesIn = new Map<string, number>()
  for (const t of interactions) {
    if (!t.ok) continue
    const key = `${t.src}|${t.dst}`
    const e = edges.get(key) ?? { src: t.src, dst: t.dst, count: 0, sum: 0, kinds: [] }
    e.count++
    e.sum += t.amount
    if (!e.kinds.includes(t.kind)) e.kinds.push(t.kind)
    edges.set(key, e)
    byId.get(t.src)!.out_cash += t.amount
    byId.get(t.dst)!.in_cash += t.amount
    if (t.kind === 'bribe') {
      bribesOut.set(t.src, (bribesOut.get(t.src) ?? 0) + 1)
      bribesIn.set(t.dst, (bribesIn.get(t.dst) ?? 0) + 1)
    }
  }
  for (const e of edges.values()) {
    byId.get(e.src)!.out_deg++
    byId.get(e.dst)!.in_deg++
  }

  // Roles: one president per party (most influence), then by dominant behaviour.
  for (const a of agents) {
    a.cash = Math.round((10 + r() * 20) * 1e6 + a.in_cash - a.out_cash * 0.6)
    const deg = a.in_deg + a.out_deg
    let role: AgentRole = 'trader'
    if (deg === 0) role = 'loner'
    else if (deg >= 6) role = 'broker'
    else if ((bribesOut.get(a.id) ?? 0) >= 2) role = 'patron'
    else if ((bribesIn.get(a.id) ?? 0) >= 2) role = 'client'
    a.role = role
  }
  for (const p of parties) {
    const members = agents.filter((a) => a.party === p.id)
    members.reduce((b, a) => (a.influence > b.influence ? a : b), members[0]).role = 'president'
  }
  const cash = agents.map((a) => a.cash)
  const lo = Math.min(...cash)
  const span = Math.max(...cash) - lo || 1
  for (const a of agents) a.score = (a.cash - lo) / span

  const top = [...agents].sort((a, b) => b.score - a.score).slice(0, 8).map((a) => a.id)
  return { agents, edges: [...edges.values()], interactions, parties, top, rounds: ROUNDS }
}

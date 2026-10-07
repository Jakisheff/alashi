// Game log: one exchange-style tape built from what GET /game/:id/state really returns.
// Finished game: {finished, result} with registrations, phases (law votes), actions (params, cash_after) and settle.
// Live game: {state} with only the last 12 public actions (no params, no amounts). Pure functions: scripts/log-check.ts.

export type Registration = { signature: string; slot: number; fee_lamports: string; network: string; commitment: string }

export type ResultAgent = { name: string; model?: string; registration?: Registration | null }

export type ResultAction = {
  seq: number
  round: number
  phase: string
  actor: number
  action: string
  ok: boolean
  err: string | null
  params: Record<string, unknown> | null
  cash_after: number | null
  goods_after: number | null
  ts: number
}

export type ResultPhase = {
  phase: string
  round: number
  card?: number
  card_name?: string
  yes?: number
  no?: number
  passed?: boolean
}

export type GameResult = {
  game_id: number
  party_no: number
  label?: string
  agents: ResultAgent[]
  actions: ResultAction[]
  phases: ResultPhase[]
  ranks: number[]
  payouts: number[]
  final_cash: number[]
  bank: number
  rake: number
  entry_fee: number
  finished_at: number
}

export type LiveAction = { seq: number; round: number; phase: string; actor: number | null; action: string; ok: boolean; ts: number }

export type LiveState = {
  game_id: number
  party_no: number
  round: number
  phase: string
  factions: { idx: number; name: string }[]
  recent_actions: LiveAction[]
}

export type StateResponse = { ok: boolean; state?: LiveState; finished?: boolean; result?: GameResult; error?: string }

export type EntryKind = 'chain' | 'phase' | 'move' | 'law' | 'settle'

export type LogEntry = {
  key: string
  kind: EntryKind
  ts: number | null
  round: number
  phase: string
  actor?: string
  text: string
  detail?: string
  ok?: boolean
  /** Simulated cash change caused by this move, when the arena recorded cash_after */
  delta?: number
  href?: string
}

export type Player = { idx: number; name: string; model?: string; rank?: number; cash?: number; payout?: number; registration?: Registration | null }

export type GameLog = {
  status: 'final' | 'live'
  game_id: number
  party_no: number
  label?: string
  round: number
  phase: string
  bank?: number
  rake?: number
  entry_fee?: number
  players: Player[]
  /** Oldest first */
  entries: LogEntry[]
  /** Live: the public window holds only the last 12 actions */
  partial: boolean
}

/** Simulated game cash: 22M, 340K. */
export const cash = (v: number) => {
  const s = v < 0 ? '−' : ''
  const a = Math.abs(v)
  return s + (a >= 1e6 ? `${+(a / 1e6).toFixed(2)}M` : a >= 1e3 ? `${Math.round(a / 1e3)}K` : String(a))
}

const title = (s: string) => s.replace(/_/g, ' ')

/** What a move did, in words. Unknown params are shown as-is rather than guessed. */
export function moveText(action: string, params: Record<string, unknown> | null, names: string[]) {
  const p = params ?? {}
  const units = typeof p.units === 'number' ? p.units : null
  const to = typeof p.to === 'number' ? (names[p.to] ?? `#${p.to}`) : null
  switch (action) {
    case 'produce':
      return 'produced goods'
    case 'sell':
      return units ? `sold ${units} goods` : 'sold goods'
    case 'buy':
      return units ? `bought ${units} goods` : 'bought goods'
    case 'vote':
      return typeof p.choice === 'string' ? `voted ${p.choice}` : 'voted'
    case 'veto':
      return 'used a veto'
    case 'donkey':
      return 'bought a donkey'
    case 'bribe':
      return `bribed${to ? ` ${to}` : ''}${typeof p.amount === 'number' ? ` · ${cash(p.amount)}` : ''}`
    case 'roof':
      return `took protection${to ? ` from ${to}` : ''}`
    default: {
      const rest = Object.entries(p).map(([k, v]) => `${k}=${typeof v === 'object' ? JSON.stringify(v) : String(v)}`)
      return `${title(action)}${rest.length ? ` (${rest.join(', ')})` : ''}`
    }
  }
}

export function explorerTx(r: Registration) {
  return `https://explorer.solana.com/tx/${r.signature}?cluster=${r.network}`
}

/** Finished game: registrations, then each phase with its moves in order, law outcomes, settlement. */
export function fromResult(r: GameResult): GameLog {
  const names = r.agents.map((a, i) => a.name || `Faction ${i + 1}`)
  const entries: LogEntry[] = []
  r.agents.forEach((a, i) => {
    if (!a.registration?.signature) return
    const sig = a.registration.signature
    entries.push({
      key: `reg-${i}`, kind: 'chain', ts: null, round: 0, phase: 'lobby', actor: names[i],
      text: `registered on Solana ${a.registration.network}`,
      detail: `tx ${sig.slice(0, 8)}…${sig.slice(-6)} · slot ${a.registration.slot} · fee ${a.registration.fee_lamports} lamports`,
      ok: a.registration.commitment === 'confirmed' || a.registration.commitment === 'finalized',
      href: explorerTx(a.registration),
    })
  })
  const moves = [...r.actions].sort((a, b) => a.seq - b.seq)
  const lastCash = new Map<number, number>()
  let lastTs: number | null = null
  r.phases.forEach((ph, i) => {
    const inPhase = moves.filter((m) => m.round === ph.round && m.phase === ph.phase)
    if (ph.phase !== 'lobby') {
      entries.push({ key: `ph-${i}`, kind: 'phase', ts: inPhase[0]?.ts ?? lastTs, round: ph.round, phase: ph.phase, text: `round ${ph.round} · ${ph.phase} phase` })
    }
    for (const m of inPhase) {
      let delta: number | undefined
      if (m.cash_after !== null) {
        const prev = lastCash.get(m.actor)
        if (prev !== undefined && prev !== m.cash_after) delta = m.cash_after - prev
        lastCash.set(m.actor, m.cash_after)
      }
      const after = [m.cash_after !== null ? `cash ${cash(m.cash_after)}` : '', m.goods_after !== null ? `goods ${m.goods_after}` : '']
      entries.push({
        key: `mv-${m.seq}`, kind: 'move', ts: m.ts, round: m.round, phase: m.phase, actor: names[m.actor] ?? `#${m.actor}`,
        text: m.ok ? moveText(m.action, m.params, names) : `${title(m.action)} rejected`,
        detail: m.ok ? after.filter(Boolean).join(' · ') || undefined : (m.err ?? undefined),
        ok: m.ok, delta,
      })
      lastTs = m.ts
    }
    if (ph.phase === 'law' && ph.card_name) {
      entries.push({
        key: `law-${i}`, kind: 'law', ts: lastTs, round: ph.round, phase: 'law',
        text: `law “${title(ph.card_name)}” ${ph.passed ? 'passed' : 'rejected'}`,
        detail: `${ph.yes ?? 0} yes · ${ph.no ?? 0} no`, ok: !!ph.passed,
      })
    }
  })
  entries.push({
    key: 'settle', kind: 'settle', ts: r.finished_at, round: r.phases.at(-1)?.round ?? 0, phase: 'finished',
    text: `game settled · winner ${names[r.ranks[0]] ?? '—'}`,
    detail: `bank ${cash(r.bank)} · rake ${cash(r.rake)} · ` + r.ranks.map((fi, place) => `${place + 1}. ${names[fi]} +${cash(r.payouts[place] ?? 0)}`).join(' · '),
    ok: true,
  })
  return {
    status: 'final', game_id: r.game_id, party_no: r.party_no, label: r.label, round: r.phases.at(-1)?.round ?? 0, phase: 'finished',
    bank: r.bank, rake: r.rake, entry_fee: r.entry_fee, partial: false, entries,
    players: names.map((name, idx) => {
      const place = r.ranks.indexOf(idx)
      return {
        idx, name, model: r.agents[idx]?.model, registration: r.agents[idx]?.registration,
        rank: place >= 0 ? place + 1 : undefined, cash: r.final_cash[idx], payout: place >= 0 ? r.payouts[place] : undefined,
      }
    }),
  }
}

/** Live game: only the public window of recent actions (no params, no amounts). */
export function fromState(s: LiveState): GameLog {
  const names = new Map(s.factions.map((f) => [f.idx, f.name]))
  const entries: LogEntry[] = [...s.recent_actions]
    .sort((a, b) => a.seq - b.seq)
    .map((a) => ({
      key: `mv-${a.seq}`, kind: 'move' as const, ts: a.ts, round: a.round, phase: a.phase,
      actor: a.actor === null ? 'system' : (names.get(a.actor) ?? `#${a.actor}`),
      text: a.ok ? moveText(a.action, null, []) : `${title(a.action)} rejected`, ok: a.ok,
    }))
  return {
    status: 'live', game_id: s.game_id, party_no: s.party_no, round: s.round, phase: s.phase, partial: true, entries,
    players: s.factions.map((f) => ({ idx: f.idx, name: f.name })),
  }
}

export function toLog(r: StateResponse): GameLog | null {
  if (r.finished && r.result) return fromResult(r.result)
  if (r.state) return fromState(r.state)
  return null
}

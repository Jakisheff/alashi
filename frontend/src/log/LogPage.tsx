import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useEffect, useMemo, useRef, useState } from 'react'
import { cash, toLog, type EntryKind, type GameLog, type LogEntry, type StateResponse } from './model'

// Game feed as one neon board (Cyberpunk-2077-style HUD): stat readouts, a scrolling ticker, a leaderboard and the
// event tape, newest first. Finished games come with the full recorded history; live games expose only the last 12
// public actions, so live rows accumulate from the moment the page is opened.

type Filter = 'all' | 'moves' | 'decisions' | 'phases' | 'chain'
const FILTERS: [Filter, string][] = [
  ['all', 'All'],
  ['moves', 'Moves'],
  ['decisions', 'Decisions'],
  ['phases', 'Phases'],
  ['chain', 'On-chain'],
]
const pass = (f: Filter, e: LogEntry) =>
  f === 'all' ||
  (f === 'moves' && e.kind === 'move') ||
  (f === 'decisions' && (e.kind === 'law' || e.kind === 'settle' || /^voted|veto/.test(e.text))) ||
  (f === 'phases' && (e.kind === 'phase' || e.kind === 'settle')) ||
  (f === 'chain' && e.kind === 'chain')

// Neon palette: yellow = signal, cyan = data, red = alarm, violet = chain, green = credit
const Y = '#fcee0a'
const C = '#00f0ff'
const RED = '#ff003c'
const VIO = '#b026ff'
const GRN = '#39ff88'
const BADGE: Record<EntryKind, [string, string]> = {
  chain: ['TX', VIO],
  phase: ['PHASE', '#6b7280'],
  move: ['MOVE', C],
  law: ['LAW', Y],
  settle: ['SETTLE', GRN],
}
/** Cut corners like the in-game HUD panels */
const cut = (px = 12) => ({ clipPath: `polygon(0 0, calc(100% - ${px}px) 0, 100% ${px}px, 100% 100%, ${px}px 100%, 0 calc(100% - ${px}px))` })
const glow = (c: string) => ({ color: c, textShadow: `0 0 6px ${c}99, 0 0 18px ${c}44` })

const time = (ts: number | null) =>
  ts === null ? '—' : new Date(ts * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })
const REPLAY_MS = 450

export function LogPage({ game, api = '' }: { game: string; api?: string }) {
  const q = useQuery({
    queryKey: ['game-log', api, game],
    refetchInterval: (query) => (query.state.data?.finished ? false : 2000),
    queryFn: async () => {
      const res = await fetch(`${api}/game/${game}/state`)
      if (!res.ok) throw new Error(`state ${res.status}`)
      return (await res.json()) as StateResponse
    },
  })
  const log = useMemo(() => (q.data ? toLog(q.data) : null), [q.data])

  // Live: keep every row seen since opening (the public window slides by 12). Adjusted during render, not in an effect.
  const [seen, setSeen] = useState<{ from: GameLog | null; rows: Map<string, LogEntry> }>({ from: null, rows: new Map() })
  if (log?.status === 'live' && seen.from !== log) {
    const rows = new Map(seen.rows)
    for (const e of log.entries) rows.set(e.key, e)
    setSeen({ from: log, rows })
  }
  const entries = log?.status === 'live' ? [...seen.rows.values()] : (log?.entries ?? [])

  // Replay a finished game as if the rows were arriving now
  const [shown, setShown] = useState<number | null>(null)
  useEffect(() => {
    if (shown === null || shown >= entries.length) return
    const id = setTimeout(() => setShown(shown + 1), REPLAY_MS)
    return () => clearTimeout(id)
  }, [shown, entries.length])
  const visible = shown === null ? entries : entries.slice(0, shown)

  const [filter, setFilter] = useState<Filter>('all')
  const rows = useMemo(() => visible.filter((e) => pass(filter, e)).reverse(), [visible, filter])

  // Rows that appear after the first paint flash once
  const [known, setKnown] = useState<Set<string> | null>(null)
  const keys = rows.map((e) => e.key).join('|')
  const prevKeys = useRef('')
  useEffect(() => {
    if (keys === prevKeys.current) return
    prevKeys.current = keys
    const t = setTimeout(() => setKnown(new Set(keys.split('|'))), 1600)
    return () => clearTimeout(t)
  }, [keys])
  const isFresh = (k: string) => known !== null && !known.has(k)

  const moves = entries.filter((e) => e.kind === 'move')
  const accepted = moves.filter((e) => e.ok).length
  const laws = entries.filter((e) => e.kind === 'law')
  const latest = [...visible].reverse().filter((e) => e.kind !== 'phase').slice(0, 10)
  const maxCash = Math.max(1, ...(log?.players.map((p) => p.cash ?? 0) ?? [1]))

  return (
    <main className="cp relative flex min-h-dvh flex-col overflow-hidden bg-[#06060a] text-[#e8e6e3]">
      <style>{`
        @font-face { font-family: Jura; src: url(${import.meta.env.BASE_URL}fonts/Jura.ttf); font-display: swap; }
        .cp { font-family: Jura, ui-sans-serif, system-ui, sans-serif; }
        .cp .mono { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
        .cp-grid { background-image: linear-gradient(rgba(0,240,255,.05) 1px, transparent 1px), linear-gradient(90deg, rgba(0,240,255,.05) 1px, transparent 1px); background-size: 40px 40px; }
        .cp-scan::after { content: ''; position: absolute; inset: 0; pointer-events: none; z-index: 30;
          background: repeating-linear-gradient(to bottom, rgba(255,255,255,.025) 0 1px, transparent 1px 3px); mix-blend-mode: overlay; }
        @keyframes cpIn { 0% { opacity: 0; transform: translateX(-8px); background: rgba(252,238,10,.28) }
          12% { opacity: 1; transform: translateX(3px) } 18% { transform: translateX(-2px); clip-path: inset(0 0 40% 0) }
          24% { transform: none; clip-path: inset(0) } 100% { background: transparent } }
        .cp-in { animation: cpIn 1.4s ease-out }
        @keyframes cpTicker { from { transform: translateX(0) } to { transform: translateX(-50%) } }
        .cp-ticker { animation: cpTicker 40s linear infinite }
        @keyframes cpBlink { 50% { opacity: .25 } }
        .cp-blink { animation: cpBlink 1.2s steps(1) infinite }
        @media (prefers-reduced-motion: reduce) { .cp-in, .cp-ticker, .cp-blink { animation: none } }
      `}</style>
      <div className="cp-grid pointer-events-none absolute inset-0" />
      <div className="cp-scan pointer-events-none absolute inset-0" />

      {/* top bar */}
      <header className="relative z-10 flex flex-wrap items-center gap-x-4 gap-y-2 border-b px-4 py-3" style={{ borderColor: `${Y}55` }}>
        <Link to="/" className="text-lg font-bold tracking-[.25em] uppercase" style={glow(Y)}>
          alashi
        </Link>
        <span className="text-xs tracking-[.4em] uppercase" style={{ color: C }}>
          // game feed
        </span>
        <GamePicker game={game} />
        {log && <StatusChip log={log} />}
        <span className="ml-auto text-[11px] tracking-wider text-[#8a8f98] uppercase">sim balances · on-chain: registrations only</span>
      </header>

      {q.isPending && <p className="relative z-10 p-6 text-sm tracking-widest uppercase" style={glow(C)}>Connecting to game {game}…</p>}
      {q.isError && (
        <p className="relative z-10 p-6 text-sm uppercase" style={glow(RED)}>
          Link down: {(q.error as Error).message}
        </p>
      )}
      {q.data && !log && (
        <p className="relative z-10 p-6 text-sm uppercase" style={glow(RED)}>
          Game {game} not found{q.data.error ? ` · ${q.data.error}` : ''}
        </p>
      )}

      {log && (
        <>
          {/* readouts */}
          <div className="relative z-10 grid grid-cols-2 gap-2 px-4 pt-4 sm:grid-cols-3 lg:grid-cols-6">
            <Readout label="Round" value={String(log.round)} sub={log.phase} color={Y} />
            <Readout label="Moves" value={`${accepted}/${moves.length}`} sub="accepted" color={C} />
            <Readout label="Laws" value={`${laws.filter((l) => l.ok).length}/${laws.length}`} sub="passed" color={Y} />
            <Readout label="Bank" value={log.bank !== undefined ? cash(log.bank) : '—'} sub="sim cash" color={GRN} />
            <Readout label="Rake" value={log.rake !== undefined ? cash(log.rake) : '—'} sub="house" color={RED} />
            <Readout label="Players" value={String(log.players.length)} sub={`party ${log.party_no}`} color={VIO} />
          </div>

          {/* ticker */}
          <div className="relative z-10 mx-4 mt-3 overflow-hidden border-y py-1.5" style={{ borderColor: `${C}44`, background: `${C}0a` }}>
            <div className="cp-ticker mono flex w-max gap-10 text-xs whitespace-nowrap">
              {[0, 1].map((dup) => (
                <div key={dup} className="flex gap-10" aria-hidden={dup === 1}>
                  {latest.length === 0 ? (
                    <span style={{ color: C }}>NO SIGNAL</span>
                  ) : (
                    latest.map((e) => (
                      <span key={e.key}>
                        <span style={{ color: BADGE[e.kind][1] }}>▲ {BADGE[e.kind][0]}</span>{' '}
                        <span className="text-[#e8e6e3]">{e.actor ? `${e.actor} ` : ''}{e.text}</span>
                        {e.delta !== undefined && <span style={{ color: e.delta > 0 ? GRN : RED }}> {e.delta > 0 ? '+' : ''}{cash(e.delta)}</span>}
                      </span>
                    ))
                  )}
                </div>
              ))}
            </div>
          </div>

          <div className="relative z-10 grid min-h-0 flex-1 gap-4 p-4 md:grid-cols-[300px_minmax(0,1fr)]">
            {/* leaderboard */}
            <aside className="flex flex-col gap-2">
              <div className="text-[11px] tracking-[.35em] uppercase" style={{ color: C }}>
                // leaderboard
              </div>
              {[...log.players]
                .sort((a, b) => (a.rank ?? 99) - (b.rank ?? 99))
                .map((p) => (
                  <div key={p.idx} className="relative border p-3" style={{ ...cut(14), borderColor: p.rank === 1 ? `${Y}aa` : '#2a2a35', background: p.rank === 1 ? `${Y}0d` : '#0d0d14' }}>
                    <div className="flex items-baseline gap-3">
                      <span className="text-3xl leading-none font-bold" style={glow(p.rank === 1 ? Y : C)}>
                        {p.rank ? String(p.rank).padStart(2, '0') : '--'}
                      </span>
                      <div className="min-w-0 flex-1">
                        <div className="mono truncate text-sm font-semibold">{p.name}</div>
                        {p.model && <div className="mono truncate text-[11px] text-[#8a8f98]">{p.model}</div>}
                      </div>
                      {p.cash !== undefined && (
                        <span className="mono text-sm" style={glow(GRN)}>
                          {cash(p.cash)}
                        </span>
                      )}
                    </div>
                    {p.cash !== undefined && (
                      <div className="mt-2 h-1 bg-[#1b1b24]">
                        <div className="h-full" style={{ width: `${Math.max(2, (p.cash / maxCash) * 100)}%`, background: p.rank === 1 ? Y : C, boxShadow: `0 0 8px ${p.rank === 1 ? Y : C}` }} />
                      </div>
                    )}
                    <div className="mt-2 flex flex-wrap items-center justify-between gap-2 text-[11px]">
                      {p.payout !== undefined && <span style={{ color: GRN }}>PAYOUT +{cash(p.payout)}</span>}
                      {p.registration?.signature && (
                        <a
                          href={`https://explorer.solana.com/tx/${p.registration.signature}?cluster=${p.registration.network}`}
                          target="_blank"
                          rel="noreferrer"
                          className="mono underline underline-offset-2"
                          style={{ color: VIO }}
                        >
                          TX {p.registration.signature.slice(0, 8)}…
                        </a>
                      )}
                    </div>
                  </div>
                ))}
              {log.partial && (
                <p className="text-[11px] leading-snug text-[#8a8f98]">
                  LIVE: the arena publishes only the last 12 actions, without amounts or targets. Rows collect from the moment this
                  board opened.
                </p>
              )}
            </aside>

            {/* tape */}
            <section className="flex min-h-0 min-w-0 flex-col border" style={{ ...cut(18), borderColor: `${C}33`, background: '#08080dcc' }}>
              <div className="flex flex-wrap items-center gap-1 border-b px-3 py-2" style={{ borderColor: `${C}33` }}>
                {FILTERS.map(([f, label]) => (
                  <button
                    key={f}
                    type="button"
                    aria-pressed={filter === f}
                    onClick={() => setFilter(f)}
                    className="h-7 cursor-pointer px-3 text-[11px] font-bold tracking-[.2em] uppercase"
                    style={filter === f ? { ...cut(6), background: Y, color: '#06060a' } : { color: '#8a8f98' }}
                  >
                    {label}
                  </button>
                ))}
                {log.status === 'final' && (
                  <button
                    type="button"
                    onClick={() => setShown(shown === null || shown >= entries.length ? 0 : null)}
                    className="ml-auto h-7 cursor-pointer border px-3 text-[11px] font-bold tracking-[.2em] uppercase"
                    style={{ ...cut(6), borderColor: Y, ...glow(Y) }}
                  >
                    {shown !== null && shown < entries.length ? `■ stop ${shown}/${entries.length}` : '▶ replay'}
                  </button>
                )}
              </div>
              <div className="min-h-0 flex-1 overflow-auto">
                <table className="mono w-full border-collapse text-xs">
                  <thead className="sticky top-0 z-10 bg-[#08080d] text-left text-[10px] tracking-[.2em] uppercase" style={{ color: `${C}aa` }}>
                    <tr>
                      <th className="px-3 py-2 font-medium">Time</th>
                      <th className="hidden px-2 py-2 font-medium sm:table-cell">Rnd</th>
                      <th className="hidden px-2 py-2 font-medium sm:table-cell">Phase</th>
                      <th className="px-2 py-2 font-medium">Sig</th>
                      <th className="hidden px-2 py-2 font-medium sm:table-cell">Who</th>
                      <th className="px-2 py-2 font-medium">Event</th>
                      <th className="hidden px-2 py-2 font-medium lg:table-cell">Data</th>
                      <th className="px-3 py-2 text-right font-medium whitespace-nowrap" title="Change of this player's balance since its previous recorded state">
                        Δ bal
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((e) => {
                      const col = BADGE[e.kind][1]
                      const bad = e.kind === 'move' && e.ok === false
                      return (
                        <tr key={e.key} className={`border-t border-[#ffffff0a] hover:bg-[#ffffff08] ${isFresh(e.key) ? 'cp-in' : ''}`}>
                          <td className="px-3 py-1.5 whitespace-nowrap" style={{ color: `${C}cc` }}>
                            {time(e.ts)}
                          </td>
                          <td className="hidden px-2 py-1.5 text-[#8a8f98] sm:table-cell">{e.round ? `R${e.round}` : '—'}</td>
                          <td className="hidden px-2 py-1.5 text-[#8a8f98] uppercase sm:table-cell">{e.phase}</td>
                          <td className="px-2 py-1.5">
                            <span className="inline-block border px-1.5 py-px text-[10px] font-bold tracking-wider" style={{ borderColor: bad ? RED : col, color: bad ? RED : col, boxShadow: `0 0 6px ${bad ? RED : col}55` }}>
                              {bad ? 'FAIL' : BADGE[e.kind][0]}
                            </span>
                          </td>
                          <td className="hidden px-2 py-1.5 whitespace-nowrap sm:table-cell">{e.actor ?? ''}</td>
                          <td className="px-2 py-1.5" style={bad ? glow(RED) : e.kind === 'law' ? (e.ok ? glow(Y) : { color: '#8a8f98' }) : e.kind === 'settle' ? glow(GRN) : undefined}>
                            {e.actor && <span className="mr-1.5 text-[#8a8f98] sm:hidden">{e.actor}</span>}
                            {e.href ? (
                              <a href={e.href} target="_blank" rel="noreferrer" className="underline underline-offset-2" style={{ color: VIO }}>
                                {e.text}
                              </a>
                            ) : (
                              e.text
                            )}
                            {e.detail && <div className="text-[#6b7280] lg:hidden">{e.detail}</div>}
                          </td>
                          <td className="hidden px-2 py-1.5 text-[#6b7280] lg:table-cell">{e.detail}</td>
                          <td className="px-3 py-1.5 text-right whitespace-nowrap" style={e.delta === undefined ? { color: '#2f2f3a' } : glow(e.delta > 0 ? GRN : RED)}>
                            {e.delta === undefined ? '·' : `${e.delta > 0 ? '+' : ''}${cash(e.delta)}`}
                          </td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
                {rows.length === 0 && <p className="p-6 text-sm tracking-widest uppercase text-[#8a8f98]">No signal yet.</p>}
              </div>
            </section>
          </div>
        </>
      )}
    </main>
  )
}

function Readout({ label, value, sub, color }: { label: string; value: string; sub: string; color: string }) {
  return (
    <div className="relative border px-3 py-2" style={{ ...cut(10), borderColor: `${color}55`, background: `${color}08` }}>
      <div className="text-[10px] tracking-[.3em] text-[#8a8f98] uppercase">{label}</div>
      <div className="mono text-2xl leading-tight font-bold" style={glow(color)}>
        {value}
      </div>
      <div className="text-[10px] tracking-[.2em] uppercase" style={{ color: `${color}aa` }}>
        {sub}
      </div>
      <span className="absolute top-0 right-0 h-2 w-2" style={{ background: color }} />
    </div>
  )
}

function StatusChip({ log }: { log: GameLog }) {
  const live = log.status === 'live'
  return (
    <span className="flex items-center gap-2 text-xs">
      <span className="border px-2 py-0.5 font-bold tracking-[.25em] uppercase" style={{ ...cut(6), borderColor: live ? RED : Y, ...glow(live ? RED : Y) }}>
        {live ? <span className="cp-blink">● live</span> : 'final'}
      </span>
      <span className="mono text-[#8a8f98]">
        G{log.game_id} · P{log.party_no}
      </span>
    </span>
  )
}

function GamePicker({ game }: { game: string }) {
  const [value, setValue] = useState(game)
  return (
    <div className="flex items-center gap-1 text-xs">
      <label htmlFor="game" className="tracking-[.2em] text-[#8a8f98] uppercase">
        game
      </label>
      <input
        id="game"
        value={value}
        onChange={(e) => setValue(e.target.value.replace(/\D/g, ''))}
        className="mono h-7 w-14 border bg-transparent px-2"
        style={{ borderColor: `${C}66`, color: C }}
        inputMode="numeric"
      />
      <Link to="/log" search={{ game: value || '1' }} className="h-7 border px-2 leading-7 font-bold tracking-[.2em] uppercase" style={{ ...cut(6), borderColor: Y, color: Y }}>
        Open
      </Link>
    </div>
  )
}

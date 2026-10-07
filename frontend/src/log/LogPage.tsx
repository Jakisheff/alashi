import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { useEffect, useMemo, useRef, useState } from 'react'
import { cash, toLog, type EntryKind, type GameLog, type LogEntry, type StateResponse } from './model'

// Exchange-style game log: newest first, new rows flash, green/red for gains and rejections.
// Finished games come with the full recorded history; live games expose only the last 12 public actions,
// so live rows accumulate from the moment the page is opened.

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

const BADGE: Record<EntryKind, [string, string]> = {
  chain: ['TX', 'bg-[#9945ff]/20 text-[#c9a7ff]'],
  phase: ['PHASE', 'bg-[#27272e] text-[#a1a1aa]'],
  move: ['MOVE', 'bg-[#1f3a5f] text-[#8ec5ff]'],
  law: ['LAW', 'bg-[#4a3a12] text-[#ffd86b]'],
  settle: ['SETTLE', 'bg-[#123d2f] text-[#5fe3b0]'],
}

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

  return (
    <main className="flex min-h-dvh flex-col bg-[#101014] font-sans text-[#ececf0]">
      <style>{'@keyframes logIn{from{background:rgba(255,216,107,.22)}to{background:transparent}}.log-in{animation:logIn 1.6s ease-out}'}</style>
      <header className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-[#26262c] px-4 py-3">
        <Link to="/" className="font-semibold">
          alashi
        </Link>
        <h1 className="text-sm font-semibold">Game log</h1>
        <GamePicker game={game} />
        {log && <StatusChip log={log} />}
        <span className="text-xs text-[#a1a1aa]">HTTP game · simulated balances · only registrations are Solana transactions</span>
      </header>

      {q.isPending && <p className="p-6 text-sm text-[#a1a1aa]">Loading game {game}…</p>}
      {q.isError && <p className="p-6 text-sm text-[#ff9e9e]">Can&apos;t reach the arena: {(q.error as Error).message}</p>}
      {q.data && !log && (
        <p className="p-6 text-sm text-[#a1a1aa]">
          Game {game} not found{q.data.error ? ` (${q.data.error})` : ''}.
        </p>
      )}

      {log && (
        <div className="grid min-h-0 flex-1 md:grid-cols-[300px_minmax(0,1fr)]">
          <aside className="flex flex-col gap-4 border-b border-[#26262c] p-4 md:border-r md:border-b-0">
            <div className="grid grid-cols-2 gap-2 text-xs">
              <Stat label="Round" value={`${log.round} · ${log.phase}`} />
              <Stat label="Moves" value={`${accepted}/${moves.length} accepted`} />
              <Stat label="Laws" value={`${laws.filter((l) => l.ok).length} passed / ${laws.length}`} />
              <Stat label="Bank" value={log.bank !== undefined ? cash(log.bank) : '—'} />
            </div>
            <div>
              <div className="mb-2 text-[11px] font-semibold tracking-[.06em] text-[#a1a1aa]">PLAYERS</div>
              <ul className="flex flex-col gap-2">
                {[...log.players]
                  .sort((a, b) => (a.rank ?? 99) - (b.rank ?? 99))
                  .map((p) => (
                    <li key={p.idx} className="rounded-lg border border-[#26262c] bg-[#17171c] p-2.5 text-xs">
                      <div className="flex items-center gap-2">
                        {p.rank && <span className="font-mono text-[#ffd86b]">#{p.rank}</span>}
                        <span className="font-mono font-semibold">{p.name}</span>
                        {p.cash !== undefined && <span className="ml-auto font-mono">{cash(p.cash)}</span>}
                      </div>
                      {p.model && <div className="mt-0.5 font-mono text-[#a1a1aa]">{p.model}</div>}
                      {p.payout !== undefined && <div className="mt-0.5 text-[#5fe3b0]">payout +{cash(p.payout)}</div>}
                      {p.registration?.signature && (
                        <a
                          href={`https://explorer.solana.com/tx/${p.registration.signature}?cluster=${p.registration.network}`}
                          target="_blank"
                          rel="noreferrer"
                          className="mt-1 block truncate font-mono text-[#c9a7ff] underline underline-offset-2"
                        >
                          devnet tx {p.registration.signature.slice(0, 10)}…
                        </a>
                      )}
                    </li>
                  ))}
              </ul>
            </div>
            {log.partial && (
              <p className="text-xs leading-snug text-[#a1a1aa]">
                Live view: the arena publishes only the last 12 actions, without amounts or targets. Rows collect from the moment you
                opened this page.
              </p>
            )}
          </aside>

          <section className="flex min-h-0 min-w-0 flex-col">
            <div className="flex flex-wrap items-center gap-2 border-b border-[#26262c] px-4 py-2">
              {FILTERS.map(([f, label]) => (
                <button
                  key={f}
                  type="button"
                  aria-pressed={filter === f}
                  onClick={() => setFilter(f)}
                  className={`h-7 cursor-pointer rounded-md px-2.5 text-xs font-medium ${filter === f ? 'bg-[#ececf0] text-[#101014]' : 'text-[#a1a1aa] hover:text-[#ececf0]'}`}
                >
                  {label}
                </button>
              ))}
              {log.status === 'final' && (
                <button
                  type="button"
                  onClick={() => setShown(shown === null || shown >= entries.length ? 0 : null)}
                  className="ml-auto h-7 cursor-pointer rounded-md border border-[#33333b] px-2.5 text-xs font-medium hover:bg-[#1f1f25]"
                >
                  {shown !== null && shown < entries.length ? `■ Stop replay (${shown}/${entries.length})` : '▶ Replay game'}
                </button>
              )}
            </div>
            <div className="min-h-0 flex-1 overflow-auto">
              <table className="w-full border-collapse font-mono text-xs">
                <thead className="sticky top-0 bg-[#101014] text-left text-[11px] text-[#71717a]">
                  <tr className="border-b border-[#26262c]">
                    <th className="px-3 py-2 font-medium">Time</th>
                    <th className="hidden px-2 py-2 font-medium sm:table-cell">Rnd</th>
                    <th className="hidden px-2 py-2 font-medium sm:table-cell">Phase</th>
                    <th className="px-2 py-2 font-medium">Type</th>
                    <th className="hidden px-2 py-2 font-medium sm:table-cell">Who</th>
                    <th className="px-2 py-2 font-medium">What</th>
                    <th className="hidden px-2 py-2 font-medium lg:table-cell">Details</th>
                    <th className="px-3 py-2 text-right font-medium">Δ cash</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((e) => (
                    <tr key={e.key} className={`border-b border-[#1c1c22] hover:bg-[#17171c] ${isFresh(e.key) ? 'log-in' : ''}`}>
                      <td className="px-2 py-1.5 whitespace-nowrap text-[#a1a1aa] sm:px-3">{time(e.ts)}</td>
                      <td className="hidden px-2 py-1.5 text-[#a1a1aa] sm:table-cell">{e.round || '—'}</td>
                      <td className="hidden px-2 py-1.5 text-[#a1a1aa] sm:table-cell">{e.phase}</td>
                      <td className="px-2 py-1.5">
                        <span className={`rounded px-1.5 py-0.5 text-[10px] font-semibold ${BADGE[e.kind][1]}`}>{BADGE[e.kind][0]}</span>
                      </td>
                      <td className="hidden px-2 py-1.5 whitespace-nowrap sm:table-cell">{e.actor ?? ''}</td>
                      {/* Red only for rejected moves; a rejected law is an ordinary vote outcome */}
                      <td
                        className={`px-2 py-1.5 ${e.kind === 'move' && e.ok === false ? 'text-[#ff7a7a]' : e.kind === 'law' ? (e.ok ? 'text-[#ffd86b]' : 'text-[#a1a1aa]') : ''}`}
                      >
                        {e.actor && <span className="mr-1.5 text-[#a1a1aa] sm:hidden">{e.actor}</span>}
                        {e.href ? (
                          <a href={e.href} target="_blank" rel="noreferrer" className="underline underline-offset-2">
                            {e.text}
                          </a>
                        ) : (
                          e.text
                        )}
                        {e.detail && <div className="text-[#71717a] lg:hidden">{e.detail}</div>}
                      </td>
                      <td className="hidden px-2 py-1.5 text-[#71717a] lg:table-cell">{e.detail}</td>
                      <td
                        className={`px-3 py-1.5 text-right whitespace-nowrap ${e.delta === undefined ? 'text-[#3f3f46]' : e.delta > 0 ? 'text-[#4ade80]' : 'text-[#f87171]'}`}
                      >
                        {e.delta === undefined ? '·' : `${e.delta > 0 ? '+' : ''}${cash(e.delta)}`}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {rows.length === 0 && <p className="p-6 text-sm text-[#a1a1aa]">No rows yet.</p>}
            </div>
          </section>
        </div>
      )}
    </main>
  )
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-[#26262c] bg-[#17171c] px-2.5 py-2">
      <div className="text-[10px] tracking-wide text-[#71717a] uppercase">{label}</div>
      <div className="mt-0.5 font-mono">{value}</div>
    </div>
  )
}

function StatusChip({ log }: { log: GameLog }) {
  return (
    <span className="flex items-center gap-2 text-xs">
      <span
        className={`rounded-full px-2 py-0.5 font-semibold tracking-wide uppercase ${log.status === 'live' ? 'bg-[#123d2f] text-[#5fe3b0]' : 'bg-[#27272e] text-[#ececf0]'}`}
      >
        {log.status === 'live' ? '● live' : 'final'}
      </span>
      <span className="font-mono text-[#a1a1aa]">
        game {log.game_id} · party {log.party_no}
      </span>
    </span>
  )
}

function GamePicker({ game }: { game: string }) {
  const [value, setValue] = useState(game)
  return (
    <div className="flex items-center gap-1 text-xs">
      <label htmlFor="game" className="text-[#a1a1aa]">
        game
      </label>
      <input
        id="game"
        value={value}
        onChange={(e) => setValue(e.target.value.replace(/\D/g, ''))}
        className="h-7 w-14 rounded-md border border-[#33333b] bg-[#17171c] px-2 font-mono"
        inputMode="numeric"
      />
      <Link to="/log" search={{ game: value || '1' }} className="h-7 rounded-md border border-[#33333b] px-2 leading-7 hover:bg-[#1f1f25]">
        Open
      </Link>
    </div>
  )
}

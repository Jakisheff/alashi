import { BrandMark } from '../brand/BrandMark'
import { useQuery } from '@tanstack/react-query'
import { Link, useNavigate } from '@tanstack/react-router'
import { useEffect, useMemo, useState } from 'react'
import { StatusPage } from '../status/StatusPage'
import { gameId } from '../feed'
import '../live/live.css'
import './log.css'
import { cash, explorerTx, toLog, type EntryKind, type GameLog, type LogEntry, type StateResponse } from './model'

type Filter = 'all' | 'moves' | 'decisions' | 'phases' | 'chain'
const FILTERS: [Filter, string][] = [
  ['all', 'All'],
  ['moves', 'Moves'],
  ['decisions', 'Decisions'],
  ['phases', 'Phases'],
  ['chain', 'Registrations'],
]
const pass = (f: Filter, e: LogEntry) =>
  f === 'all' ||
  (f === 'moves' && e.kind === 'move') ||
  (f === 'decisions' && (e.kind === 'law' || e.kind === 'settle' || /^voted|veto/.test(e.text))) ||
  (f === 'phases' && (e.kind === 'phase' || e.kind === 'settle')) ||
  (f === 'chain' && e.kind === 'chain')

const BADGE: Record<EntryKind, string> = { chain: 'Registration', phase: 'Phase', move: 'Move', law: 'Law', settle: 'Settlement' }
const time = (ts: number | null) =>
  ts === null ? '—' : new Date(ts * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })
const REPLAY_MS = 450

export function LogPage({ game }: { game?: string }) {
  const q = useQuery<StateResponse>({
    queryKey: ['admin-game-log', game], enabled: Boolean(game), retry: false, gcTime: 0,
    // Stops once the game is finished, or unknown to the arena ({ok:false, error:'unknown_game'})
    refetchInterval: (query) => (query.state.error || query.state.data?.finished || query.state.data?.ok === false ? false : 2000),
    queryFn: async ({ signal }) => {
      const response = await fetch(`/admin/games/${game}/log`, { credentials: 'same-origin', cache: 'no-store', redirect: 'error', signal: AbortSignal.any([signal, AbortSignal.timeout(10_000)]) })
      if (response.status === 401 || response.status === 403) throw new Error('admin_denied')
      if (response.status === 404) return { ok: false, error: 'unknown_game' } as StateResponse
      if (!response.ok || !response.headers.get('content-type')?.includes('application/json')) throw new Error('log_unavailable')
      return await response.json() as StateResponse
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
  const [filter, setFilter] = useState<Filter>('all')
  const visible = shown === null ? entries : entries.slice(0, shown)
  const rows = useMemo(() => visible.filter((e) => pass(filter, e)).reverse(), [visible, filter])
  const moves = entries.filter((e) => e.kind === 'move')
  const accepted = moves.filter((e) => e.ok).length
  const laws = entries.filter((e) => e.kind === 'law')
  if (q.isError) return <StatusPage code={q.error.message === 'admin_denied' ? '403' : '503'} title={q.error.message === 'admin_denied' ? 'Administrator access required' : 'Game log unavailable'} message="The game log has been closed. Your administrator session or the log service is unavailable." onRetry={() => void q.refetch()} />
  return <main className="live-page log-page" lang="en">
    <header className="live-header"><Link to="/" className="live-brand"><BrandMark /></Link><span className="live-header-label">Game log</span><span className="live-preview-label">Admin</span></header>
    <section className="log-content">
      <div className="log-title"><div><span className="live-eyebrow">Game records</span><h1>{game ? `Game ${game}` : 'Open a game log'}</h1></div><GamePicker game={game} /></div>
      {!game && <p className="log-note">Enter a game ID to view its recorded actions.</p>}
      {game && q.isPending && <p role="status" className="log-note">Loading game {game}…</p>}
      {q.data && !log && <div className="log-empty"><h2>Game not found</h2><p>Check this game’s ID and try again.</p></div>}
      {log && <>
        <div className="log-meta"><span>{log.status === 'live' ? 'Live · received records' : 'Finished · recorded history'} · Party {log.party_no}</span><span>HTTP arena · simulated balances. Only registration receipts are on-chain.</span></div>
        <dl className="log-readouts"><div><dt>Round</dt><dd>{log.round}<small>{log.phase}</small></dd></div><div><dt>Accepted moves</dt><dd>{accepted} / {moves.length}</dd></div><div><dt>Passed laws</dt><dd>{laws.filter((e) => e.ok).length} / {laws.length}</dd></div><div><dt>Bank</dt><dd>{log.bank !== undefined ? cash(log.bank) : '—'}</dd></div></dl>
        <div className="log-layout">
          <aside className="log-players"><h2>Players</h2><ol>{[...log.players].sort((a, b) => (a.rank ?? 99) - (b.rank ?? 99)).map((player) => <li key={player.idx}><span className="log-rank">{player.rank ? String(player.rank).padStart(2, '0') : '—'}</span><div><strong>{player.name}</strong>{player.model && <small>{player.model}</small>}{player.registration?.signature && <a href={explorerTx(player.registration)} target="_blank" rel="noreferrer">Registration receipt ↗</a>}</div><div className="log-cash">{player.cash !== undefined ? cash(player.cash) : '—'}{player.payout !== undefined && <small>Payout {cash(player.payout)}</small>}</div></li>)}</ol>{log.partial && <p className="log-note">Partial history. Live rows are collected from the last 12 public actions; earlier gaps and missing amounts are not reconstructed.</p>}</aside>
          <section className="log-events" aria-label="Recorded events">
            <div className="log-toolbar"><div role="group" aria-label="Event type">{FILTERS.map(([key, label]) => <button key={key} type="button" aria-pressed={filter === key} onClick={() => setFilter(key)}>{label}</button>)}</div>{log.status === 'final' && <button type="button" className="log-replay" onClick={() => setShown(shown === null || shown >= entries.length ? 0 : null)}>{shown !== null && shown < entries.length ? `Stop replay ${shown}/${entries.length}` : 'Replay records'}</button>}</div>
            <div className="log-table-scroll" tabIndex={0} aria-label="Game event table"><table><thead><tr><th>Time</th><th>Round</th><th>Type</th><th>Player</th><th>Event</th><th>Balance change</th></tr></thead><tbody>{rows.map((entry) => <tr key={entry.key} data-rejected={entry.kind === 'move' && entry.ok === false}><td>{time(entry.ts)}</td><td>{entry.round || '—'}</td><td>{BADGE[entry.kind]}{entry.kind === 'move' && entry.ok === false && <small>Rejected</small>}</td><td>{entry.actor ?? '—'}</td><td>{entry.href ? <a href={entry.href} target="_blank" rel="noreferrer">{entry.text} ↗</a> : entry.text}{entry.detail && <small>{entry.detail}</small>}</td><td>{entry.delta === undefined ? '—' : `${entry.delta > 0 ? '+' : ''}${cash(entry.delta)}`}</td></tr>)}</tbody></table></div>
            {!rows.length && <p className="log-note">No records for this filter.</p>}
          </section>
        </div>
      </>}
    </section>
  </main>
}
function GamePicker({ game }: { game?: string }) {
  const [value, setValue] = useState(game ?? ''), [error, setError] = useState('')
  const navigate = useNavigate()
  return <form className="log-picker" onSubmit={(event) => { event.preventDefault(); const id = gameId(value.trim()); if (!id) { setError('Enter a positive numeric game ID.'); return } setError(''); void navigate({ to: '/log', search: { game: id } }) }}><label htmlFor="log-game">Game ID</label><div><input id="log-game" value={value} inputMode="numeric" onChange={(event) => setValue(event.target.value)} autoComplete="off" /><button type="submit">Open</button></div>{error && <p role="alert">{error}</p>}</form>
}

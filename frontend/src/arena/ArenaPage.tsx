import { Link } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { activeArenaGames, arenaEvents, ArenaApiError, mergeArenaEvents, type ArenaEvent, type ArenaGame } from './client'

const short = (id: string) => id.length > 12 ? `${id.slice(0, 6)}…${id.slice(-4)}` : id
const clock = (value: string) => new Date(value).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })
const eventText = (event: ArenaEvent) => {
  if (event.kind === 'agent_message') return event.text ?? ''
  if (event.kind === 'game_action') return `Agent ${event.actor} · ${event.action?.replaceAll('_', ' ') ?? 'action'} · accepted`
  if (event.kind === 'phase_changed') return `Round ${event.round ?? '—'} · ${event.phase ?? 'phase change'}`
  return 'Game finished.'
}
const failure = (error: unknown) => error instanceof ArenaApiError && error.code === 'network'
  ? 'Connection interrupted. Trying again…'
  : 'The public arena is unavailable. Trying again…'

export function ArenaPage({ requestedGame }: { requestedGame?: string }) {
  const [games, setGames] = useState<ArenaGame[]>([])
  const [gamesReady, setGamesReady] = useState(false)
  const [gamesError, setGamesError] = useState('')
  const [retry, setRetry] = useState(0)
  const selected = requestedGame ? Number(requestedGame) : games[0]?.gameId ?? null
  const game = games.find((item) => item.gameId === selected)
  const [events, setEvents] = useState<ArenaEvent[]>([])
  const [eventsReady, setEventsReady] = useState(false)
  const [eventsError, setEventsError] = useState('')
  const [truncated, setTruncated] = useState(false)
  const [finished, setFinished] = useState(false)
  const cursor = useRef(0)

  useEffect(() => {
    const controller = new AbortController()
    let timer = 0, stopped = false
    const poll = async () => {
      try {
        const next = await activeArenaGames(controller.signal)
        if (stopped) return
        setGames(next); setGamesReady(true); setGamesError('')
      } catch (error) {
        if (!stopped) { setGamesReady(true); setGamesError(failure(error)) }
      }
      if (!stopped) timer = window.setTimeout(poll, 3_000)
    }
    void poll()
    return () => { stopped = true; controller.abort(); window.clearTimeout(timer) }
  }, [retry])

  useEffect(() => {
    const controller = new AbortController()
    let timer = 0, stopped = false
    cursor.current = 0
    setEvents([]); setEventsReady(false); setEventsError(''); setTruncated(false); setFinished(false)
    if (selected === null) return () => controller.abort()
    const poll = async () => {
      try {
        const page = await arenaEvents(selected, cursor.current, controller.signal)
        if (stopped) return
        cursor.current = page.cursor
        setEvents((previous) => mergeArenaEvents(previous, page.events))
        setTruncated((previous) => previous || page.truncated)
        if (page.events.some((event) => event.kind === 'final_result')) setFinished(true)
        setEventsReady(true); setEventsError('')
      } catch (error) {
        if (!stopped) { setEventsReady(true); setEventsError(failure(error)) }
      }
      if (!stopped) timer = window.setTimeout(poll, 2_000)
    }
    void poll()
    return () => { stopped = true; controller.abort(); window.clearTimeout(timer) }
  }, [selected, retry])

  useEffect(() => {
    const title = document.title
    document.title = selected ? `Arena ${selected} · alashi` : 'Watch arena · alashi'
    return () => { document.title = title }
  }, [selected])

  const copy = useCallback(async () => {
    if (!selected) return
    try { await navigator.clipboard.writeText(`${window.location.origin}/arena?game=${selected}`) }
    catch { /* The address bar remains the canonical shareable link. */ }
  }, [selected])
  const visible = useMemo(() => [...events].reverse(), [events])
  const state = finished ? 'Finished' : game ? 'Live' : requestedGame ? 'Public history' : 'Waiting for an arena'

  return <main className="min-h-dvh bg-[#081112] px-4 py-6 text-[#e7f5ee] sm:px-8">
    <div className="mx-auto max-w-5xl space-y-5">
      <header className="flex flex-wrap items-start justify-between gap-4 border-b border-[#2b6050] pb-5">
        <div>
          <p className="text-xs font-semibold tracking-[.24em] text-[#85dcb8] uppercase">Devnet-registered agents · public arena</p>
          <h1 className="mt-1 text-3xl font-semibold tracking-tight">Watch the arena</h1>
          <p className="mt-2 max-w-2xl text-sm text-[#b4c9bf]">Real public events from the live server game. No wallet or registration is needed. Agent identities are devnet-registered; game balances are off-chain and do not settle on-chain.</p>
        </div>
        <div className="flex gap-2"><button className="rounded border border-[#4da482] px-3 py-2 text-sm hover:bg-[#163c30]" onClick={() => setRetry((value) => value + 1)}>Reconnect</button>{selected && <button className="rounded border border-[#4da482] px-3 py-2 text-sm hover:bg-[#163c30]" onClick={() => void copy()}>Copy arena link</button>}</div>
      </header>
      {gamesError && <p className="rounded border border-[#b98049] bg-[#382619] p-3 text-sm" role="status">{gamesError}</p>}
      {eventsError && <p className="rounded border border-[#b98049] bg-[#382619] p-3 text-sm" role="status">{eventsError}</p>}
      {!gamesReady ? <p className="text-sm text-[#b4c9bf]" role="status">Finding a public arena…</p> : selected === null ? <section className="rounded border border-[#2b6050] bg-[#0e211d] p-6"><h2 className="text-xl font-medium">No active arena right now.</h2><p className="mt-2 text-sm text-[#b4c9bf]">This page checks again automatically. A shared arena link will keep its public history after the game finishes.</p></section> : <>
        <section className="grid gap-3 rounded border border-[#2b6050] bg-[#0e211d] p-5 sm:grid-cols-4">
          <Readout label="Status" value={state} />
          <Readout label="Game" value={`#${selected}`} detail={game ? `Party ${game.partyNo}${game.label ? ` · ${game.label}` : ''}` : 'Shared public link'} />
          <Readout label="Round" value={game ? String(game.round) : '—'} detail={game ? game.phase : finished ? 'Recorded result' : 'Awaiting public context'} />
          <Readout label="Players" value={game ? String(game.factions) : '—'} detail={game?.names.join(' · ') || 'Public names only'} />
        </section>
        {truncated && <p className="rounded border border-[#b98049] bg-[#382619] p-3 text-sm" role="status">Some earlier public history is no longer available.</p>}
        <section className="rounded border border-[#2b6050] bg-[#0e211d]"><div className="flex items-center justify-between border-b border-[#2b6050] px-5 py-3"><h2 className="font-medium">Public event log</h2><span className="text-xs text-[#85dcb8]">{eventsError ? 'Reconnecting' : eventsReady ? 'Connected' : 'Connecting'}</span></div><ol className="divide-y divide-[#1b4236]">
          {visible.map((event) => <li key={event.id} className="grid gap-1 px-5 py-4 sm:grid-cols-[6rem_1fr]"><time className="font-mono text-xs text-[#85dcb8]" dateTime={event.createdAt}>{clock(event.createdAt)}</time><div><p className="text-sm">{eventText(event)}</p>{event.kind === 'agent_message' && event.author && <p className="mt-1 text-xs text-[#9fb9ad]">Agent {short(event.author)} · <a className="underline underline-offset-2" href={`/stream?agent=${encodeURIComponent(event.author)}`}>Watch agent</a></p>}</div></li>)}
          {eventsReady && visible.length === 0 && <li className="px-5 py-6 text-sm text-[#b4c9bf]">{finished ? 'This finished game has no retained public events.' : game ? 'No public events yet. Agents may act or remain silent.' : 'No retained public history is available for this game link.'}</li>}
          {!eventsReady && <li className="px-5 py-6 text-sm text-[#b4c9bf]">Connecting to the public event log…</li>}
        </ol></section>
      </>}
      <footer className="flex flex-wrap gap-x-4 gap-y-2 text-sm text-[#b4c9bf]"><Link to="/" className="underline underline-offset-4">Home</Link><Link to="/stream" className="underline underline-offset-4">Watch an agent stream</Link><span>For Amir → Max: share the copied arena link while the devnet game is running.</span></footer>
    </div>
  </main>
}

function Readout({ label, value, detail }: { label: string; value: string; detail?: string }) {
  return <div><p className="text-xs tracking-[.18em] text-[#85dcb8] uppercase">{label}</p><p className="mt-1 text-lg font-medium">{value}</p>{detail && <p className="mt-1 text-xs text-[#9fb9ad]">{detail}</p>}</div>
}

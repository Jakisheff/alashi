import { Link, useNavigate } from '@tanstack/react-router'
import { Component, lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { MarketTrade, type TradePreview } from '../live/market/MarketTrade'
import { ScenarioAction, type ScenarioPreview } from '../live/actions'
import { LivingGenie, type ReactionPreview } from '../live/reactions'
import { validRecord } from '../live/api/client'
import type { ChainFaction } from './client'
import { useScene } from '../store'
import { consumeOwnerFragment, readOwnerLocator, redeemOwnerFragment, rememberOwnerLocator, type OwnerLocator } from '../owner/pairing'
import { validBase58, formatAmount, formatCash, type ChainEvent } from './client'
import { useChainSnapshot } from './useChainSnapshot'
import { defaultWinner, eventLabel, liveAdditions, newLiveCursor, playerEvents, playerStandings, visualAction } from './playback'
import '../live/live.css'
import './devnet.css'
const Scene = lazy(() => import('../genie/Scene'))
const ChainOwnerPanel = lazy(() => import('./ChainOwnerPanel').then((m) => ({ default: m.ChainOwnerPanel })))
const short = (id: string) => `${id.slice(0, 6)}…${id.slice(-4)}`
const ignoreTime = () => {}
function AgentSetupGuide() {
  return <details id="agent-setup" className="devnet-setup"><summary>How to connect your agent</summary><ol>
    <li>Your operator registers the agent and runs the opt-in chain runner locally. Its registered wallet must already have joined a new active devnet game and be bound to that game's player.</li>
    <li>Ask the operator for the game's public viewing link and your registered agent ID. The link has the form <code>/devnet?game=&lt;Game address&gt;</code>. Open it, then choose your player.</li>
    <li>In “Own this player?”, enter that agent ID and verify its registered wallet. Once the server confirms the active binding, you can send up to three private instructions for that agent in this game.</li>
  </ol><p><a href="/devnet-runner.html">Operator instructions for joining with a local no-LLM runner ↗</a></p><p>The selected instruction guides the deterministic agent; optional wording stays private. This page views an existing game. HTTP arena enrollment does not join a Solana game, and a completed game can only be replayed.</p></details>
}
class HeroBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  render() { return this.state.failed ? <p className="live-hero-fallback">3D is unavailable. Confirmed actions remain below.</p> : this.props.children }
}
function OwnerEntry({ gamePda, faction, autoRecord }: { gamePda: string; faction: ChainFaction; autoRecord: string }) {
  const [input, setInput] = useState(''), [record, setRecord] = useState(autoRecord), [error, setError] = useState('')
  useEffect(() => { if (autoRecord) setRecord(autoRecord) }, [autoRecord])
  return record ? <><Suspense fallback={<p>Loading private owner access…</p>}><ChainOwnerPanel key={record} record={record} gamePda={gamePda} faction={faction} /></Suspense><button className="devnet-text-button" onClick={() => { setRecord(''); setInput('') }}>Change registered agent</button></> : <section className="devnet-owner-entry"><h2>Own this player?</h2><p>Enter its registered agent ID to check private owner access. The registered wallet must match this on-chain player.</p><form onSubmit={(e) => { e.preventDefault(); if (!validRecord(input.trim())) { setError('Enter a 64-character registered agent ID.'); return } setError(''); setRecord(input.trim()) }}><label htmlFor="chain-agent">Registered agent ID</label><input id="chain-agent" autoComplete="off" spellCheck={false} value={input} maxLength={64} onChange={(e) => setInput(e.target.value)} /><button>Check private access</button></form>{error && <p role="alert">{error}</p>}</section>
}
function GameViewer({ pda, initialPlayer, ownerRecord }: { pda: string; initialPlayer: string; ownerRecord: string }) {
  const { snapshot, error, waiting, retry } = useChainSnapshot(pda)
  const [selected, setSelected] = useState(initialPlayer)
  useEffect(() => { if (initialPlayer) setSelected(initialPlayer) }, [initialPlayer])
  const faction = snapshot?.factions.find((f) => f.pda === selected) ?? (snapshot ? defaultWinner(snapshot) : undefined)
  const standings = snapshot ? playerStandings(snapshot) : null
  const [current, setCurrent] = useState<ChainEvent | null>(null)
  const [mode, setMode] = useState<'live' | 'replay'>('live')
  const [playing, setPlaying] = useState(false)
  const [position, setPosition] = useState(-1)
  const [take, setTake] = useState(0)
  const [replayRevision, setReplayRevision] = useState(0)
  const [replayEvents, setReplayEvents] = useState<ChainEvent[]>([])
  const [now, setNow] = useState(Date.now())
  const initialWinnerReplay = useRef(false)
  const queue = useRef<ChainEvent[]>([]), liveCursor = useRef(newLiveCursor()), liveEvent = useRef<ChainEvent | null>(null), completedTake = useRef(-1)
  const observed = useMemo(() => faction && snapshot ? playerEvents(snapshot.events, faction) : [], [snapshot, faction])
  const action = current && faction ? visualAction(current, faction) : null
  const reset = useCallback(() => { queue.current = []; liveEvent.current = null; setCurrent(null); setPosition(-1); setReplayEvents([]); setPlaying(false); setMode('live'); useScene.getState().say('') }, [])
  const finish = useCallback(() => {
    if (completedTake.current === take || (mode === 'replay' && !playing)) return
    completedTake.current = take
    if (mode === 'replay') { setPosition((i) => i + 1); return }
    const next = queue.current.shift() ?? null
    liveEvent.current = next; setCurrent(next); setTake((n) => n + 1)
  }, [mode, take, playing])
  useEffect(() => { reset(); liveCursor.current = newLiveCursor() }, [faction?.pda, reset])
  useEffect(() => {
    if (!snapshot || !faction) return
    const fresh = liveAdditions(liveCursor.current, observed, snapshot.complete)
    if (mode !== 'live' || error) return
    queue.current.push(...fresh.filter((e) => visualAction(e, faction)))
    if (!liveEvent.current && queue.current.length) { const next = queue.current.shift()!; liveEvent.current = next; setCurrent(next); setTake((n) => n + 1) }
  }, [snapshot, faction, observed, mode, error])
  const activeEvents = mode === 'replay' ? replayEvents : observed
  const replayEvent = mode === 'replay' && position >= 0 ? replayEvents[position] : undefined
  useEffect(() => {
    if (mode !== 'replay') return
    if (!replayEvent) { setCurrent(null); setPlaying(false); return }
    setCurrent(replayEvent); setTake((n) => n + 1)
    // Polling a fresh snapshot must not restart an in-progress replay event.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, replayEvent?.id, replayRevision])
  useEffect(() => {
    if (mode !== 'replay' || !playing || !current || action) return
    const timer = window.setTimeout(finish, 1800)
    return () => window.clearTimeout(timer)
  }, [mode, playing, current, action, finish])
  useEffect(() => {
    // Recover the transcript even when WebGL fails or stops delivering frames.
    if (!current || (mode === 'replay' && !playing) || !action) return
    const timer = window.setTimeout(finish, 10_000)
    return () => window.clearTimeout(timer)
  }, [current, action, mode, playing, finish])
  useEffect(() => { const timer = window.setInterval(() => setNow(Date.now()), 1000); return () => { window.clearInterval(timer); useScene.getState().say('') } }, [])
  const replay = (index = 0, seek = false) => { setReplayEvents(seek && mode === 'replay' ? replayEvents : observed.slice()); queue.current = []; liveEvent.current = null; setCurrent(null); setMode('replay'); setPosition(index); setPlaying(true); setReplayRevision((n) => n + 1) }
  useEffect(() => {
    if (initialWinnerReplay.current || selected || !snapshot?.settled || !snapshot.complete || !faction || !observed.length || defaultWinner(snapshot)?.pda !== faction.pda) return
    initialWinnerReplay.current = true
    replay()
    // An initial complete result starts once; later polls and manual selection never restart it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot, faction, selected, observed])
  const running = mode === 'live' || playing
  const preview = { take, playing: running, speed: 1, seek: null, entry: 'bottom' as const }
  const trade: TradePreview | null = action === 'buy' || action === 'sell' ? { ...preview, action } : null
  const scenario: ScenarioPreview | null = action === 'mule' || action === 'bribe' || action === 'vote' ? { ...preview, action } : null
  const reaction: ReactionPreview | null = action === 'victory' ? { ...preview, kind: 'victory' } : null
  const connection = error ? (snapshot ? 'Reconnecting' : 'Unavailable') : !snapshot ? 'Connecting' : mode === 'replay' ? 'Replay' : snapshot.settled ? 'Finished' : 'On-chain'
  const remaining = snapshot ? Math.max(0, Math.ceil(snapshot.endsAt - now / 1000)) : 0
  const latestFeed = mode === 'replay' ? replayEvents.slice(0, Math.max(0, position + 1)).slice(-4) : observed.slice(-4)
  return <div className="live-layout devnet-layout">
    <section className="live-frame devnet-frame" aria-label="Confirmed devnet gameplay">
      <div className="live-host"><span className="live-avatar">{faction?.name.slice(0, 1) ?? 'α'}</span><div><strong>{faction?.name ?? 'Degenie'}</strong><span>Solana devnet · public spectator</span></div><span className="live-air">{connection}</span></div>
      <div className="live-context"><span>{snapshot ? `Game ${snapshot.gameId} · ${mode === 'replay' ? `action ${Math.min(position + 1, activeEvents.length)}` : `round ${snapshot.round}`}` : 'Reading confirmed chain history…'}</span><strong>{mode === 'replay' ? 'Action replay' : snapshot?.phase ?? 'Connecting'}</strong><div className="live-deadline"><b>{!snapshot ? '—' : snapshot.settled || mode === 'replay' ? '✓' : `${Math.floor(remaining / 60)}:${String(remaining % 60).padStart(2, '0')}`}</b><small>{!snapshot ? 'waiting for data' : mode === 'replay' ? 'compressed pauses' : snapshot?.settled ? 'settled on-chain' : 'phase ends in'}</small></div></div>
      <div className="live-hero" data-action={action ?? 'idle'}><div className="live-scene"><HeroBoundary><Suspense fallback={<p className="live-hero-fallback">Loading Degenie…</p>}><Scene frozen={null} background="#102c25" interactive={false}>{trade ? <MarketTrade preview={trade} onTime={ignoreTime} onFinished={finish} /> : scenario ? <ScenarioAction preview={scenario} onTime={ignoreTime} onFinished={finish} /> : <LivingGenie reaction={reaction} onFinished={finish} />}</Scene></Suspense></HeroBoundary></div></div>
      {current && faction && <div className="devnet-caption" role="status">{eventLabel(current, faction)}</div>}
      <div className="live-transcript"><div className="live-transcript-head"><span>Confirmed actions</span><small>{mode === 'replay' ? 'Replay · not a live action' : snapshot ? 'Real chain events' : 'Awaiting verified history'}</small></div><div className="live-feed" tabIndex={0}>{error && <p className="live-network" role="status">{error}</p>}{waiting && <p>Loading public game history…</p>}{snapshot && !faction && <p>Choose a player to view their confirmed actions.</p>}{!waiting && !error && faction && !observed.length && <p>No confirmed actions yet.</p>}{latestFeed.map((e) => <div className="live-event live-fact" key={e.id}><p>{faction && eventLabel(e, faction)}</p><a href={`https://explorer.solana.com/tx/${e.signature}?cluster=devnet`} target="_blank" rel="noreferrer">Confirmed transaction ↗</a></div>)}</div></div>
      <div className="live-frame-footer"><span>{error ? 'Last received snapshot' : 'Read-only · no wallet required'}</span><span>{snapshot ? short(snapshot.pda) : short(pda)}</span></div>
    </section>
    <aside className="live-sidebar devnet-sidebar"><span className="live-eyebrow">Real game. Familiar character.</span><h1>Watch the moves.<br />Follow your player.</h1><p className="live-description">Confirmed Solana actions drive the animations. Finished games retain their public action history.</p>
      {snapshot && <><section className="devnet-leaderboard" aria-label="Players"><div className="devnet-leaderboard-head"><strong>Players</strong><span>{standings?.final ? standings.rows.every((row) => row.rank === null) ? 'Settled · payout ranks unavailable' : standings.rows.some((row) => row.rank === null) ? 'Confirmed payout ranks · others unranked' : 'Final payout order · confirmed' : 'Provisional order · latest cash only'}</span></div><ol className="devnet-leaderboard-list">{standings?.rows.map(({ faction: f, rank }) => <li key={f.pda}><button type="button" className="devnet-leaderboard-row" aria-pressed={f.pda === faction?.pda} onClick={() => { reset(); setSelected(f.pda) }}><span className="devnet-leaderboard-rank">{rank === null ? '—' : String(rank + 1).padStart(2, '0')}</span><span className="devnet-leaderboard-name"><strong>{f.name}</strong><small>{short(f.pda)} · {f.goods} goods · {f.influence} influence</small></span><span className="devnet-leaderboard-cash">{formatCash(f.cash)}<small>{rank === 0 ? 'Verified winner' : rank === null && standings.final ? 'No payout rank' : 'Latest cash'}</small></span></button></li>)}</ol></section>
        <div className="devnet-resources"><span>Latest confirmed balances{mode === 'replay' ? ' · not replay balances' : ''}</span><strong>{faction ? formatCash(faction.cash) : '—'}</strong>{faction && <p className="devnet-hard">Hard currency: {formatAmount(faction.hard)}</p>}<p>{faction?.goods ?? '—'} goods · {faction?.influence ?? '—'} influence · vote {faction?.vote ?? '—'}{faction && !faction.alive ? ' · exited' : ''}</p></div>
        <div className="live-playback"><button disabled={!observed.length} onClick={() => replay()}>{snapshot.complete ? 'Replay confirmed actions' : 'Play available actions'}</button>{mode === 'replay' && <button disabled={!current} onClick={() => setPlaying((s) => !s)}>{playing ? 'Pause' : 'Resume'}</button>}</div>
        {mode === 'replay' && <><label className="devnet-timeline">Action {Math.min(position + 1, activeEvents.length)} / {activeEvents.length}<input aria-label="Replay action" type="range" min={0} max={Math.max(0, activeEvents.length - 1)} value={Math.max(0, Math.min(position, activeEvents.length - 1))} onChange={(e) => replay(Number(e.target.value), true)} /></label><button className="devnet-text-button" onClick={reset}>Return to latest state</button></>}
        <p className="devnet-note">{snapshot.complete ? 'Confirmed action history loaded.' : 'Partial history · more confirmed events may arrive.'} Playback uses shortened pauses; it does not reconstruct past balances or invent agent thoughts.</p>
        <details className="live-about"><summary>Chain receipt and all players</summary><p>Snapshot received {new Date(snapshot.fetchedAt).toLocaleTimeString()}. {snapshot.events.length} confirmed events. State slot {snapshot.snapshotSlot}; journal through {snapshot.journalThroughSlot}.</p>{snapshot.factions.map((f) => <p key={f.pda}><strong>{f.name}</strong> · {formatCash(f.cash)} · {f.goods} goods · {f.influence} influence</p>)}<a href={`https://explorer.solana.com/address/${pda}?cluster=devnet`} target="_blank" rel="noreferrer">View Game account ↗</a></details></>}
      {snapshot && faction && faction.alive && !snapshot.settled && !['Finished', 'Aborted'].includes(snapshot.phase) && <OwnerEntry key={faction.pda} gamePda={pda} faction={faction} autoRecord={faction.pda === initialPlayer ? ownerRecord : ''} />}
      {error && <div className="devnet-error"><p role="alert">{error}</p><button onClick={retry}>Retry connection</button><p>No demo actions are substituted for missing chain data.</p></div>}
      <Link className="live-back" to="/">← Back to alashi</Link>
    </aside>
  </div>
}
export function DevnetPage({ requestedGame, requestedPlayer = '' }: { requestedGame: string; requestedPlayer?: string }) {
  const navigate = useNavigate()
  // React runs this initializer before effects or data fetches. The one-use
  // fragment never reaches analytics, referrers, or the chain projection URL.
  const [handoff] = useState(() => consumeOwnerFragment())
  const [linked, setLinked] = useState<OwnerLocator | null>(null)
  const [linkError, setLinkError] = useState('')
  const [remembered] = useState(() => readOwnerLocator())
  const [input, setInput] = useState(requestedGame), [inputError, setInputError] = useState('')
  const valid = validBase58(requestedGame)
  useEffect(() => {
    if (!handoff) return
    let stopped = false
    void redeemOwnerFragment(handoff.record, handoff.code, requestedGame).then((locator) => {
      if (stopped) return
      rememberOwnerLocator(locator)
      setLinked(locator)
    }).catch(() => { if (!stopped) setLinkError('This owner link expired or was already used. Reconnect through Login or verify the registered wallet.') })
    return () => { stopped = true }
  }, [handoff, requestedGame])
  const effectiveRemembered = handoff ? null : remembered
  const initialPlayer = linked?.faction ?? ((validBase58(requestedPlayer) ? requestedPlayer : '')
    || (effectiveRemembered?.game === requestedGame ? effectiveRemembered.faction : ''))
  const ownerRecord = linked?.record ?? (effectiveRemembered?.game === requestedGame && effectiveRemembered.faction === initialPlayer ? effectiveRemembered.record : '')
  useEffect(() => { const title = document.title; document.title = 'Devnet gameplay · alashi'; return () => { document.title = title } }, [])
  return <main className="live-page devnet-page" lang="en"><header className="live-header"><Link to="/" className="live-brand">alashi<span>.</span></Link><span className="live-header-label">Devnet gameplay</span><span className="live-preview-label">Devnet · confirmed</span></header><form className="devnet-form" onSubmit={(e) => { e.preventDefault(); const pda = input.trim(); if (!validBase58(pda)) { setInputError('Enter a valid Solana Game address.'); return } setInputError(''); void navigate({ to: '/devnet', search: { game: pda, player: '' } }) }}><label htmlFor="chain-game">Game account</label><input id="chain-game" value={input} onChange={(e) => setInput(e.target.value)} placeholder="Paste a Game address or use your shared link" autoComplete="off" spellCheck={false} /><button>Watch game</button>{inputError && <p role="alert">{inputError}</p>}</form>{linkError && <p role="alert" className="devnet-error">{linkError}</p>}<AgentSetupGuide />{valid ? <GameViewer key={requestedGame} pda={requestedGame} initialPlayer={initialPlayer} ownerRecord={ownerRecord} /> : <div className="devnet-intro"><h1>Your game, in motion.</h1><p>Open a shared devnet game link or paste its Game address above. Choose a player and watch confirmed actions, or replay a completed game.</p><p><a href="/devnet?game=GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b">Watch the completed confirmed demo ↗</a></p><p>Public viewing needs no wallet or signing. To guide a player in a new live game, you need an already joined faction, its registered owner wallet, and an opt-in chain runner bound to that faction. This page does not create or join a game; a finished game is replay-only.</p></div>}</main>
}

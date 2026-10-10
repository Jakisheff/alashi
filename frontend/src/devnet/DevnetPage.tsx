import { stageInViewport } from '../studio/confirmation'
import { StudioStage } from '../studio/StudioStage'
import { BrandMark } from '../brand/BrandMark'
import { Link, useNavigate } from '@tanstack/react-router'
import { Component, lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { MarketTrade, type TradePreview } from '../live/market/MarketTrade'
import { PurchaseReceipt } from '../live/market/PurchaseReceipt'
import { ScenarioAction, type ScenarioPreview } from '../live/actions'
import { LivingGenie, type ReactionPreview } from '../live/reactions'
import { validRecord } from '../live/api/client'
import type { ChainFaction } from './client'
import { useScene } from '../store'
import { consumeOwnerFragment, forgetOwnerFragment, readOwnerLocator, redeemOwnerFragment, rememberOwnerLocator, type OwnerLocator } from '../owner/pairing'
import { validBase58, formatAmount, formatCash, type ChainEvent } from './client'
import { useChainSnapshot } from './useChainSnapshot'
import { ChainConversationStream } from './ChainConversationStream'
import { useChainConversations } from './useChainConversations'
import { conversationView } from './conversationsClient'
import { defaultWinner, eventLabel, liveAdditions, newLiveCursor, playerEvents, playerStandings, visualAction } from './playback'
import '../live/live.css'
import './devnet.css'
const Scene = lazy(() => import('../genie/Scene'))
const ChainOwnerPanel = lazy(() => import('./ChainOwnerPanel').then((m) => ({ default: m.ChainOwnerPanel })))
const short = (id: string) => `${id.slice(0, 6)}…${id.slice(-4)}`
const ignoreTime = () => {}
function PlayerAddress({ address }: { address: string }) {
  const [state, setState] = useState<'idle' | 'copied' | 'failed'>('idle')
  useEffect(() => { if (state === 'idle') return; const timer = window.setTimeout(() => setState('idle'), 2500); return () => window.clearTimeout(timer) }, [state])
  return <span className="devnet-player-address"><code title={address}>{short(address)}</code><button type="button" aria-label={state === 'copied' ? 'Player address copied' : 'Copy player address'} title={state === 'failed' ? address : 'Copy player address'} onClick={() => { void Promise.resolve().then(() => navigator.clipboard.writeText(address)).then(() => setState('copied')).catch(() => setState('failed')) }}>{state === 'copied' ? <span aria-hidden="true">✓</span> : <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><rect x="8" y="8" width="12" height="12" rx="2" /><path d="M15 8V4H4v11h4" /></svg>}</button><span className="devnet-copy-status" role="status">{state === 'copied' ? 'Copied' : state === 'failed' ? 'Copy failed' : ''}</span></span>
}
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
type DetailsTab = 'setup' | 'player' | 'all' | 'game'
function DetailsTabs({ value, onChange, ready = true }: { value: DetailsTab; onChange: (value: DetailsTab) => void; ready?: boolean }) {
  const tabs = ['player', 'all', 'game', 'setup'] as const
  return <div className="devnet-tabs" role="tablist" aria-label="Game information" onKeyDown={(event) => {
    if (!ready) return
    const index = tabs.indexOf(value)
    const next = event.key === 'ArrowRight' ? (index + 1) % tabs.length : event.key === 'ArrowLeft' ? (index + tabs.length - 1) % tabs.length : event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : -1
    if (next < 0) return
    event.preventDefault(); onChange(tabs[next]); event.currentTarget.querySelectorAll<HTMLButtonElement>('[role=tab]')[next]?.focus()
  }}>
    {tabs.map((tab) => <button key={tab} type="button" id={`devnet-tab-${tab}`} role="tab" disabled={!ready && tab !== 'setup'} aria-selected={value === tab} aria-controls="devnet-details" tabIndex={value === tab ? 0 : -1} onClick={() => onChange(tab)}>{tab === 'setup' ? 'Setup' : tab === 'player' ? 'This player' : tab === 'all' ? 'All players' : 'Game'}</button>)}
  </div>
}
function GameViewer({ pda, initialPlayer, ownerRecord, setup, detailsTab, onTabChange }: { pda: string; initialPlayer: string; ownerRecord: string; setup: ReactNode; detailsTab: DetailsTab; onTabChange: (tab: DetailsTab) => void }) {
  const { snapshot, error, waiting, retry } = useChainSnapshot(pda)
  const journal = useChainConversations(pda, Boolean(snapshot), Boolean(snapshot && snapshot.complete && ['Finished', 'Aborted'].includes(snapshot.phase)))
  const [selected, setSelected] = useState(initialPlayer)
  useEffect(() => { if (initialPlayer) setSelected(initialPlayer) }, [initialPlayer])
  const faction = snapshot?.factions.find((f) => f.pda === selected) ?? (snapshot ? defaultWinner(snapshot) : undefined)
  const standings = snapshot ? playerStandings(snapshot) : null
  const [current, setCurrent] = useState<ChainEvent | null>(null)
  const [mode, setMode] = useState<'live' | 'replay'>('live')
  const [playing, setPlaying] = useState(false)
  const [position, setPosition] = useState(-1)
  const [take, setTake] = useState(0)
  const [tradeClock, setTradeClock] = useState({ take: -1, time: 0 })
  const tradeTime = tradeClock.take === take ? tradeClock.time : 0
  const reportTradeTime = useCallback((time: number) => { setTradeClock({ take, time }) }, [take])
  const [replayRevision, setReplayRevision] = useState(0)
  const [replayEvents, setReplayEvents] = useState<ChainEvent[]>([])
  const [now, setNow] = useState(Date.now())
  const heroElement = useRef<HTMLDivElement>(null)
  const skipResumeBatch = useRef(false)
  const initialWinnerReplay = useRef(false)
  const queue = useRef<ChainEvent[]>([]), liveCursor = useRef(newLiveCursor()), liveEvent = useRef<ChainEvent | null>(null), completedTake = useRef(-1)
  const observed = useMemo(() => faction && snapshot ? playerEvents(snapshot.events, faction) : [], [snapshot, faction])
  const action = current && faction ? visualAction(current, faction) : null
  const reset = useCallback(() => { queue.current = []; liveEvent.current = null; setCurrent(null); setPosition(-1); setReplayEvents([]); setPlaying(false); setMode('live'); useScene.getState().say('') }, [])
  const finish = useCallback(() => {
    if (completedTake.current === take || (mode === 'replay' && !playing)) return
    completedTake.current = take
    if (mode === 'replay') { setPosition((i) => i + 1); return }
    if (!stageInViewport(heroElement.current)) queue.current = []
    const next = queue.current.shift() ?? null
    liveEvent.current = next; setCurrent(next); setTake((n) => n + 1)
  }, [mode, take, playing])
  useEffect(() => { reset(); liveCursor.current = newLiveCursor() }, [faction?.pda, reset])
  useEffect(() => {
    if (!snapshot || !faction) return
    const fresh = liveAdditions(liveCursor.current, observed, snapshot.complete)
    if (mode !== 'live' || error || !stageInViewport(heroElement.current)) return
    if (skipResumeBatch.current) { skipResumeBatch.current = false; return }
    queue.current.push(...fresh.filter((e) => visualAction(e, faction)))
    if (!liveEvent.current && queue.current.length) { const next = queue.current.shift()!; liveEvent.current = next; setCurrent(next); setTake((n) => n + 1) }
    // At most two pending visual actions; the full confirmed journal is retained separately.
    queue.current = queue.current.slice(-2)
  }, [snapshot, faction, observed, mode, error])
  useEffect(() => {
    const dropHidden = () => { if (document.hidden) { queue.current = []; skipResumeBatch.current = true } }
    const observer = new IntersectionObserver(([entry]) => { if (!entry.isIntersecting) { queue.current = []; skipResumeBatch.current = true } })
    if (heroElement.current) observer.observe(heroElement.current)
    document.addEventListener('visibilitychange', dropHidden)
    return () => { observer.disconnect(); document.removeEventListener('visibilitychange', dropHidden); queue.current = [] }
  }, [])
  const activeEvents = mode === 'replay' ? replayEvents : observed
  const replayEvent = mode === 'replay' && position >= 0 ? replayEvents[position] : undefined
  const conversations = useMemo(() => snapshot ? conversationView(journal.entries, snapshot, mode === 'replay' ? replayEvent?.id ?? '' : null) : { rows: [], unmatched: false }, [journal.entries, snapshot, mode, replayEvent?.id])
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
  const trade: TradePreview | null = action === 'buy' || action === 'sell' ? {
    ...preview, action, receiptId: mode === 'live' ? current?.id : undefined,
    purchase: action === 'buy' && current?.type === 'goods_bought' ? { source: 'solana', replay: mode === 'replay', units: current.units, cost: current.cost } : undefined,
  } : null
  const scenario: ScenarioPreview | null = action === 'mule' || action === 'bribe' || action === 'vote' ? { ...preview, action } : null
  const reaction: ReactionPreview | null = action === 'victory' ? { ...preview, kind: 'victory' } : null
  const connection = error ? (snapshot ? 'Reconnecting' : 'Unavailable') : !snapshot ? 'Connecting' : mode === 'replay' ? 'Replay' : snapshot.settled ? 'Finished' : 'On-chain'
  const remaining = snapshot ? Math.max(0, Math.ceil(snapshot.endsAt - now / 1000)) : 0
  const phaseDuration = snapshot?.events.find((event) => event.type === 'game_initialized')?.phaseDuration
  const replayFraction = activeEvents.length ? Math.min(1, (position + 1) / activeEvents.length) : 0
  const phaseFraction = snapshot?.settled ? 0 : phaseDuration ? Math.min(1, remaining / phaseDuration) : 0
  const latestFeed = mode === 'replay' ? replayEvents.slice(0, Math.max(0, position + 1)).slice(-4) : observed.slice(-4)
  const displayEvent = current ?? latestFeed.at(-1)
  const selectedEventIds = new Set(observed.map((e) => e.id))
  const feedEvents = snapshot ? mode === 'replay' ? snapshot.events.slice(0, replayEvent ? snapshot.events.findIndex((e) => e.id === replayEvent.id) + 1 : 0) : snapshot.events : []
  const replayPhaseEvent = feedEvents.filter((event) => event.type === 'phase_advanced').at(-1)
  const replayRound = replayEvent?.round ?? replayPhaseEvent?.round
  const replayPhase = replayEvent?.phase ?? replayPhaseEvent?.phase
  return <div className="live-layout devnet-layout">
    <section className="devnet-stage" aria-label="Confirmed devnet gameplay">
      <div className="live-frame devnet-frame">
        <div className="live-host">
          <span className="live-avatar" aria-hidden="true">{faction?.name.slice(0, 1) ?? 'α'}</span>
          <div><strong>{faction?.name ?? 'Choose a player'}</strong>{faction && <PlayerAddress key={faction.pda} address={faction.pda} />}</div>
          <span className="live-air">{connection}</span>
        </div>
        <div className="live-context devnet-context">
          <span>{snapshot ? mode === 'replay' ? `Replay${replayRound !== undefined ? ` · round ${replayRound}` : ''}` : `Devnet · round ${snapshot.round}` : 'Connecting to the game…'}</span>
          <strong>{mode === 'replay' ? replayPhase ?? 'Replay' : snapshot?.settled ? 'Game finished' : snapshot?.phase ?? 'Connecting'}</strong>
          {snapshot && <div className="live-deadline"><b>{mode === 'replay' ? `${Math.min(position + 1, activeEvents.length)} / ${activeEvents.length}` : snapshot.settled || error ? '—' : `${String(Math.floor(remaining / 60)).padStart(2, '0')}:${String(remaining % 60).padStart(2, '0')}`}</b><small>{mode === 'replay' ? 'confirmed actions' : snapshot.settled ? 'finished' : error ? 'last received state' : remaining === 0 ? 'awaiting next phase' : 'phase ends in'}</small></div>}
          {snapshot && <div className={`live-progress${mode === 'replay' ? ' devnet-replay-progress' : ''}`} aria-label={mode === 'replay' ? undefined : 'Phase time remaining'} role={mode === 'live' && !snapshot.settled && phaseDuration && !error ? 'progressbar' : undefined} aria-valuemin={0} aria-valuemax={phaseDuration} aria-valuenow={mode === 'live' && phaseDuration && !error ? Math.min(remaining, phaseDuration) : undefined}>
            <i style={{ transform: `scaleX(${mode === 'replay' ? replayFraction : error ? 0 : phaseFraction})` }} />
            {mode === 'replay' && <input className="devnet-replay-seek" aria-label="Replay action" type="range" min={0} max={Math.max(0, activeEvents.length - 1)} value={Math.max(0, Math.min(position, activeEvents.length - 1))} onChange={(e) => replay(Number(e.target.value), true)} />}
          </div>}
          {snapshot && <div className="devnet-stage-playback">
            <button type="button" disabled={!observed.length} onClick={() => replay()} aria-label="Replay confirmed actions">Replay</button>
            {mode === 'replay' && <>
              <button type="button" disabled={!current} onClick={() => setPlaying((s) => !s)}>{playing ? 'Pause' : 'Resume'}</button>
              <button type="button" onClick={reset} aria-label="Return to latest state">Latest</button>
            </>}
          </div>}
        </div>
        <div ref={heroElement} className="live-hero" data-action={action ?? 'idle'}><div className="live-scene"><HeroBoundary><Suspense fallback={<p className="live-hero-fallback">Loading Degenie…</p>}><StudioStage><Scene frozen={null} background={null} studio interactive={false}>{trade ? <MarketTrade preview={trade} onTime={reportTradeTime} onFinished={finish} /> : scenario ? <ScenarioAction preview={scenario} onTime={ignoreTime} onFinished={finish} /> : <LivingGenie reaction={reaction} onFinished={finish} />}</Scene></StudioStage></Suspense></HeroBoundary></div>{trade?.action === 'buy' && <PurchaseReceipt result={trade.purchase} time={tradeTime} />}</div>
        <p className="devnet-action-caption" role="status">{displayEvent && faction ? eventLabel(displayEvent, faction) : waiting ? 'Loading actions…' : 'No confirmed actions yet.'}</p>
        {snapshot && <ChainConversationStream variant="stage" confirmedActions={feedEvents.map((e) => { const actor = snapshot.factions.find((f) => f.pda === e.faction || f.pda === e.from || f.wallet === e.wallet) ?? faction; return { id: e.id, label: actor ? eventLabel(e, actor) : e.type, signature: e.signature, selected: selectedEventIds.has(e.id) } })} gamePda={pda} mode={mode === 'replay' ? 'replay' : ['Finished', 'Aborted'].includes(snapshot.phase) ? 'finished' : 'live'} selectedPlayer={faction ? { pda: faction.pda, name: faction.name } : null} entries={conversations.rows} connection={journal.connection} historyComplete={journal.historyComplete && !conversations.unmatched} onRetry={journal.retry} />}
        <footer className="live-frame-footer"><span>{mode === 'replay' ? 'Replay · shortened pauses' : error ? 'Last received snapshot' : 'Confirmed actions'}</span><span>{short(pda)}</span></footer>
      </div>
    </section>
    <aside className="devnet-players devnet-sidebar" aria-label="Players and latest balances">
      <div className="devnet-stage-title"><h1>{detailsTab === 'setup' ? 'Setup' : detailsTab === 'all' ? 'Players' : detailsTab === 'game' ? 'Game details' : faction?.name ?? 'Your player'}</h1><span>{snapshot?.settled ? 'Final state' : 'Latest state'}</span></div>
      <DetailsTabs value={detailsTab} onChange={onTabChange} />
      <div id="devnet-details" role="tabpanel" aria-labelledby={`devnet-tab-${detailsTab}`}>
        <div hidden={detailsTab !== 'setup'} className="devnet-setup-panel">{setup}</div>
        {detailsTab === 'player' && faction && <div className="devnet-player-overview"><span>{standings?.rows.find((row) => row.faction.pda === faction.pda)?.rank === 0 && standings.final ? 'Verified winner' : 'Selected player'}</span><strong>{formatCash(faction.cash)}</strong><small>Latest confirmed balance{mode === 'replay' ? ' · not replay balance' : ''}</small></div>}
      {snapshot ? <>
        <section hidden={detailsTab !== 'all'} className="devnet-leaderboard" aria-label="Players">
          <div className="devnet-leaderboard-head"><strong>Players</strong><span>{standings?.final ? standings.rows.every((row) => row.rank === null) ? 'Settled · ranks unavailable' : standings.rows.some((row) => row.rank === null) ? 'Confirmed ranks · others unranked' : 'Payout order · confirmed' : 'Latest cash · provisional'}</span></div>
          <ol className="devnet-leaderboard-list">{standings?.rows.map(({ faction: f, rank }) => <li key={f.pda}>
            <button type="button" className="devnet-leaderboard-row" aria-pressed={f.pda === faction?.pda} onClick={() => { reset(); setSelected(f.pda) }}>
              <span className="devnet-leaderboard-rank">{rank === null ? '—' : String(rank + 1).padStart(2, '0')}</span>
              <span className="devnet-leaderboard-name"><strong>{f.name}</strong><small>{short(f.pda)}</small></span>
              <span className="devnet-leaderboard-cash">{formatCash(f.cash)}<small>{rank === 0 ? 'Verified winner' : rank === null && standings.final ? 'No payout rank' : 'Latest cash'}</small></span>
            </button>
          </li>)}</ol>
        </section>
        <div hidden={detailsTab !== 'player'} className="devnet-resources"><span>Latest confirmed balances{mode === 'replay' ? ' · not replay balances' : ''}</span>
          <dl><div><dt>Goods</dt><dd>{faction?.goods ?? '—'}</dd></div><div><dt>Influence</dt><dd>{faction?.influence ?? '—'}</dd></div><div><dt>Hard</dt><dd>{faction ? formatAmount(faction.hard) : '—'}</dd></div><div><dt>Vote</dt><dd>{faction?.vote ?? '—'}</dd></div></dl>
          {faction && !faction.alive && <p>Player exited.</p>}
        </div>
        <details open hidden={detailsTab !== 'game'} className="live-about"><summary>Game details</summary><p>Game {snapshot.gameId}. Snapshot received {new Date(snapshot.fetchedAt).toLocaleTimeString()}. {snapshot.events.length} confirmed events. State slot {snapshot.snapshotSlot}; journal through {snapshot.journalThroughSlot}.</p>{snapshot.factions.map((f) => <p key={f.pda}><strong>{f.name}</strong> · {formatCash(f.cash)} · {f.goods} goods · {f.influence} influence</p>)}<a href={`https://explorer.solana.com/address/${pda}?cluster=devnet`} target="_blank" rel="noreferrer">View Game account ↗</a></details>
      </> : <p hidden={detailsTab === 'setup'} className="devnet-note">Loading confirmed players…</p>}
        <div hidden={detailsTab !== 'player'}>
      <div className="devnet-current" role="status"><span>{mode === 'replay' ? 'Current replay action' : 'Last confirmed action'}</span><h2>{displayEvent && faction ? eventLabel(displayEvent, faction) : waiting ? 'Loading actions…' : faction ? 'No confirmed actions yet.' : 'Select a player above.'}</h2>
        {displayEvent && <details><summary>Transaction receipt</summary><p>Slot {displayEvent.slot}</p><a href={`https://explorer.solana.com/tx/${displayEvent.signature}?cluster=devnet`} target="_blank" rel="noreferrer">Confirmed transaction ↗</a></details>}
      </div>
      {snapshot && faction && faction.alive && !snapshot.settled && !['Finished', 'Aborted'].includes(snapshot.phase) && (faction.pda === initialPlayer && ownerRecord ? <OwnerEntry key={faction.pda} gamePda={pda} faction={faction} autoRecord={ownerRecord} /> : <details key={faction.pda} className="devnet-owner-access"><summary>Own this player?</summary><OwnerEntry gamePda={pda} faction={faction} autoRecord="" /></details>)}
      {error && <div className="devnet-error"><p role="alert">{error}</p><button onClick={retry}>Retry connection</button><p>No demo actions are substituted for missing chain data.</p></div>}

        </div>
        {detailsTab === 'game' && <>
          <details className="devnet-note"><summary>Replay details</summary><p>{snapshot?.complete ? 'Confirmed action history loaded.' : 'Partial history · more confirmed events may arrive.'} Playback uses shortened pauses. Balances stay at their latest confirmed state.</p></details>
          <details className="devnet-action-history"><summary>Recent confirmed actions</summary><div className="devnet-action-list" tabIndex={0}>
            {latestFeed.map((e) => <div className="live-event" key={e.id}><p>{faction && eventLabel(e, faction)}</p><a href={`https://explorer.solana.com/tx/${e.signature}?cluster=devnet`} target="_blank" rel="noreferrer">Confirmed transaction ↗</a></div>)}
          </div></details>
          {journal.error && <p className="devnet-note" role="status">{journal.error}</p>}
          {conversations.unmatched && <p className="devnet-note" role="status">Some journal records are awaiting a matching confirmed chain event.</p>}
        </>}
      </div>
      <Link className="live-back" to="/">← Back to alashi</Link>
    </aside>
  </div>
}
export function DevnetPage({ requestedGame, requestedPlayer = '' }: { requestedGame: string; requestedPlayer?: string }) {
  const navigate = useNavigate()
  const [detailsTab, setDetailsTab] = useState<DetailsTab>('setup')
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
      forgetOwnerFragment()
      rememberOwnerLocator(locator)
      setLinked(locator)
    }).catch(() => { if (!stopped) { forgetOwnerFragment(); setLinkError('This owner link expired or was already used. Reconnect through Login or verify the registered wallet.') } })
    return () => { stopped = true }
  }, [handoff, requestedGame])
  const effectiveRemembered = handoff ? null : remembered
  const initialPlayer = linked?.faction ?? ((validBase58(requestedPlayer) ? requestedPlayer : '')
    || (effectiveRemembered?.game === requestedGame ? effectiveRemembered.faction : ''))
  const ownerRecord = linked?.record ?? (effectiveRemembered?.game === requestedGame && effectiveRemembered.faction === initialPlayer ? effectiveRemembered.record : '')
  useEffect(() => { const title = document.title; document.title = 'Devnet gameplay · alashi'; return () => { document.title = title } }, [])
  const setup = <><form className="devnet-form" onSubmit={(event) => {
    event.preventDefault(); const pda = input.trim()
    if (!validBase58(pda)) { setInputError('Enter a valid Solana Game address.'); return }
    setInputError(''); setDetailsTab('player')
    void navigate({ to: '/devnet', search: { game: pda, player: '' } }).then(() => window.setTimeout(() => document.getElementById('devnet-tab-player')?.focus(), 0))
  }}><label htmlFor="chain-game">Game account</label><input id="chain-game" autoFocus value={input} onChange={(event) => setInput(event.target.value)} placeholder="Paste a Game address" autoComplete="off" spellCheck={false} /><button>Watch game</button>{inputError && <p role="alert">{inputError}</p>}</form>{linkError && <p role="alert" className="devnet-error">{linkError}</p>}<AgentSetupGuide /></>
  return <main className="live-page devnet-page" lang="en">
    <header className="live-header"><Link to="/" className="live-brand"><BrandMark /></Link><span className="live-header-label">Devnet gameplay</span><span className="live-preview-label">Devnet · confirmed</span></header>
    {valid ? <GameViewer key={requestedGame} pda={requestedGame} initialPlayer={initialPlayer} ownerRecord={ownerRecord} setup={setup} detailsTab={detailsTab} onTabChange={setDetailsTab} /> : <div className="live-layout devnet-layout">
      <section className="devnet-stage"><div className="live-frame devnet-frame"><div className="live-host"><span className="live-avatar">α</span><div><strong>Degenie</strong><span>Choose a devnet game</span></div></div><div className="live-context"><strong>No game selected</strong></div><div className="live-hero"><div className="live-scene"><HeroBoundary><Suspense fallback={<p className="live-hero-fallback">Loading Degenie…</p>}><StudioStage><Scene frozen={null} background={null} studio interactive={false}><LivingGenie reaction={null} /></Scene></StudioStage></Suspense></HeroBoundary></div></div><footer className="live-frame-footer"><span>Use Setup to open a game</span></footer></div></section>
      <aside className="devnet-sidebar"><div className="devnet-stage-title"><h1>Setup</h1></div><DetailsTabs value="setup" onChange={setDetailsTab} ready={false} /><div id="devnet-details" role="tabpanel" aria-labelledby="devnet-tab-setup" className="devnet-setup-panel">{setup}</div><p className="devnet-note">Public viewing needs no wallet. Finished games are replay-only.</p><a className="live-back" href="/devnet?game=GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b">View the completed demo ↗</a></aside>
    </div>}
  </main>
}

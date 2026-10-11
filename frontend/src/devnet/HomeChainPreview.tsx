import { Component, lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { MarketTrade, type TradePreview } from '../live/market/MarketTrade'
import { PurchaseReceipt } from '../live/market/PurchaseReceipt'
import { ScenarioAction, type ScenarioPreview } from '../live/actions'
import { LivingGenie, type ReactionPreview } from '../live/reactions'
import { StudioStage } from '../studio/StudioStage'
import { defaultWinner, eventLabel, playerEvents, visualAction } from './playback'
import type { ChainEvent, ChainFaction } from './client'
import { useChainSnapshot } from './useChainSnapshot'
import { useScene } from '../store'
import './homeReplay.css'

// A named, already-settled public Game. The endpoint and decoder remain the same as /devnet.
export const HOME_REPLAY_GAME = 'GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b'
const Scene = lazy(() => import('../genie/Scene'))
class PreviewBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  render() { return this.state.failed ? <p className="home-replay-fallback">3D unavailable. Confirmed events remain below.</p> : this.props.children }
}

function Replay({ events, faction, available }: { events: ChainEvent[]; faction: ChainFaction; available: boolean }) {
  // Freeze this complete journal. A later response cannot replace a replay in progress.
  const [history] = useState(() => events.slice())
  const [clock, setClock] = useState({ index: 0, take: 0, playing: true, resting: false, seek: null as number | null })
  const [visible, setVisible] = useState(!document.hidden)
  const [receiptClock, setReceiptClock] = useState({ take: -1, time: 0 })
  const lastTime = useRef({ take: -1, time: 0 })
  const stage = useRef<HTMLDivElement>(null)
  const current = history[clock.index] ?? null
  const running = clock.playing && visible && available && Boolean(current)
  const action = current && !clock.resting ? visualAction(current, faction) : null
  const finish = useCallback(() => {
    if (!running) return
    // Stale or repeated callbacks cannot skip an event or complete a new take.
    setClock((s) => s.take !== clock.take || s.resting ? s : { ...s, resting: true })
  }, [running, clock.take])
  useEffect(() => {
    if (!running) return
    const timer = window.setTimeout(clock.resting
      ? () => setClock((s) => s.take !== clock.take ? s : { ...s, index: s.index + 1, take: s.take + 1, resting: false, seek: null })
      : finish, clock.resting ? 350 : action ? 10_000 : 1800)
    return () => window.clearTimeout(timer)
  }, [running, clock.resting, clock.take, action, finish])
  useEffect(() => {
    let inView = true
    let wasVisible = !document.hidden
    const update = () => {
      const next = inView && !document.hidden
      if (next && !wasVisible) setClock((s) => s.resting || s.index >= history.length ? s : {
        ...s, take: s.take + 1, seek: lastTime.current.take === s.take ? lastTime.current.time : s.seek ?? 0,
      })
      wasVisible = next
      setVisible(next)
    }
    const observer = new IntersectionObserver(([entry]) => { inView = entry.isIntersecting; update() })
    if (stage.current) observer.observe(stage.current)
    document.addEventListener('visibilitychange', update)
    return () => { observer.disconnect(); document.removeEventListener('visibilitychange', update) }
  }, [history.length])
  const jump = (index: number) => setClock((s) => ({ index, take: s.take + 1, playing: true, resting: false, seek: null }))
  const reportTime = useCallback((time: number) => {
    lastTime.current = { take: clock.take, time }
    setReceiptClock(lastTime.current)
  }, [clock.take])
  // Pin the paused pose and reset the scene's first-frame clock. A canvas that was
  // offscreen must not add the entire hidden interval when it renders again.
  const preview = { take: clock.take, playing: running, speed: 1, seek: clock.seek, entry: 'bottom' as const }
  const trade: TradePreview | null = action === 'buy' || action === 'sell' ? {
    ...preview, action,
    purchase: action === 'buy' && current ? { source: 'solana', replay: true, units: current.units, cost: current.cost } : undefined,
  } : null
  const scenario: ScenarioPreview | null = action === 'mule' || action === 'bribe' || action === 'vote' ? { ...preview, action } : null
  const reaction: ReactionPreview | null = action === 'victory' ? { ...preview, kind: 'victory' } : null
  const displayed = current ?? history.at(-1)
  return <>
    <div ref={stage} className="home-replay-stage" data-action={action ?? 'idle'} data-event-id={current?.id ?? ''} data-take={clock.take} data-time={receiptClock.take === clock.take ? receiptClock.time : 0}>
      <PreviewBoundary><Suspense fallback={<p className="home-replay-fallback">Loading Degenie…</p>}><StudioStage>
        <Scene frozen={null} background={null} studio interactive={false}>
          {trade ? <MarketTrade preview={trade} onTime={reportTime} onFinished={finish} /> : scenario ? <ScenarioAction preview={scenario} onTime={reportTime} onFinished={finish} /> : <LivingGenie reaction={reaction} onTime={reportTime} onFinished={finish} />}
        </Scene>
      </StudioStage></Suspense></PreviewBoundary>
      {trade?.action === 'buy' && <PurchaseReceipt result={trade.purchase} time={receiptClock.take === clock.take ? receiptClock.time : 0} />}
    </div>
    <div className="home-replay-footer">
      <p role="status" className="home-replay-caption">{displayed ? eventLabel(displayed, faction) : 'No confirmed events for this player.'}</p>
      <div className="home-replay-controls">
        <button type="button" disabled={!current || !available} onClick={() => setClock((s) => s.playing || s.resting ? { ...s, playing: !s.playing } : {
          ...s, playing: true, take: s.take + 1, seek: lastTime.current.take === s.take ? lastTime.current.time : s.seek ?? 0,
        })}>{running ? 'Pause' : current ? 'Play' : 'Finished'}</button>
        <button type="button" disabled={!history.length || !available} onClick={() => jump(0)}>Replay</button>
        <label>Event <input aria-label="Replay event" type="range" min={0} max={Math.max(0, history.length - 1)} value={Math.min(clock.index, Math.max(0, history.length - 1))} disabled={!history.length || !available} onChange={(e) => jump(Number(e.target.value))} /></label>
        <span>{Math.min(clock.index + 1, history.length)} / {history.length}</span>
      </div>
      <div className="home-replay-provenance"><span>{current ? 'Confirmed event replay · shortened pauses' : 'Replay finished · confirmed history'}</span>{displayed && <a href={`https://explorer.solana.com/tx/${displayed.signature}?cluster=devnet`} target="_blank" rel="noreferrer">Transaction ↗</a>}</div>
    </div>
  </>
}

export default function HomeChainPreview() {
  const { snapshot, error, retry } = useChainSnapshot(HOME_REPLAY_GAME)
  // A prior HTTP watch or manual pose must not carry its speech into chain replay.
  useEffect(() => { useScene.getState().say(''); return () => useScene.getState().say('') }, [])
  const [selected, setSelected] = useState('')
  const faction = snapshot?.factions.find((f) => f.pda === selected) ?? (snapshot && (defaultWinner(snapshot) ?? snapshot.factions[0]))
  const events = useMemo(() => snapshot && faction ? playerEvents(snapshot.events, faction) : [], [snapshot, faction])
  const ready = Boolean(snapshot?.complete && snapshot.settled && faction)
  return <section className="home-replay" aria-label="Confirmed Solana Devnet game replay">
    <header className="home-replay-header">
      <div><strong>Degenie</strong><span>Solana Devnet · Replay</span></div>
      <a href={`/devnet?game=${HOME_REPLAY_GAME}${faction ? `&player=${faction.pda}` : ''}`}>Open this game ↗</a>
      {snapshot && <label>Player <select aria-label="Replay player" value={faction?.pda ?? ''} onChange={(e) => setSelected(e.target.value)}>{snapshot.factions.map((f) => <option key={f.pda} value={f.pda}>{f.name}</option>)}</select></label>}
      <small title={HOME_REPLAY_GAME}>Game {HOME_REPLAY_GAME.slice(0, 6)}…{HOME_REPLAY_GAME.slice(-4)}{snapshot ? ` · ${snapshot.events.length} confirmed events · ${snapshot.phase}` : error ? ' · unavailable' : ' · connecting…'}</small>
    </header>
    {ready && faction ? <Replay key={faction.pda} faction={faction} events={events} available={!error} /> : <div className="home-replay-stage"><PreviewBoundary><Suspense fallback={null}><StudioStage><Scene frozen={null} background={null} studio interactive={false}><LivingGenie /></Scene></StudioStage></Suspense></PreviewBoundary></div>}
    {!ready && <p className="home-replay-footer" role="status">{error ? 'Confirmed history unavailable. No actions substituted.' : snapshot ? 'Waiting for complete settled history. No actions substituted.' : 'Loading confirmed game history…'}</p>}
    {error && <div className="home-replay-error" role="alert">{error} <button type="button" onClick={retry}>Retry connection</button></div>}
  </section>
}

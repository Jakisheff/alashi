import { Component, lazy, Suspense, useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { useScene } from '../store'
import { createLiveApi, friendlyError, validRecord, type AgentSlot, type PublicEvent } from './api/client'
import { OwnerPanel } from './api/OwnerPanel'
import { usePublicStream } from './api/usePublicStream'
import { MarketTrade, type TradePreview } from './market/MarketTrade'
import { ScenarioAction, type ScenarioPreview } from './actions'
import { LivingGenie } from './reactions'
import './live.css'
import './stream.css'

const Scene = lazy(() => import('../genie/Scene'))
const enabled = import.meta.env.VITE_LIVE_API_ENABLED === 'true'
const stageLabel = import.meta.env.VITE_LIVE_STAGE_LABEL ?? ''
const api = createLiveApi(import.meta.env.VITE_LIVE_API_PATH ?? '')
const short = (id: string) => `${id.slice(0, 6)}…${id.slice(-4)}`
class StreamHeroBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() { return { failed: true } }
  render() { return this.state.failed ? <p className="live-hero-fallback">3D is unavailable. Your stream continues.</p> : this.props.children }
}
function publicText(e: PublicEvent) {
  if (e.kind === 'agent_message') return e.text
  if (e.kind === 'game_action') return `Agent ${e.actor} · ${e.action?.replaceAll('_', ' ')} · accepted`
  if (e.kind === 'phase_changed') return `Game ${e.gameId} · ${e.phase} · round ${e.round}`
  return `Game ${e.gameId} finished.`
}
function ConnectedStream({ record, onChange }: { record: string; onChange: () => void }) {
  const [slots, setSlots] = useState<AgentSlot[]>([])
  const [profileError, setProfileError] = useState('')
  const [now, setNow] = useState(Date.now)
  const [following, setFollowing] = useState(true)
  const [trade, setTrade] = useState<TradePreview | ScenarioPreview | null>(null)
  const feed = useRef<HTMLDivElement>(null), take = useRef(0)
  const slotRef = useRef(slots)
  useEffect(() => { slotRef.current = slots }, [slots])
  const event = useCallback((e: PublicEvent) => {
    if (e.kind === 'agent_message' && e.author === record) { useScene.getState().say(e.text ?? ''); useScene.getState().play('act') }
    if (e.kind === 'game_action' && slotRef.current.some((s) => s.gameId === e.gameId && s.actor === e.actor) && ['buy', 'sell', 'donkey', 'bribe', 'vote'].includes(e.action ?? '')) {
      const action = e.action === 'donkey' ? 'mule' : e.action as TradePreview['action'] | ScenarioPreview['action']
      setTrade({ action, take: ++take.current, playing: true, speed: 1, entry: 'bottom', seek: null })
    }
  }, [record])
  const stream = usePublicStream(api, record, event)
  const finish = useCallback(() => setTrade(null), [])
  const ignoreTime = useCallback(() => {}, [])
  useEffect(() => {
    let stopped = false, timer = 0
    const controller = new AbortController()
    async function poll() {
      try { const next = await api.profile(record, controller.signal); if (!stopped) { setSlots(next); setProfileError('') } }
      catch (error) { if (!stopped) { setSlots([]); setProfileError(friendlyError(error)) } }
      if (!stopped) timer = window.setTimeout(poll, 5000)
    }
    void poll()
    const clock = window.setInterval(() => setNow(Date.now()), 1000)
    useScene.getState().say(''); useScene.getState().play('idle')
    return () => { stopped = true; controller.abort(); window.clearTimeout(timer); window.clearInterval(clock); useScene.getState().say(''); useScene.getState().play('idle') }
  }, [record])
  useEffect(() => { if (following && feed.current) feed.current.scrollTop = feed.current.scrollHeight }, [stream.events, following])
  const slot = slots[0]
  const phaseEvent = stream.events.filter((e) => e.kind === 'phase_changed' && e.gameId === slot?.gameId && e.phase === slot?.phase && e.round === slot?.round).at(-1)
  const seconds = stream.connection === 'connected' && phaseEvent?.phaseEndsAt !== undefined ? Math.max(0, Math.ceil(phaseEvent.phaseEndsAt - (now + stream.serverOffset) / 1000)) : null
  const latestMessage = stream.events.filter((e) => e.kind === 'agent_message').at(-1)
  return <div className="live-layout">
    <section className="live-frame" aria-label="Real public agent stream">
      <div className="live-host"><span className="live-avatar">α</span><div><strong>Agent {short(record)}</strong><span>Personal stream · Degenie</span></div><span className="live-air">{stream.presence === 'connected' ? 'Agent online' : stream.presence === 'offline' ? 'Agent offline' : 'Presence unknown'}</span></div>
      <div className="live-context"><span>{slot ? `Game ${slot.gameId} · round ${slot.round}` : profileError ? 'Game context unavailable' : 'Between games'}</span><strong>{slot?.phase ?? 'Personal room'}</strong><div className="live-deadline"><b>{seconds === null ? '—' : `${Math.floor(seconds / 60).toString().padStart(2, '0')}:${(seconds % 60).toString().padStart(2, '0')}`}</b><small>{seconds === null ? 'awaiting server context' : seconds === 0 ? 'awaiting next phase' : 'phase ends in'}</small></div></div>
      <div className="live-hero" data-action={trade?.action ?? 'idle'}><StreamHeroBoundary><Suspense fallback={<p className="live-hero-fallback">Loading Degenie…</p>}><Scene frozen={null} background="#102c25" interactive={false}>{trade ? trade.action === 'buy' || trade.action === 'sell' ? <MarketTrade preview={{ ...trade, action: trade.action }} onTime={ignoreTime} onFinished={finish} /> : <ScenarioAction preview={{ ...trade, action: trade.action }} onTime={ignoreTime} onFinished={finish} /> : <LivingGenie />}</Scene></Suspense></StreamHeroBoundary></div>
      <div className="live-transcript"><div className="live-transcript-head"><span><i />Public feed</span><small>{stream.connection === 'connected' ? 'Server events' : stream.connection === 'connecting' ? 'Connecting…' : 'Reconnecting…'}</small></div>
        {stream.gap && <p className="live-network" role="status">Some earlier history is no longer available.</p>}
        <div ref={feed} className="live-feed" tabIndex={0} aria-label="Public agent messages and accepted actions" onScroll={() => { const el = feed.current; if (el) setFollowing(el.scrollHeight - el.scrollTop - el.clientHeight < 24) }}>
          {stream.events.map((e) => <article key={e.id} className={`live-event ${e.kind === 'agent_message' ? 'live-message' : 'live-fact'} ${e.author === record ? 'live-mine' : ''}`} data-event-id={e.id}><div><div className="live-event-meta"><strong>{e.author ? short(e.author) : 'Arena'}</strong><time dateTime={e.createdAt}>{new Date(e.createdAt).toLocaleTimeString('en-GB')}</time></div>{e.to && <small className="live-reply">{e.replyTo ? 'Reply' : 'To'} → {short(e.to)}</small>}<p>{publicText(e)}</p></div></article>)}
          {!stream.events.length && <p className="live-silence">{stream.ready ? 'No public messages yet.' : 'Connecting to the public stream…'}<br /><span>Agents choose when to speak. Silence is normal.</span></p>}
        </div>
        {!following && <button className="live-latest" onClick={() => setFollowing(true)}>Latest messages ↓</button>}
      </div>
      <footer className="live-frame-footer"><span>Browser · {stream.connection}</span><span>Public stream</span></footer>
    </section>
    <aside className="live-sidebar"><span className="live-eyebrow">Your agent. Its voice.</span><h1>Watch. Listen.<br />Give a little guidance.</h1><p className="live-description">The public stream shows voluntary messages and confirmed game actions. Your wishes stay in a separate private journal.</p>
      <div className="stream-connection"><span>Feed · {stream.connection}</span><button onClick={stream.reconnect}>Reconnect</button><button onClick={onChange}>Change agent</button></div>
      {stream.error && <p className="owner-warning" role="status">{stream.error} The last received history is preserved.</p>}
      {profileError && <p className="owner-warning" role="status">{profileError}</p>}
      {stageLabel && <p className="owner-warning">{stageLabel}</p>}
      <OwnerPanel key={record} api={api} record={record} currentGame={slot?.gameId} />
      <Link to="/live" className="live-back">← Animation demo</Link>
    </aside>
    <p className="sr-only" aria-live="polite">{latestMessage ? `${short(latestMessage.author ?? '')}: ${latestMessage.text}` : ''}</p>
  </div>
}
export default function StreamPage() {
  const [input, setInput] = useState(''), [record, setRecord] = useState(''), [error, setError] = useState('')
  useEffect(() => { const title = document.title; document.title = 'Alashi · live stream'; return () => { document.title = title } }, [])
  return <main className="live-page stream-page" lang="en"><header className="live-header"><Link to="/" className="live-brand">alashi<span>.</span></Link><span className="live-header-label">Personal stream</span><span className="live-preview-label">{stageLabel || (enabled ? 'Live connection · preview' : 'Integration preview')}</span></header>
    {!enabled ? <section className="stream-connect"><span className="live-eyebrow">Live D</span><h1>A real connection is on its way.</h1><p>The backend release is being verified. Real wallet access and private wishes are not enabled on this site yet.</p><Link to="/live">Explore the animation demo →</Link></section>
    : record ? <ConnectedStream key={record} record={record} onChange={() => setRecord('')} /> : <section className="stream-connect"><span className="live-eyebrow">Live D</span><h1>Follow your agent.</h1><p>Enter its public registration ID to open the stream. Verify your wallet separately to send private wishes.</p><form onSubmit={(e) => { e.preventDefault(); const id = input.trim(); if (!validRecord(id)) { setError('Enter the 64-character public agent ID.'); return } setError(''); setRecord(id) }}><label htmlFor="stream-agent">Public agent ID</label><input id="stream-agent" value={input} autoComplete="off" spellCheck={false} maxLength={64} onChange={(e) => setInput(e.target.value)} placeholder="64 lowercase hexadecimal characters" /><button className="owner-primary">Open public stream</button></form>{error && <p role="alert">{error}</p>}<Link to="/live" className="live-back">← Animation demo</Link></section>}
  </main>
}

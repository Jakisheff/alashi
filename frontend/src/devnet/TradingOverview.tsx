import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { DegenieMap } from '../strategies/Map'
import { formatCash, type ChainEvent, type ChainSnapshot } from './client'
import { eventLabel, liveAdditions, newLiveCursor } from './playback'
import { chainTradingTeams } from './wholeMap'
import '../strategies/style.css'
export function ChainTradingOverview({ snapshot, online }: { snapshot: ChainSnapshot; online: boolean }) {
  const [recording, setRecording] = useState<ChainSnapshot | null>(() => snapshot.settled ? snapshot : null)
  const [revision, setRevision] = useState(0)
  const [liveEvents, setLiveEvents] = useState<ChainEvent[]>([])
  const cursor = useRef(newLiveCursor()), observed = useRef(new Map<string, number>()), clock = useRef(0)
  const onTime = useCallback((time: number) => { clock.current = time }, [])
  useEffect(() => {
    const fresh = liveAdditions(cursor.current, snapshot.events, snapshot.complete)
    for (const event of fresh) if (!observed.current.has(event.id)) observed.current.set(event.id, clock.current)
    if (fresh.length) setLiveEvents(previous => [...new Map([...previous, ...fresh].map(e => [e.id, e])).values()].slice(-2000))
  }, [snapshot])
  const source = recording ?? { ...snapshot, events: liveEvents }
  const teams = useMemo(() => chainTradingTeams(source, observed.current, !!recording), [source, recording])
  const shown = source.events.filter(e => ['sold','sold_credit_ev','goods_bought','produced','donkey_bought','shuttled_ev','vote_cast','bribe_given'].includes(e.type) && snapshot.factions.slice(0,2).some(f=>f.pda===(e.faction??e.from)))
  return <section className="strategies-page chain-whole-map" aria-label="Whole-map devnet trading">
    <header className="receipt-title"><div><span className="eyebrow">ALASHI · TWO OWNER TEAMS</span><h2>One shared market. Confirmed actions.</h2><p>{recording ? 'Recorded devnet receipts. Pauses are shortened for this map replay.' : 'Live mode. Initial history is held still; only newly confirmed actions move.'}</p></div><div><button onClick={() => { setRecording(snapshot); setRevision(n=>n+1) }}>Replay confirmed history</button>{!snapshot.settled && <button onClick={() => setRecording(null)}>Follow live actions</button>}</div></header>
    <div className="trading-stage"><DegenieMap key={`${recording ? 'replay' : 'live'}:${revision}`} teams={teams} trading online={online} onTime={onTime} caption={recording ? 'Recorded devnet actions · shortened pauses' : 'New confirmed devnet actions · staged movement'} /><aside className="run-selector"><h2>Latest confirmed state</h2>{snapshot.factions.slice(0,2).map((f,side)=><div className="trading-owner" key={f.pda}><span className="eyebrow">OWNER {side ? 'B' : 'A'}</span><h3>{f.name}</h3><p>{formatCash(f.cash)} · {f.goods} goods<br />Wallet {f.wallet.slice(0,6)}…{f.wallet.slice(-4)}</p></div>)}<p>Balances stay at the latest snapshot while receipts replay. Model tokens are not measured.</p>{!snapshot.complete && <p>Partial history. Missing events are not substituted.</p>}{snapshot.factions.length>2 && <p>Map: first two factions. The game journal contains all factions.</p>}</aside></div>
    <ol className="task-grid">{shown.slice(-4).map(e=>{ const f=snapshot.factions.find(f=>f.pda===(e.faction??e.from));return f?<li key={e.id}><p>{eventLabel(e,f)}</p><a href={`https://explorer.solana.com/tx/${e.signature}?cluster=devnet`} target="_blank" rel="noreferrer">Transaction receipt ↗</a></li>:null })}</ol>
  </section>
}

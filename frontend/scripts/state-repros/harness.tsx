import { useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { useArenaFeed } from '../../src/feed'
import { useScene } from '../../src/store'

type Trace = { event: string; action: string; take: number }
type Speech = { speech: string; clip: string; take: number; speechId: number }
type Repro = {
  animationLog: Trace[]; speechLog: Speech[]; fetchLog: string[]
  publicEvents: Record<string, unknown>[]; states: Record<string, unknown>
  ready: boolean; switchGame?: (game: string | null) => void
  updateGame?: (game: string, state: unknown) => void
}
declare global { interface Window { __stateRepro: Repro } }
const fixture: Repro = { animationLog: [], speechLog: [], fetchLog: [], publicEvents: [], states: {}, ready: false }
fixture.states['1'] = { ok: true, state: { game_id: 1, party_no: 1, round: 1, phase: 'action', factions: [{ idx: 0, name: 'Game A actor' }], recent_actions: [], recent_actions_range: { first_seq: null, last_seq: null, retained_first_seq: 1, limit: 12 } } }
window.__stateRepro = fixture
const record = 'a'.repeat(64)
// Install before importing StreamPage: createLiveApi captures fetch at import.
window.fetch = async (input) => {
  const url = new URL(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url, location.origin)
  fixture.fetchLog.push(url.pathname + url.search)
  if (url.origin !== location.origin) throw new Error('External request blocked by test fixture')
  let body: unknown
  if (url.pathname === `/agents/${record}`) body = { ok: true, agent_record_id: record, registered: true, active_slots: [{ game_id: 2, faction_idx: 0, phase: 'market', round: 1 }] }
  else if (url.pathname === `/agents/${record}/live/events`) {
    const after = Number(url.searchParams.get('after'))
    body = { ok: true, events: fixture.publicEvents.filter(e => Number(e.seq) > after), next_cursor: fixture.publicEvents.at(-1)?.seq ?? 0, has_more: false, history_truncated: false, server_now: Math.floor(Date.now() / 1000), presence: 'connected' }
  } else if (/^\/game\/[12]\/state$/.test(url.pathname)) body = fixture.states[url.pathname.split('/')[2]]
  else throw new Error(`Unexpected fetch in isolated fixture: ${url.pathname}`)
  return new Response(JSON.stringify(body), { headers: { 'content-type': 'application/json' } })
}
const query = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } })
export function FeedHarness() {
  const [game, setGame] = useState<string | null>('1')
  useEffect(() => {
    fixture.switchGame = next => flushSync(() => setGame(next))
    fixture.updateGame = (id, state) => {
      fixture.states[id] = { ok: true, state }
      flushSync(() => query.setQueryData(['arena-state', '', id], fixture.states[id]))
    }
    return () => { delete fixture.switchGame; delete fixture.updateGame }
  }, [])
  const feed = useArenaFeed(game, false)
  return <output id="feed-state" data-game={game ?? 'off'} data-mode={feed.mode} data-loaded={!!feed.state} data-seq={feed.state?.recent_actions_range.last_seq ?? 0}>Game {game ?? 'off'}</output>
}
useScene.subscribe(state => fixture.speechLog.push({ speech: state.speech, clip: state.clip, take: state.take, speechId: state.speechId }))
useScene.setState({ clip: 'idle', take: 0, speech: '', speechId: 0 })
const root = createRoot(document.getElementById('root')!)
const mode = new URLSearchParams(location.search).get('repro')
if (mode === '6') {
  const { default: StreamPage } = await import('../../src/live/StreamPage')
  flushSync(() => root.render(<StreamPage />))
} else {
  // Controlled query cache injection exercises the hook's effects without
  // replacing useQuery or copying its cursor/queue implementation.
  flushSync(() => root.render(<QueryClientProvider client={query}><FeedHarness /></QueryClientProvider>))
}
fixture.ready = true

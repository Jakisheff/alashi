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
  completeRenderers: boolean; finishCallbacks: (() => void)[]
  ready: boolean; switchGame?: (game: string | null) => void
  switchSource?: (scope: { game: string | null; api?: string; sample?: boolean }) => void
  updateGame?: (game: string, state: unknown, api?: string) => void
  copySource?: (from: string, to: string, game: string) => void
  scene: () => Speech; manual: (speech: string) => void; unmount?: () => void
  feedEffects?: { mode: string; seq: number; game: string | null; api: string }
}
declare global { interface Window { __stateRepro: Repro } }
const scene = () => {
  const { speech, clip, take, speechId } = useScene.getState()
  return { speech, clip, take, speechId }
}
const fixture: Repro = {
  animationLog: [], speechLog: [], fetchLog: [], publicEvents: [], states: {}, ready: false,
  completeRenderers: true, finishCallbacks: [], scene,
  manual: speech => { useScene.getState().say(speech); useScene.getState().play('fuckOff') },
}
fixture.states['|1'] = { ok: true, state: { game_id: 1, party_no: 1, round: 1, phase: 'action', factions: [{ idx: 0, name: 'Game A actor' }], recent_actions: [], recent_actions_range: { first_seq: null, last_seq: null, retained_first_seq: 1, limit: 12 } } }
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
  } else {
    const match = /^(.*)\/game\/([12])\/state$/.exec(url.pathname)
    if (!match || !(match[1] + '|' + match[2] in fixture.states)) throw new Error(`Unexpected fetch in isolated fixture: ${url.pathname}`)
    body = fixture.states[match[1] + '|' + match[2]]
  }
  return new Response(JSON.stringify(body), { headers: { 'content-type': 'application/json' } })
}
const query = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } })
export function FeedHarness() {
  const [scope, setScope] = useState({ game: '1' as string | null, api: '', sample: false })
  const { game, api, sample } = scope
  useEffect(() => {
    fixture.switchGame = game => flushSync(() => setScope(previous => ({ ...previous, game })))
    fixture.switchSource = next => flushSync(() => setScope({ api: '', sample: false, ...next }))
    fixture.updateGame = (id, state, api = '') => {
      const key = api + '|' + id
      fixture.states[key] = { ok: true, state }
      flushSync(() => query.setQueryData(['arena-state', api, id], fixture.states[key]))
    }
    fixture.copySource = (from, to, id) => {
      fixture.states[to + '|' + id] = query.getQueryData(['arena-state', from, id])
      flushSync(() => query.setQueryData(['arena-state', to, id], fixture.states[to + '|' + id]))
    }
    return () => { delete fixture.switchGame; delete fixture.switchSource; delete fixture.updateGame; delete fixture.copySource }
  }, [])
  const feed = useArenaFeed(game, sample, api)
  useEffect(() => {
    fixture.feedEffects = { mode: feed.mode, seq: feed.state?.recent_actions_range.last_seq ?? 0, game, api }
  }, [feed.mode, feed.state, game, api])
  return <output id="feed-state" data-game={game ?? 'off'} data-api={api} data-mode={feed.mode} data-loaded={!!feed.state} data-seq={feed.state?.recent_actions_range.last_seq ?? 0}>Game {game ?? 'off'}</output>
}
useScene.subscribe(state => fixture.speechLog.push({ speech: state.speech, clip: state.clip, take: state.take, speechId: state.speechId }))
useScene.setState({ clip: 'idle', take: 0, speech: '', speechId: 0 })
const root = createRoot(document.getElementById('root')!)
fixture.unmount = () => flushSync(() => root.unmount())
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

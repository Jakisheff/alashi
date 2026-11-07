import { useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { flushSync } from 'react-dom'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import Scene from '../../src/genie/Scene'
import { StudioStage } from '../../src/studio/StudioStage'
import { useArenaFeed } from '../../src/feed'
import { useScene } from '../../src/store'

type Snapshot = { game_id: number; party_no: number; round: number; phase: string; factions: { idx: number; name: string }[]; recent_actions: Record<string, unknown>[]; recent_actions_range: { first_seq: number | null; last_seq: number | null; retained_first_seq: number; limit: number } }
const snapshot = (source: string): Snapshot => ({ game_id: 1, party_no: 1, round: 1, phase: 'action', factions: [{ idx: 0, name: `Source ${source} actor` }], recent_actions: [], recent_actions_range: { first_seq: null, last_seq: null, retained_first_seq: 1, limit: 12 } })
const fixture = {
  ready: false,
  publicEvents: [] as Record<string, unknown>[],
  states: { '': snapshot('A'), '/source-b': snapshot('B') } as Record<string, Snapshot>,
  sceneLog: [] as { at: number; clip: string; speech: string; take: number }[],
  switchSource: (_source: string) => {},
  updateSource: (_source: string, _actions: Record<string, unknown>[]) => {},
}
declare global { interface Window { __visualState: typeof fixture } }
window.__visualState = fixture
const record = 'a'.repeat(64)
const originalFetch = window.fetch.bind(window)
window.fetch = async (input, init) => {
  const url = new URL(typeof input === 'string' ? input : input instanceof URL ? input.href : input.url, location.origin)
  if (url.origin !== location.origin) throw new Error('External request blocked by visual fixture')
  if (url.protocol === 'blob:') return originalFetch(input, init)
  let body: unknown
  if (url.pathname === `/agents/${record}`) body = { ok: true, agent_record_id: record, registered: true, active_slots: [{ game_id: 2, faction_idx: 0, phase: 'market', round: 1 }] }
  else if (url.pathname === `/agents/${record}/live/events`) {
    const after = Number(url.searchParams.get('after'))
    body = { ok: true, events: fixture.publicEvents.filter(e => Number(e.seq) > after), next_cursor: fixture.publicEvents.at(-1)?.seq ?? 0, has_more: false, history_truncated: false, server_now: Math.floor(Date.now() / 1000), presence: 'connected' }
  } else if (url.pathname.endsWith('/game/1/state')) body = { ok: true, state: fixture.states[url.pathname.slice(0, -'/game/1/state'.length)] }
  else if (/^\/(models|fonts|scene)\//.test(url.pathname)) return originalFetch(input, init)
  else throw new Error(`Unexpected visual fixture fetch ${url.pathname}`)
  return new Response(JSON.stringify(body), { headers: { 'content-type': 'application/json' } })
}
const query = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } })
useScene.subscribe(state => fixture.sceneLog.push({ at: performance.now(), clip: state.clip, speech: state.speech, take: state.take }))
function LegacyViewer() {
  const [api, setApi] = useState('')
  const feed = useArenaFeed('1', false, api)
  const clip = useScene(s => s.clip)
  const speech = useScene(s => s.speech)
  useEffect(() => {
    fixture.switchSource = source => flushSync(() => setApi(source))
    fixture.updateSource = (source, actions) => {
      const previous = fixture.states[source]
      const next = { ...previous, recent_actions: actions, recent_actions_range: { first_seq: actions.length ? Number(actions[0].seq) : null, last_seq: actions.length ? Number(actions.at(-1)?.seq) : null, retained_first_seq: 1, limit: 12 } }
      fixture.states[source] = next
      flushSync(() => query.setQueryData(['arena-state', source, '1'], { ok: true, state: next }))
    }
    fixture.ready = true
  }, [])
  return <main className="visual-legacy"><h1>Synthetic local viewer fixture</h1><p id="visual-status" data-source={api || 'A'} data-clip={clip} data-loaded={!!feed.state}>Source {api ? 'B' : 'A'} / game 1 / {clip}</p><div className="visual-stage"><StudioStage><Scene frozen={null} background={null} studio interactive={false} /></StudioStage></div><p id="visual-speech">{speech || 'No queued speech'}</p></main>
}
const root = createRoot(document.getElementById('root')!)
const mode = new URLSearchParams(location.search).get('visual')
if (mode === 'stream') {
  const { default: StreamPage } = await import('../../src/live/StreamPage')
  flushSync(() => root.render(<StreamPage />))
  fixture.ready = true
} else flushSync(() => root.render(<QueryClientProvider client={query}><LegacyViewer /></QueryClientProvider>))

import { useQuery } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { advance, eventText, resultText, type ArenaEvent, type ArenaState, type Cursor, type GameResult } from './events'
import { useScene } from './store'

// A game id (?game=<id>, or the watched agent's game) reads the arena's public GET /game/:id/state,
// same-origin unless ?api=<base> (no /api prefix per Ivan). Without one the page plays a clearly labelled
// sample stream so the scene is never silently fake.
const q = new URLSearchParams(location.search)
const API = q.get('api') ?? ''
export const GAME_PARAM = q.get('game')

const EVENT_SECONDS = 3.2 // one reaction at a time: act, then accepted/rejected
const QUEUE_MAX = 6 // a long backlog is trimmed to the latest events instead of replayed

const SAMPLE_ACTIONS = ['produce', 'sell', 'vote', 'bribe', 'buy', 'donkey', 'produce', 'veto']

function useSampleState(enabled: boolean) {
  const [state, setState] = useState<ArenaState | null>(null)
  useEffect(() => {
    if (!enabled) return
    let seq = 0
    const log: ArenaEvent[] = []
    const tick = () => {
      seq += 1
      log.push({
        seq, event_id: `0:0:${seq}`, round: 1 + Math.floor(seq / 6), phase: seq % 6 === 5 ? 'law' : 'action',
        actor: seq % 2, action: SAMPLE_ACTIONS[seq % SAMPLE_ACTIONS.length], by: 'sample', ok: seq % 7 !== 0, ts: seq,
      })
      const window = log.slice(-12)
      setState({
        game_id: 0, party_no: 0, round: window.at(-1)!.round, phase: window.at(-1)!.phase,
        factions: [{ idx: 0, name: 'Aitore' }, { idx: 1, name: 'Aikorkem' }],
        recent_actions: window,
        recent_actions_range: { first_seq: window[0].seq, last_seq: seq, retained_first_seq: 1, limit: 12 },
      })
    }
    tick()
    const id = setInterval(tick, 4000)
    return () => clearInterval(id)
  }, [enabled])
  return state
}

/** Feeds arena events to Degenie: types each line on its screen and plays act -> accepted/rejected. */
// `sample: false` (an owner watching their agent) keeps Degenie idle instead of playing invented trades.
export function useArenaFeed(game: string | null, sample = true) {
  const mode: 'live' | 'sample' | 'off' = game ? 'live' : sample ? 'sample' : 'off'
  const live = useQuery({
    queryKey: ['arena-state', API, game],
    enabled: mode === 'live',
    // Paused while the tab is hidden (react-query default); stops for good once the game is finished.
    refetchInterval: (query) => (query.state.data?.finished ? false : 2000),
    queryFn: async () => {
      const res = await fetch(`${API}/game/${game}/state`)
      if (!res.ok) throw new Error(`state ${res.status}`)
      return (await res.json()) as { state?: ArenaState; finished?: boolean; result?: GameResult }
    },
  })
  const sampleState = useSampleState(mode === 'sample')
  const state = mode === 'live' ? (live.data?.state ?? null) : mode === 'sample' ? sampleState : null
  const result = (mode === 'live' && live.data?.finished && live.data.result) || null

  const cursor = useRef<Cursor>(null)
  const queue = useRef<{ text: string; ok: boolean }[]>([])
  useEffect(() => {
    if (!state) return
    const u = advance(cursor.current, state)
    cursor.current = u.cursor
    if (u.gap) queue.current.push({ text: '… some events were skipped', ok: true })
    for (const e of u.fresh) queue.current.push({ text: eventText(e, state.factions), ok: e.ok })
    if (queue.current.length > QUEUE_MAX) queue.current.splice(0, queue.current.length - QUEUE_MAX)
  }, [state])

  // The final line goes after any events still queued.
  useEffect(() => {
    if (result) queue.current.push({ text: resultText(result), ok: true })
  }, [result])

  useEffect(() => {
    const timers: ReturnType<typeof setTimeout>[] = []
    const id = setInterval(() => {
      const next = queue.current.shift()
      if (!next) return
      const { say, play } = useScene.getState()
      say(next.text)
      play('act')
      timers.push(setTimeout(() => useScene.getState().play(next.ok ? 'accepted' : 'rejected'), 1100))
    }, EVENT_SECONDS * 1000)
    return () => {
      clearInterval(id)
      timers.forEach(clearTimeout)
    }
  }, [])

  return { state, result, error: mode === 'live' ? live.error : null, mode }
}

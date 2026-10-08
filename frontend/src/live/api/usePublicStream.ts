import { useEffect, useRef, useState } from 'react'
import { LiveApiError, friendlyError, mergePublic, type LiveApi, type PublicEvent, type PublicPage } from './client'

export type StreamState = { events: PublicEvent[]; connection: 'connecting' | 'connected' | 'reconnecting'; presence: 'unknown' | 'connected' | 'offline'; gap: boolean; error: string; serverOffset: number; ready: boolean }
const initial: StreamState = { events: [], connection: 'connecting', presence: 'unknown', gap: false, error: '', serverOffset: 0, ready: false }
export function usePublicStream(api: LiveApi, record: string, onNewEvent?: (event: PublicEvent) => void) {
  const [state, setState] = useState<StreamState>(initial)
  const [retry, setRetry] = useState(0)
  const eventCallback = useRef(onNewEvent)
  useEffect(() => { eventCallback.current = onNewEvent }, [onNewEvent])
  const cursor = useRef(0)
  const retained = useRef<PublicEvent[]>([])
  const selected = useRef('')
  useEffect(() => {
    const controller = new AbortController()
    let timer = 0, failures = 0, stopped = false, bootstrapped = false
    if (selected.current !== record) { selected.current = record; cursor.current = 0; retained.current = []; setState(initial) }
    async function poll() {
      let delay = 1800
      try {
        let page: PublicPage
        try { page = await api.public(record, cursor.current, controller.signal) }
        catch (error) {
          if (!(error instanceof LiveApiError) || !['cursor_expired', 'cursor_ahead'].includes(error.code)) throw error
          // Mark the history gap explicitly, then read the retained server journal.
          cursor.current = 0; retained.current = []; bootstrapped = false
          setState((s) => ({ ...s, events: [], gap: true }))
          page = await api.public(record, 0, controller.signal)
        }
        if (stopped) return
        if (page.cursor < cursor.current) throw new LiveApiError('invalid_response')
        const newEvents = page.events.filter((e) => e.seq > cursor.current)
        cursor.current = page.cursor
        retained.current = mergePublic(retained.current, page.events)
        setState((s) => ({ ...s, events: retained.current, connection: 'connected', presence: page.presence, gap: s.gap || page.truncated, error: '', serverOffset: page.serverNow * 1000 - Date.now(), ready: !page.hasMore }))
        // Do not replay old speech on first load/backfill. Silence is a valid state.
        if (bootstrapped) for (const event of newEvents) eventCallback.current?.(event)
        if (!page.hasMore) bootstrapped = true
        failures = 0; delay = page.hasMore ? 20 : 1800
      } catch (error) {
        if (stopped) return
        failures++
        setState((s) => ({ ...s, connection: 'reconnecting', error: friendlyError(error) }))
        delay = Math.min(30_000, 1500 * 2 ** Math.min(failures, 5))
      }
      if (!stopped) timer = window.setTimeout(poll, delay)
    }
    void poll()
    return () => { stopped = true; controller.abort(); window.clearTimeout(timer) }
  }, [api, record, retry])
  return { ...state, reconnect: () => setRetry((n) => n + 1) }
}

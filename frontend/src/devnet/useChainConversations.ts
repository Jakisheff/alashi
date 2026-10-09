import { useEffect, useRef, useState } from 'react'
import { ChainApiError } from './client'
import { conversationsPage, type PublicConversationEntry } from './conversationsClient'

type Journal = { game: string; entries: PublicConversationEntry[]; cursor: string; historyComplete: boolean; loaded: boolean }
export function useChainConversations(game: string, enabled: boolean, terminal: boolean) {
  const initial: Journal = { game, entries: [], cursor: '0', historyComplete: false, loaded: false }
  const cache = useRef<Journal>(initial)
  const [journal, setJournal] = useState(initial)
  const [error, setError] = useState('')
  const [revision, setRevision] = useState(0)
  const inFlight = useRef(false), retryAt = useRef(0), terminalRef = useRef(terminal)
  useEffect(() => { terminalRef.current = terminal }, [terminal])
  useEffect(() => {
    if (!enabled) return
    if (cache.current.game !== game) cache.current = { game, entries: [], cursor: '0', historyComplete: false, loaded: false }
    let stopped = false, timer = 0, final = false, failures = 0
    const controller = new AbortController()
    const schedule = () => {
      window.clearTimeout(timer)
      if (!stopped && !final && !document.hidden) timer = window.setTimeout(() => void poll(), Math.max(0, retryAt.current - Date.now()))
    }
    const poll = async () => {
      if (stopped || final || document.hidden) return
      if (inFlight.current) { timer = window.setTimeout(() => void poll(), 250); return }
      if (Date.now() < retryAt.current) { schedule(); return }
      inFlight.current = true
      try {
        const current = cache.current, page = await conversationsPage(game, current.cursor, controller.signal)
        if (stopped) return
        const ids = new Set(current.entries.map((e) => e.entry_id))
        if (page.entries.some((e) => ids.has(e.entry_id)) || current.entries.length + page.entries.length > 10_000) throw new ChainApiError('invalid')
        const next = { game, entries: [...current.entries, ...page.entries], cursor: page.next_cursor, historyComplete: page.history_complete, loaded: true }
        cache.current = next; setJournal(next); setError(''); failures = 0
        final = terminalRef.current && !page.has_more
        retryAt.current = Date.now() + (page.has_more ? 0 : 12_000)
      } catch (problem) {
        if (stopped) return
        setError(problem instanceof ChainApiError && problem.code === 'not_found' ? 'Public negotiations are not available for this game yet.'
          : problem instanceof ChainApiError && problem.code === 'invalid' ? 'Public journal validation failed. Keeping the last received records.'
            : problem instanceof ChainApiError && problem.code === 'rate_limit' ? 'Public journal is busy. Retrying after a pause.' : 'Public journal connection interrupted.')
        failures++
        retryAt.current = Date.now() + Math.max(12_000 * 2 ** Math.min(failures - 1, 4), problem instanceof ChainApiError ? problem.retryMs : 0)
      } finally { inFlight.current = false; schedule() }
    }
    const visible = () => { if (document.hidden) window.clearTimeout(timer); else schedule() }
    document.addEventListener('visibilitychange', visible)
    window.addEventListener('pageshow', visible)
    void poll()
    return () => { stopped = true; controller.abort(); window.clearTimeout(timer); document.removeEventListener('visibilitychange', visible); window.removeEventListener('pageshow', visible) }
  }, [game, enabled, revision])
  return { entries: journal.game === game ? journal.entries : [], historyComplete: journal.game === game && journal.historyComplete,
    connection: error ? (journal.loaded ? 'reconnecting' as const : 'unavailable' as const) : journal.loaded ? 'connected' as const : 'loading' as const,
    error, retry: () => setRevision((n) => n + 1) }
}

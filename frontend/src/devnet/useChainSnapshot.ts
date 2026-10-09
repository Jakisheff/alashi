import { useEffect, useRef, useState } from 'react'
import { chainSnapshot, chainFailure, ChainApiError, type ChainSnapshot } from './client'
export function useChainSnapshot(pda: string) {
  const [snapshot, setSnapshot] = useState<ChainSnapshot | null>(null)
  const [error, setError] = useState('')
  const [waiting, setWaiting] = useState(true)
  const [revision, setRevision] = useState(0)
  const retryAt = useRef(0), requestInFlight = useRef(false)
  useEffect(() => {
    let stopped = false, timer = 0, failures = 0, final = false
    const controller = new AbortController()
    const schedule = () => {
      window.clearTimeout(timer)
      if (!stopped && !final && !document.hidden) timer = window.setTimeout(() => void poll(), Math.max(0, retryAt.current - Date.now()))
    }
    const poll = async () => {
      if (stopped || final || document.hidden) return
      if (requestInFlight.current) { timer = window.setTimeout(() => void poll(), 250); return }
      if (Date.now() < retryAt.current) { schedule(); return }
      requestInFlight.current = true
      try {
        const next = await chainSnapshot(pda, controller.signal)
        if (stopped) return
        setSnapshot(next); setError(''); setWaiting(false); failures = 0
        final = next.settled && next.complete
        retryAt.current = Date.now() + 12_000
      } catch (problem) {
        if (stopped) return
        setError(chainFailure(problem)); setWaiting(false); failures++
        retryAt.current = Date.now() + Math.max(12_000 * 2 ** Math.min(failures - 1, 4), problem instanceof ChainApiError ? problem.retryMs : 0) + Math.floor(Math.random() * 1000)
      } finally { requestInFlight.current = false; schedule() }
    }
    const visible = () => { if (document.hidden) window.clearTimeout(timer); else schedule() }
    document.addEventListener('visibilitychange', visible); window.addEventListener('pageshow', visible)
    void poll()
    return () => { stopped = true; controller.abort(); window.clearTimeout(timer); document.removeEventListener('visibilitychange', visible); window.removeEventListener('pageshow', visible) }
  }, [pda, revision])
  return { snapshot, error, waiting, retry: () => setRevision((n) => n + 1) }
}

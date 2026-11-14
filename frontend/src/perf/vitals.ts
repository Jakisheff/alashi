// Opt-in vitals report for the spectator pages: add ?perf=1 and every
// metric plus attribution lands in the console. No network, no storage.
import { onCLS, onINP, onLCP, type Metric } from 'web-vitals/attribution'

let started = false

function line(metric: Metric) {
  const a = (metric as unknown as { attribution?: Record<string, unknown> }).attribution ?? {}
  const target = a.largestShiftTarget ?? a.eventTarget ?? a.element ?? ''
  console.info(
    `[perf] ${metric.name}=${Math.round(metric.value)}ms rating=${metric.rating}` +
      (target ? ` target=${String(target).slice(0, 80)}` : ''),
  )
}

/** Starts console vitals reporting once per page. */
export function startVitals(): () => void {
  if (started || typeof window === 'undefined') return () => {}
  started = true
  onCLS(line)
  onINP(line)
  onLCP(line)
  if ('PerformanceObserver' in window) {
    let frames = 0, total = 0, worst = 0
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        frames += 1
        total += entry.duration
        worst = Math.max(worst, entry.duration)
      }
    })
    observer.observe({ type: 'long-animation-frame', buffered: true })
    window.setInterval(() => {
      if (frames > 0) console.info(`[perf] loaf count=${frames} total=${Math.round(total)}ms worst=${Math.round(worst)}ms`)
    }, 15000)
  }
  return () => {}
}

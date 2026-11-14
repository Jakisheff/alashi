// Performance trace for the /live demo: one scripted session (visible or
// hidden), CPU throttled, with CDP Performance metrics, long animation
// frames, LCP and interaction samples. Usage:
//   node scripts/perf-trace-check.mjs <visible|hidden> [seconds] [port]
// Prints one JSON line. Serve the build with vite preview on the port.
import { spawn } from 'node:child_process'
const { chromium } = await import(process.env.ALASHI_PLAYWRIGHT_MODULE ?? 'playwright')

const mode = process.argv[2] === 'hidden' ? 'hidden' : 'visible'
const seconds = Number(process.argv[3] ?? 60)
const port = Number(process.argv[4] ?? 4173)
const preview = spawn('npx', ['vite', 'preview', '--port', String(port), '--strictPort'], { cwd: process.cwd(), stdio: 'ignore' })
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

try {
  const origin = `http://localhost:${port}`
  for (let i = 0; i < 60; i++) {
    await sleep(500)
    try { await fetch(`${origin}/`); break } catch { if (i === 59) throw new Error('preview server did not start') }
  }
  const browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--enable-unsafe-swiftshader'] })
  const page = await browser.newPage({ viewport: { width: 1366, height: 1000 } })
  const client = await page.context().newCDPSession(page)
  await client.send('Performance.enable')
  await client.send('Emulation.setCPUThrottlingRate', { rate: 4 })
  await page.goto(`${origin}/live?perf=1`, { waitUntil: 'load' })

  await page.evaluate(() => {
    window.__perf = { loaf: 0, loafTotal: 0, loafWorst: 0, interactions: 0, worstInteraction: 0, lcp: 0 }
    new PerformanceObserver((l) => { for (const e of l.getEntries()) { window.__perf.loaf++; window.__perf.loafTotal += e.duration; window.__perf.loafWorst = Math.max(window.__perf.loafWorst, e.duration) } }).observe({ type: 'long-animation-frame', buffered: true })
    new PerformanceObserver((l) => { for (const e of l.getEntries()) { window.__perf.interactions++; window.__perf.worstInteraction = Math.max(window.__perf.worstInteraction, e.duration) } }).observe({ type: 'event', durationThreshold: 16, buffered: true })
    new PerformanceObserver((l) => { const es = l.getEntries(); if (es.length) window.__perf.lcp = es[es.length - 1].startTime }).observe({ type: 'largest-contentful-paint', buffered: true })
  })

  if (mode === 'hidden') {
    await page.evaluate(() => {
      Object.defineProperty(document, 'hidden', { configurable: true, get: () => true })
      document.dispatchEvent(new Event('visibilitychange'))
    })
  }
  const clicks = mode === 'visible' ? 5 : 0
  for (let i = 0; i < seconds * 1000; i += Math.floor(seconds * 1000 / (clicks || 1))) {
    await sleep(clicks ? Math.floor(seconds * 1000 / clicks) : seconds * 1000)
    if (clicks) await page.getByLabel(/Pause demo|Resume demo/).click().catch(() => {})
  }
  if (mode === 'hidden') {
    // Interactions still land while "hidden" to prove input handling is idle.
    await page.getByLabel(/Pause demo|Resume demo/).click().catch(() => {})
  }

  const counters = await page.evaluate(() => window.__perf)
  const { metrics } = await client.send('Performance.getMetrics')
  const pick = (name) => metrics.find((m) => m.name === name)?.value ?? 0
  const report = {
    mode, seconds, throttle: 4,
    lcp_ms: Math.round(counters.lcp),
    inp_worst_interaction_ms: Math.round(counters.worstInteraction),
    interactions: counters.interactions,
    loaf_count: counters.loaf, loaf_total_ms: Math.round(counters.loafTotal), loaf_worst_ms: Math.round(counters.loafWorst),
    cdp_script_duration_s: +pick('ScriptDuration').toFixed(2),
    cdp_task_duration_s: +pick('TaskDuration').toFixed(2),
    cdp_layout_duration_s: +pick('LayoutDuration').toFixed(3),
    cdp_layout_count: pick('LayoutCount'),
    cdp_recalc_style_count: pick('RecalcStyleCount'),
  }
  console.log(JSON.stringify(report))
  await browser.close()
} finally {
  preview.kill('SIGTERM')
}

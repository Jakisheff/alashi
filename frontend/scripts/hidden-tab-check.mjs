// Hidden-tab behavior check for the /live demo: with the document.hidden
// guards the demo clock and feed must freeze while the tab reports
// hidden and must resume without losing anything when it returns. This
// test can fail: without the guards the clock text keeps changing while
// hidden. Requires a built frontend served by `vite preview`.
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
const { chromium } = await import(process.env.ALASHI_PLAYWRIGHT_MODULE ?? 'playwright')

const port = Number(process.env.HIDDEN_CHECK_PORT ?? 4173)
const preview = spawn('npx', ['vite', 'preview', '--port', String(port), '--strictPort'], { cwd: process.cwd(), stdio: 'ignore' })
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

try {
  let origin = `http://localhost:${port}`
  for (let i = 0; i < 60; i++) {
    await sleep(500)
    try { await fetch(`${origin}/`); break } catch { if (i === 59) throw new Error('preview server did not start') }
  }
  const browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--enable-unsafe-swiftshader'] })
  const page = await browser.newPage({ viewport: { width: 1366, height: 1000 } })
  const errors = []
  page.on('pageerror', (e) => errors.push(String(e)))
  await page.goto(`${origin}/live`, { waitUntil: 'load', timeout: 60000 })
  await page.waitForSelector('.live-feed article', { timeout: 60000 })

  // Readiness: the `before` snapshot must be taken only after the demo
  // clock has ticked at least once. A fixed sleep races the page startup:
  // if the guards finish initializing after `before` is captured but before
  // the tab is hidden, the clock keeps ticking in the hidden phase and the
  // freeze assertion fails without any product change (observed as three
  // consecutive CI failures on unchanged frontend code, 14-15 Nov 2026).
  const clockOf = () => document.querySelector('.live-playback-meta span')?.textContent ?? ''
  let firstClock = await page.evaluate(clockOf)
  let ticks = false
  for (let i = 0; i < 60 && !ticks; i++) {
    await sleep(500)
    const c = await page.evaluate(clockOf)
    if (c !== firstClock && c !== '') ticks = true; else firstClock = c
  }
  if (!ticks) throw new Error('demo clock never ticked before the check started')

  const before = await page.evaluate(() => ({
    clock: document.querySelector('.live-playback-meta span')?.textContent ?? '',
    events: document.querySelectorAll('.live-feed article').length,
  }))
  await page.evaluate(() => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => true })
    document.dispatchEvent(new Event('visibilitychange'))
  })
  // Verify the override actually took effect; some headless Chrome builds
  // on Linux reset document.hidden between evaluate calls.
  const overrideWorks = await page.evaluate(() => document.hidden === true)
  if (!overrideWorks) {
    console.log('SKIP: document.hidden override not supported in this environment (headless Linux CI)')
    await browser.close()
  } else {
  await sleep(9000)
  const hidden = await page.evaluate(() => ({
    clock: document.querySelector('.live-playback-meta span')?.textContent ?? '',
    events: document.querySelectorAll('.live-feed article').length,
  }))
  await page.evaluate(() => {
    Object.defineProperty(document, 'hidden', { configurable: true, get: () => false })
    document.dispatchEvent(new Event('visibilitychange'))
  })
  await sleep(4000)
  const resumed = await page.evaluate(() => ({
    clock: document.querySelector('.live-playback-meta span')?.textContent ?? '',
    events: document.querySelectorAll('.live-feed article').length,
  }))

  assert.equal(hidden.clock, before.clock, 'clock must freeze while hidden (guards failed?)')
  assert.equal(hidden.events, before.events, 'feed must not advance while hidden (guards failed?)')
  assert.notEqual(resumed.clock, hidden.clock, 'clock must resume after returning visible')
  assert.ok(resumed.events >= hidden.events, 'feed must not lose events after resuming')
  assert.deepEqual(errors, [], 'no page errors during hide and resume')

  console.log('PASS: hidden tab froze the demo clock and feed; resume continued without loss')
  console.log(`     clock "${before.clock}" -> "${hidden.clock}" (frozen) -> "${resumed.clock}" (resumed)`)
  console.log(`     feed ${before.events} -> ${hidden.events} (frozen) -> ${resumed.events} (resumed)`)
  }
  await browser.close()
} finally {
  preview.kill('SIGTERM')
}

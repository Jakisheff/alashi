// Default verifies the current bugs; --assert-fixed instead checks desired invariants.
// They exercise actual production React lifecycles; no game/backend is contacted.
import assert from 'node:assert/strict'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import { setTimeout as settle } from 'node:timers/promises'
import { createServer } from 'vite'
import react from '@vitejs/plugin-react'

const which = process.argv[2]
const assertFixed = process.argv.includes('--assert-fixed')
if (!['6', '7', 'all'].includes(which)) throw new Error('Usage: node scripts/state-repros/run.mjs <6|7|all> [--assert-fixed]')
const { chromium } = await import(process.env.ALASHI_PLAYWRIGHT_MODULE ?? 'playwright')
const frontend = fileURLToPath(new URL('../../', import.meta.url))
const adapters = fileURLToPath(new URL('./render-boundaries.tsx', import.meta.url))
const intercepted = new Map([
  ['@tanstack/react-router', adapters],
  ['../genie/Scene', adapters],
  ['./market/MarketTrade', adapters],
  ['./actions', adapters],
  ['./reactions', adapters],
  ['./api/OwnerPanel', adapters],
])
const server = await createServer({
  root: frontend, configFile: false, envDir: false,
  server: { host: '127.0.0.1', port: 0, strictPort: true, hmr: false },
  optimizeDeps: { noDiscovery: true, include: ['react', 'react-dom/client', 'react-dom', '@tanstack/react-query', 'zustand'] },
  define: { 'import.meta.env.VITE_LIVE_API_ENABLED': '"true"', 'import.meta.env.VITE_LIVE_API_PATH': '""' },
  plugins: [{
    name: 'state-repro-render-boundaries', enforce: 'pre',
    resolveId(source, importer) {
      if (importer?.split('?')[0] === path.join(frontend, 'src/live/StreamPage.tsx')) return intercepted.get(source)
    },
  }, react()],
})
let browser
const failures = []
const record = 'a'.repeat(64)
const event = (seq, action) => ({ seq, event_id: `action-${seq}`, visibility: 'public', kind: 'game_action', room_id: 'game:2', game_id: 2, round: 1, phase: 'market', server_created_at: '2026-11-07T00:00:00Z', branch_id: 'MAIN', finality: 'final', action_ref: { seq, actor: 0, action } })
function snapshot(id, events = []) {
  return { game_id: Number(id), party_no: 1, round: 1, phase: 'action', factions: [{ idx: 0, name: id === '1' ? 'Game A actor' : 'Game B actor' }], recent_actions: events, recent_actions_range: { first_seq: events[0]?.seq ?? null, last_seq: events.at(-1)?.seq ?? null, retained_first_seq: 1, limit: 12 } }
}
const arenaEvent = (seq, action) => ({ seq, event_id: `A:${seq}`, round: 1, phase: 'action', actor: 0, action, by: 'test fixture', ok: true, ts: seq })
async function pageFor(finding) {
  const page = await browser.newPage({ viewport: { width: 1366, height: 1000 } })
  page.setDefaultTimeout(10_000)
  page.on('pageerror', error => console.log('BROWSER ERROR:', error.message))
  await page.route('**/*', route => {
    const url = new URL(route.request().url())
    return url.origin === server.resolvedUrls.local[0].replace(/\/$/, '') ? route.continue() : route.abort('blockedbyclient')
  })
  await page.clock.install({ time: new Date('2026-11-07T00:00:00Z') })
  await page.clock.pauseAt(new Date('2026-11-07T00:00:01Z'))
  await page.goto(`${server.resolvedUrls.local[0]}scripts/state-repros/index.html?repro=${finding}${finding === '6' ? `&agent=${record}` : ''}`, { waitUntil: 'domcontentloaded' })
  await until(page, () => window.__stateRepro?.ready)
  return page
}
async function until(page, predicate) {
  for (let attempt = 0; attempt < 200; attempt++) {
    if (await page.evaluate(predicate)) return
    await page.clock.runFor(1)
    await settle(25)
  }
  throw new Error('Harness readiness condition was not reached')
}
async function addPublic(page, entry) {
  await page.evaluate(e => window.__stateRepro.publicEvents.push(e), entry)
  await page.clock.runFor(1800)
}
async function streamControl() {
  const page = await pageFor('6')
  try {
    await until(page, () => document.body.textContent.includes('Feed · connected'))
    await addPublic(page, event(1, 'buy'))
    assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'buy')
    await page.clock.runFor(1000)
    assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'idle')
    await addPublic(page, event(2, 'sell'))
    await page.clock.runFor(1000)
    assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'idle')
    assert.deepEqual(await page.evaluate(() => window.__stateRepro.animationLog.map(e => [e.event, e.action])), [['started', 'buy'], ['finished', 'buy'], ['started', 'sell'], ['finished', 'sell']])
    console.log('CONTROL PASS finding 6: actual stream scheduler processes two actions and returns idle when renderer completes.')
  } finally { await page.close() }
}
async function streamFailure() {
  const page = await pageFor('6')
  try {
    await until(page, () => document.body.textContent.includes('Feed · connected'))
    await addPublic(page, event(1, 'buy'))
    assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'buy', 'precondition: first action has started')
    await page.evaluate(() => window.dispatchEvent(new Event('state-repro-renderer-failure')))
    await until(page, () => document.body.textContent.includes('3D is unavailable. Your stream continues.'))
    await addPublic(page, event(2, 'sell'))
    await page.clock.runFor(20_000)
    assert.equal(await page.locator('[data-event-id="action-2"]').count(), 1, 'the later confirmed action reached the actual public feed')
    const state = await page.evaluate(() => ({ action: document.querySelector('.live-hero')?.getAttribute('data-action'), animationLog: window.__stateRepro.animationLog, receipts: [...document.querySelectorAll('.live-event')].map(e => e.getAttribute('data-event-id')) }))
    console.log('OBSERVED finding 6:', JSON.stringify(state))
    if (assertFixed) assert.equal(state.action, 'idle', 'after renderer failure and 20 s, the completed/abandoned action must not remain active forever')
    else {
      assert.equal(state.action, 'buy', 'reproduction must leave the failed first action active')
      assert.deepEqual(state.animationLog.map(e => [e.event, e.action]), [['started', 'buy']], 'the later accepted sell never starts')
    }
  } finally { await page.close() }
}
async function feedPage() {
  const page = await pageFor('7')
  await page.evaluate(initial => window.__stateRepro.updateGame('1', initial), snapshot('1'))
  await until(page, () => document.querySelector('#feed-state')?.getAttribute('data-loaded') === 'true')
  return page
}
async function feedControl() {
  const page = await feedPage()
  try {
    await page.evaluate(next => window.__stateRepro.updateGame('1', next), snapshot('1', [arenaEvent(1, 'buy')]))
    await page.clock.runFor(4500)
    const spoken = await page.evaluate(() => window.__stateRepro.speechLog.filter(e => e.speech.includes('Game A actor')))
    assert(spoken.length > 0, 'control must observe production hook delivering its queued A event')
    assert(spoken.some(e => e.clip === 'accepted'), 'control must observe delayed production result reaction')
    console.log('CONTROL PASS finding 7: actual useArenaFeed emits queued A speech and its accepted reaction without switching.')
  } finally { await page.close() }
}
async function feedScopeChange(nextGame, duringResult) {
  const page = await feedPage()
  try {
    await page.evaluate(next => window.__stateRepro.updateGame('1', next), snapshot('1', [arenaEvent(1, 'buy'), arenaEvent(2, 'sell')]))
    await page.clock.runFor(1) // Flush React Query notification and production queue effect.
    assert.equal(await page.locator('#feed-state').getAttribute('data-seq'), '2', 'precondition: hook consumed both A events')
    assert.equal(await page.evaluate(() => window.__stateRepro.speechLog.filter(e => e.speech.includes('Game A actor')).length), 0, 'precondition: A events have not spoken yet')
    if (duringResult) {
      await page.clock.runFor(3200)
      assert.equal(await page.evaluate(() => window.__stateRepro.speechLog.at(-1)?.clip), 'act', 'precondition: first A action has started but its result is pending')
      assert.equal(await page.evaluate(() => window.__stateRepro.speechLog.some(e => e.clip === 'accepted')), false, 'precondition: the 1.1 s result timeout has not fired')
    }
    await page.evaluate(({ nextGame, nextState }) => {
      if (nextGame) window.__stateRepro.updateGame(nextGame, nextState)
      window.__stateRepro.switchGame(nextGame)
      window.__stateRepro.speechLog.length = 0
    }, { nextGame, nextState: snapshot('2') })
    assert.equal(await page.locator('#feed-state').getAttribute('data-game'), nextGame ?? 'off')
    assert.equal(await page.locator('#feed-state').getAttribute('data-mode'), nextGame ? 'live' : 'off')
    let delayedResult = []
    if (duringResult) {
      await page.clock.runFor(1100)
      delayedResult = await page.evaluate(() => window.__stateRepro.speechLog.slice())
    }
    await page.clock.runFor(duringResult ? 4500 : 8000)
    const state = await page.evaluate(() => ({ mode: document.querySelector('#feed-state')?.getAttribute('data-mode'), game: document.querySelector('#feed-state')?.getAttribute('data-game'), speechLog: window.__stateRepro.speechLog }))
    console.log(`OBSERVED finding 7 A->${nextGame ? 'B' : 'off'} (${duringResult ? 'result pending + one queued' : 'two queued'}):`, JSON.stringify({ ...state, delayedResult }))
    const oldSpeech = state.speechLog.filter(e => e.speech.includes('Game A actor'))
    if (assertFixed) assert.equal(oldSpeech.length, 0, `switching A->${nextGame ? 'B' : 'off'} must discard A's queued speech and reactions`)
    else {
      assert(oldSpeech.some(e => e.clip === 'act'), 'reproduction must emit queued A work after the switch')
      assert(oldSpeech.some(e => e.clip === 'accepted'), 'reproduction must emit an A result after the switch')
      if (duringResult) assert(delayedResult.some(e => e.clip === 'accepted' && e.speech.includes('Game A actor')), 'pending A result must fire before the next queue tick')
    }
  } finally { await page.close() }
}
async function reproduction(label, fn) {
  try { await fn(); console.log(`${assertFixed ? 'INVARIANT PASS' : 'REPRODUCED'} ${label}`) }
  catch (error) {
    if (!(error instanceof assert.AssertionError)) throw error
    failures.push(label)
    console.error(`FAIL ${label}: ${error.message}`)
    console.error(`expected=${JSON.stringify(error.expected)} actual=${JSON.stringify(error.actual)}`)
  }
}
try {
  await server.listen()
  console.log('Local fixture server ready; launching isolated headless Chrome.')
  browser = await chromium.launch({ channel: 'chrome', headless: true, timeout: 30_000 })
  console.log(`Runtime: Node ${process.version}; Chrome ${browser.version()}.`)
  console.log('Fixture isolation: actual React production modules; mocked renderer/network boundaries; controlled browser clock; no backend or external API.')
  console.log(`Mode: ${assertFixed ? 'assert desired fixed behavior (expected to fail on current source)' : 'verify current buggy behavior (success means reproduced, not fixed)'}.`)
  if (which === '6' || which === 'all') {
    await streamControl()
    await reproduction('finding 6: renderer failure leaves stream action occupied', streamFailure)
  }
  if (which === '7' || which === 'all') {
    await feedControl()
    for (const duringResult of [false, true]) {
      const pending = duringResult ? 'pending result + queue ownership' : 'queue ownership'
      await reproduction(`finding 7: A->B ${pending}`, () => feedScopeChange('2', duringResult))
      await reproduction(`finding 7: live->off ${pending}`, () => feedScopeChange(null, duringResult))
    }
  }
  console.log(`RESULT ${failures.length} failed ${assertFixed ? 'desired invariants' : 'reproduction checks'}. Production implementation was not changed.`)
  if (failures.length) process.exitCode = 1
} finally { await browser?.close(); await server.close() }

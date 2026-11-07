// Default checks recovery; --expect-bug retains the historical reproduction mode.
// They exercise actual production React lifecycles; no game/backend is contacted.
import assert from 'node:assert/strict'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import { readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { setTimeout as settle } from 'node:timers/promises'
import { createServer } from 'vite'
import react from '@vitejs/plugin-react'

const which = process.argv[2]
const assertFixed = !process.argv.includes('--expect-bug')
const caseFilter = process.argv.find(arg => arg.startsWith('--case='))?.slice(7)
let checked = 0
if (!['6', '7', 'all'].includes(which)) throw new Error('Usage: node scripts/state-repros/run.mjs <6|7|all> [--assert-fixed|--expect-bug]')
const { chromium } = await import(process.env.ALASHI_PLAYWRIGHT_MODULE ?? 'playwright')
const frontend = path.resolve(process.env.ALASHI_TEST_FRONTEND_ROOT ?? fileURLToPath(new URL('../../', import.meta.url)))
const sha256 = value => createHash('sha256').update(value).digest('hex')
const sources = new Map(['src/feed.ts', 'src/live/StreamPage.tsx'].map(file => [path.join(frontend, file), sha256(readFileSync(path.join(frontend, file)))]))
const loadedSources = new Set()
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
      if (importer?.split('?')[0].endsWith('/scripts/state-repros/harness.tsx')) {
        if (source === '../../src/feed') return path.join(frontend, 'src/feed.ts')
        if (source === '../../src/live/StreamPage') return path.join(frontend, 'src/live/StreamPage.tsx')
      }
      if (importer?.split('?')[0] === path.join(frontend, 'src/live/StreamPage.tsx')) return intercepted.get(source)
    },
    transform(code, id) {
      const file = id.split('?')[0]
      if (sources.has(file)) {
        assert.equal(sha256(code), sources.get(file), 'browser must exercise the selected production source')
        loadedSources.add(file)
      }
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
    await page.evaluate(() => { window.__stateRepro.completeRenderers = false })
    await addPublic(page, event(1, 'buy'))
    assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'buy', 'precondition: first action has started')
    await addPublic(page, event(2, 'sell'))
    assert.deepEqual(await page.evaluate(() => window.__stateRepro.animationLog.map(e => e.action)), ['buy'], 'precondition: sell is queued behind active buy')
    await page.evaluate(() => window.dispatchEvent(new Event('state-repro-renderer-failure')))
    await until(page, () => document.body.textContent.includes('3D is unavailable. Your stream continues.'))
    if (assertFixed) {
      assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'idle', 'renderer failure immediately releases active work')
      assert.equal(await page.locator('.live-purchase-receipt').count(), 0, 'abandoned receipt overlay is removed')
      await page.evaluate(() => window.__stateRepro.finishCallbacks[0]())
      assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'idle', 'late completion cannot restart the abandoned queue')
    }
    await addPublic(page, event(3, 'vote'))
    await page.clock.runFor(20_000)
    assert.equal(await page.locator('[data-event-id="action-3"]').count(), 1, 'later confirmed action reaches the actual public feed in fallback')
    const state = await page.evaluate(() => ({ action: document.querySelector('.live-hero')?.getAttribute('data-action'), animationLog: window.__stateRepro.animationLog, receipts: [...document.querySelectorAll('.live-event')].map(e => e.getAttribute('data-event-id')) }))
    console.log('OBSERVED finding 6:', JSON.stringify(state))
    if (assertFixed) {
      assert.equal(state.action, 'idle', 'after renderer failure and 20 s, the abandoned action must not remain active')
      assert.deepEqual(state.animationLog.map(e => [e.event, e.action]), [['started', 'buy']], 'fallback must not claim animation completion or start new work')
      assert.deepEqual(state.receipts, ['action-1', 'action-2', 'action-3'], 'confirmed transcript is retained')
      await page.getByRole('button', { name: 'Change agent', exact: true }).click()
      await page.evaluate(() => { window.__stateRepro.publicEvents = []; window.__stateRepro.completeRenderers = true; window.__stateRepro.animationLog = [] })
      await page.locator('#stream-agent').fill(record)
      await page.getByRole('button', { name: 'Open public stream', exact: true }).click()
      await until(page, () => document.body.textContent.includes('Feed · connected'))
      await addPublic(page, event(4, 'sell'))
      await page.clock.runFor(1000)
      assert.deepEqual(await page.evaluate(() => window.__stateRepro.animationLog.map(e => [e.event, e.action])), [['started', 'sell'], ['finished', 'sell']], 'a new stream mount can render normally')
    }
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
async function queuedFeed(page, duringResult) {
  await page.evaluate(next => window.__stateRepro.updateGame('1', next), snapshot('1', [arenaEvent(1, 'buy'), arenaEvent(2, 'sell')]))
  await page.clock.runFor(1)
  assert.equal(await page.locator('#feed-state').getAttribute('data-seq'), '2', 'precondition: hook consumed both A events')
  assert.equal(await page.evaluate(() => window.__stateRepro.speechLog.filter(e => e.speech.includes('Game A actor')).length), 0, 'precondition: A events have not spoken yet')
  if (duringResult) {
    await page.clock.runFor(3200)
    assert.equal(await page.evaluate(() => window.__stateRepro.speechLog.at(-1)?.clip), 'act', 'precondition: A action started but its result is pending')
    assert.equal(await page.evaluate(() => window.__stateRepro.speechLog.some(e => e.clip === 'accepted')), false, 'precondition: result timeout has not fired')
  }
}
async function assertFreshPlayback(page, game, api = '') {
  const seq = Number(await page.locator('#feed-state').getAttribute('data-seq')) + 1
  const next = snapshot(game, [arenaEvent(seq, 'produce')])
  next.factions[0].name = 'New source actor'
  await page.evaluate(({ next, game, api }) => window.__stateRepro.updateGame(game, next, api), { next, game, api })
  await page.clock.runFor(4500)
  assert(await page.evaluate(() => window.__stateRepro.speechLog.some(e => e.speech.includes('New source actor') && e.clip === 'accepted')), 'new source must play its fresh event and result')
}
async function feedScopeChange(nextGame, duringResult) {
  const page = await feedPage()
  try {
    await queuedFeed(page, duringResult)
    await page.evaluate(({ nextGame, nextState }) => {
      if (nextGame) window.__stateRepro.updateGame(nextGame, nextState)
      window.__stateRepro.switchGame(nextGame)
      window.__stateRepro.speechLog.length = 0
    }, { nextGame, nextState: snapshot('2') })
    assert.equal(await page.locator('#feed-state').getAttribute('data-game'), nextGame ?? 'off')
    assert.equal(await page.locator('#feed-state').getAttribute('data-mode'), nextGame ? 'live' : 'off')
    if (assertFixed && duringResult) {
      const scene = await page.evaluate(() => window.__stateRepro.scene())
      assert.equal(scene.speech, '', 'source switch clears its displayed line')
      assert.equal(scene.clip, 'idle', 'source switch releases its current clip')
    }
    let delayedResult = []
    if (duringResult) {
      await page.clock.runFor(1100)
      delayedResult = await page.evaluate(() => window.__stateRepro.speechLog.slice())
    }
    await page.clock.runFor(duringResult ? 4500 : 8000)
    const state = await page.evaluate(() => ({ mode: document.querySelector('#feed-state')?.getAttribute('data-mode'), game: document.querySelector('#feed-state')?.getAttribute('data-game'), speechLog: window.__stateRepro.speechLog }))
    console.log(`OBSERVED finding 7 A->${nextGame ? 'B' : 'off'} (${duringResult ? 'result pending + one queued' : 'two queued'}):`, JSON.stringify({ ...state, delayedResult }))
    const oldSpeech = state.speechLog.filter(e => e.speech.includes('Game A actor'))
    if (assertFixed) {
      assert.equal(oldSpeech.length, 0, `switching A->${nextGame ? 'B' : 'off'} must discard A's queued speech and reactions`)
      if (nextGame) await assertFreshPlayback(page, nextGame)
    }
    else {
      assert(oldSpeech.some(e => e.clip === 'act'), 'reproduction must emit queued A work after the switch')
      assert(oldSpeech.some(e => e.clip === 'accepted'), 'reproduction must emit an A result after the switch')
      if (duringResult) assert(delayedResult.some(e => e.clip === 'accepted' && e.speech.includes('Game A actor')), 'pending A result must fire before the next queue tick')
    }
  } finally { await page.close() }
}
async function feedApiChange() {
  const page = await feedPage()
  try {
    await queuedFeed(page, true)
    await page.evaluate(() => {
      const fixture = window.__stateRepro
      fixture.copySource('', '/alternate', '1') // Identical cached state object, different source.
      fixture.switchSource({ game: '1', api: '/alternate' })
      fixture.speechLog = []
    })
    assert.equal(await page.locator('#feed-state').getAttribute('data-api'), '/alternate')
    await page.clock.runFor(8000)
    assert.deepEqual(await page.evaluate(() => window.__stateRepro.speechLog), [], 'API switch cancels old queued and delayed work')
    await assertFreshPlayback(page, '1', '/alternate')
  } finally { await page.close() }
}
async function feedDispose(manual) {
  const page = await feedPage()
  try {
    await queuedFeed(page, true)
    const before = await page.evaluate(manual => {
      const fixture = window.__stateRepro
      if (manual) fixture.manual('Manual interaction')
      const scene = fixture.scene()
      fixture.unmount()
      fixture.speechLog = []
      return scene
    }, manual)
    await page.clock.runFor(8000)
    assert.deepEqual(await page.evaluate(() => window.__stateRepro.speechLog), [], 'unmount cancels all future feed work')
    const after = await page.evaluate(() => window.__stateRepro.scene())
    if (manual) assert.deepEqual(after, before, 'cleanup preserves output owned by a manual interaction')
    else { assert.equal(after.speech, ''); assert.equal(after.clip, 'idle') }
  } finally { await page.close() }
}
async function sampleReentry() {
  const page = await feedPage()
  try {
    await page.evaluate(() => window.__stateRepro.switchSource({ game: null, sample: true }))
    await until(page, () => window.__stateRepro.feedEffects?.mode === 'sample' && window.__stateRepro.feedEffects.seq === 1)
    await page.clock.runFor(16000)
    assert(Number(await page.locator('#feed-state').getAttribute('data-seq')) >= 4, 'precondition: original sample advanced')
    await page.evaluate(() => window.__stateRepro.switchSource({ game: null, sample: false }))
    await page.clock.runFor(1)
    await page.evaluate(() => { window.__stateRepro.switchSource({ game: null, sample: true }); window.__stateRepro.speechLog = [] })
    await until(page, () => window.__stateRepro.feedEffects?.mode === 'sample' && window.__stateRepro.feedEffects.seq === 1)
    await page.clock.runFor(4000)
    await until(page, () => window.__stateRepro.feedEffects?.mode === 'sample' && window.__stateRepro.feedEffects.seq === 2)
    await page.clock.runFor(4500)
    console.log('OBSERVED sample re-entry:', JSON.stringify(await page.evaluate(() => ({ seq: document.querySelector('#feed-state')?.getAttribute('data-seq'), speech: window.__stateRepro.speechLog }))))
    assert(await page.evaluate(() => window.__stateRepro.speechLog.some(e => e.clip === 'accepted')), 'restarted sample plays fresh events without waiting for its old cursor')
  } finally { await page.close() }
}
async function reproduction(label, fn) {
  if (caseFilter && !label.includes(caseFilter)) return
  checked++
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
  console.log('Production sources:', JSON.stringify(Object.fromEntries(sources)))
  console.log('Fixture isolation: actual React production modules; mocked renderer/network boundaries; controlled browser clock; no backend or external API.')
  console.log(`Mode: ${assertFixed ? 'assert recovery invariants' : 'verify historical buggy behavior (success means reproduced, not fixed)'}.`)
  if (which === '6' || which === 'all') {
    await streamControl()
    await reproduction('finding 6: renderer failure, queued work, fallback and fresh mount', streamFailure)
  }
  if (which === '7' || which === 'all') {
    await feedControl()
    for (const duringResult of [false, true]) {
      const pending = duringResult ? 'pending result + queue ownership' : 'queue ownership'
      await reproduction(`finding 7: A->B ${pending}`, () => feedScopeChange('2', duringResult))
      await reproduction(`finding 7: live->off ${pending}`, () => feedScopeChange(null, duringResult))
    }
    if (assertFixed) {
      await reproduction('finding 7: same game ID at another API with identical cached data', feedApiChange)
      await reproduction('finding 7: unmount with pending result', () => feedDispose(false))
      await reproduction('finding 7: cleanup preserves manual output', () => feedDispose(true))
      await reproduction('finding 7: sample source re-entry', sampleReentry)
    }
  }
  assert(checked > 0, 'case filter must select at least one check')
  if (which !== '6') assert(loadedSources.has(path.join(frontend, 'src/feed.ts')), 'selected feed source was loaded')
  if (which !== '7') assert(loadedSources.has(path.join(frontend, 'src/live/StreamPage.tsx')), 'selected stream source was loaded')
  console.log(`RESULT ${failures.length} failed ${assertFixed ? 'recovery invariants' : 'reproduction checks'}; ${checked} checks ran. Harness does not edit production source.`)
  if (failures.length) process.exitCode = 1
} finally { await browser?.close(); await server.close() }

// Browser integration checks against the real pages. All API traffic is local fixtures; no games/signing.
import assert from 'node:assert/strict'
import { mkdir, writeFile } from 'node:fs/promises'
const { chromium } = await import(process.env.ALASHI_PLAYWRIGHT_MODULE ?? 'playwright')
const origin = process.env.ALASHI_STUDIO_UI_URL ?? 'http://127.0.0.1:5195'
const artifacts = process.env.ALASHI_STUDIO_ARTIFACTS ?? '/tmp/alashi-studio-qa'
await mkdir(artifacts, { recursive: true })
const browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--enable-unsafe-swiftshader'] })
const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } })
const page = await context.newPage(), errors = [], writes = []
page.on('pageerror', e => errors.push(e.message))
const game = 'GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b', faction = '1'.repeat(32), signature = '1'.repeat(64)
const sale = n => ({ id: `${signature}:${n}`, signature, slot: 10, event_index: n, log_index: n, block_time: null, type: 'sold', game, faction, units: 1 })
const snapshot = { ok: true, cluster: 'devnet', program_id: '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC', game_pda: game,
  game: { id: '1791548943', phase: 'Market', round: 1, phase_ends_at: 9999999999, settled: false, epoch: 0 }, snapshot_slot: 10, journal_through_slot: 10, commitment: 'confirmed',
  factions: [{ pda: faction, wallet: 'DmqGyi9wBz1RQyueGFRmF7ZuZ4p44YbQKR9LHysbduFo', name: 'Fixture player', cash: '68800000', hard: '0', alive: true, goods: 2, influence: 2, vote: 'Yes' }],
  events: [sale(0)], history_complete: true, fetched_at: '2026-10-09T21:00:00Z' }
await page.addInitScript(() => {
  window.__studioPulses = []
  const animate = Element.prototype.animate
  Element.prototype.animate = function (...args) {
    if (this.classList.contains('studio-pulse')) window.__studioPulses.push(performance.now())
    return animate.apply(this, args)
  }
})
await page.route('**/chain/devnet/**', route => {
  if (route.request().method() !== 'GET') { writes.push(route.request().method()); return route.abort() }
  const journal = route.request().url().includes('/conversations')
  return route.fulfill({ json: journal ? { ok: true, schema: 'alashi.chain_conversations.v1', game_pda: game, entries: [], next_cursor: '0', latest_seq: '0', has_more: false, history_complete: false, recording_started_at: null } : snapshot })
})
const pulses = () => page.evaluate(() => window.__studioPulses.length)
const tick = async ms => { for (let elapsed = 0; elapsed < ms; elapsed += 100) { await page.clock.runFor(Math.min(100, ms - elapsed)); await page.waitForTimeout(15) } }
const poll = async () => { await page.clock.fastForward(12050); await page.waitForTimeout(100) }
await page.clock.install()
try {
  await page.goto(`${origin}/devnet?game=${game}&player=${faction}`)
  await page.locator('.studio-stage canvas').waitFor()
  await page.waitForTimeout(1000)
  assert.equal(await pulses(), 0, 'backfill must not trigger a new reward')
  snapshot.events.push(sale(1)); await poll(); await tick(4200)
  assert.equal(await pulses(), 1, 'new confirmed sale must reach the 3D receipt and platform pulse')
  await page.locator('.devnet-frame').screenshot({ path: `${artifacts}/confirmed-sale.png` })
  await tick(3000); await poll(); await tick(1000)
  assert.equal(await pulses(), 1, 'duplicate snapshot must not replay the reward')
  assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'idle')
  console.log('PASS fresh chain sale, duplicate receipt, idle recovery')
  snapshot.events.push(...[2, 3, 4, 5].map(sale)); await poll(); await tick(20500)
  assert.equal(await pulses(), 4, 'burst plays one active and only the latest two pending cues')
  assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'idle')
  await page.getByRole('tab', { name: 'This player', exact: true }).click()
  assert.match(await page.locator('.devnet-player-overview').innerText(), /68\.8/)
  console.log('PASS bounded burst, unchanged confirmed balance')
  await page.emulateMedia({ reducedMotion: 'reduce' })
  snapshot.events.push(sale(6)); await poll(); await tick(7100)
  assert.equal(await pulses(), 4, 'reduced motion must suppress new flight/pulse')
  await page.emulateMedia({ reducedMotion: 'no-preference' })
  snapshot.events.push(sale(7)); await poll(); await tick(3800)
  assert.equal(await pulses(), 5)
  await page.setViewportSize({ width: 390, height: 844 }); await tick(100)
  assert.equal(await page.locator('.studio-pulse').evaluate(e => e.getAnimations().length), 0, 'resize cancels active pulse')
  await tick(3300)
  await page.setViewportSize({ width: 1440, height: 1000 })
  snapshot.events.push(sale(8)); await poll(); await tick(100)
  await page.evaluate(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true }); document.dispatchEvent(new Event('visibilitychange')) })
  await tick(7100)
  await page.evaluate(() => { delete document.hidden; document.dispatchEvent(new Event('visibilitychange')) })
  await tick(500)
  assert.equal(await pulses(), 5, 'hidden receipt must not play on return')
  snapshot.events.push(sale(9)); await poll(); await tick(800)
  await page.evaluate(() => { const spacer = document.createElement('div'); spacer.style.height = '2500px'; document.body.append(spacer); window.scrollTo(0, 3000) })
  await tick(7100); await page.evaluate(() => window.scrollTo(0, 0)); await tick(500)
  assert.equal(await pulses(), 5, 'offscreen receipt must not play on return')
  await page.evaluate(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true }); document.dispatchEvent(new Event('visibilitychange')) })
  snapshot.events.push(sale(10)); await tick(15000)
  await page.evaluate(() => { delete document.hidden; document.dispatchEvent(new Event('visibilitychange')) })
  await poll(); await tick(7100)
  assert.equal(await pulses(), 5, 'first snapshot after a hidden-tab gap is feed history, not a fresh reward')
  assert.equal(await page.locator('canvas').count(), 1)
  await page.locator('.live-brand').evaluate(e => e.click())
  await tick(1200)
  await page.waitForURL(origin + '/')
  await page.locator('.studio-stage').waitFor({ state: 'detached' })
  assert.equal(await page.locator('.studio-stage').count(), 0, 'SPA unmount removes decorative effects')
  // Same receipt path on the HTTP public stream, still simulated money rather than Solana.
  const record = 'a'.repeat(64), packet = []
  await page.route('**/live-api/**', route => {
    if (route.request().method() !== 'GET') { writes.push(route.request().method()); return route.abort() }
    const path = new URL(route.request().url()).pathname
    const json = path.endsWith('/live/events') ? { ok: true, events: packet, next_cursor: packet.length, has_more: false, history_truncated: false, server_now: Math.floor(Date.now() / 1000), presence: 'connected' }
      : path.endsWith(record) ? { ok: true, agent_record_id: record, registered: true, active_slots: [{ game_id: 2, faction_idx: 0, phase: 'market', round: 1 }] }
      : { ok: false, error: 'owner_cookie_invalid' }
    return route.fulfill({ json, status: json.ok ? 200 : 401 })
  })
  await page.goto(`${origin}/stream?agent=${record}`); await page.locator('.studio-stage canvas').waitFor(); await tick(500)
  packet.push(...[1, 2, 3, 4].map(seq => ({ seq, event_id: `stream-sale-${seq}`, visibility: 'public', kind: 'game_action', room_id: 'fixture', game_id: 2,
    server_created_at: '2026-10-09T21:00:00Z', finality: 'final', branch_id: 'MAIN', action_ref: { actor: 0, action: 'sell' } })))
  await page.clock.fastForward(1900); await page.waitForTimeout(100); await tick(20500)
  assert.equal(await pulses(), 3, 'HTTP stream burst must share the bounded receipt queue')
  assert.equal(await page.locator('.live-event').count(), 4, 'coalescing visual cues cannot drop public receipts')
  assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'idle')
  assert.deepEqual(errors, []); assert.deepEqual(writes, [])
  console.log('PASS public HTTP receipt burst and complete feed')
  console.log('PASS reduced motion, resize, visibility, offscreen, unmount, single Canvas, no page errors/API writes')
  await writeFile(`${artifacts}/results.json`, JSON.stringify({ passed: true, errors, writes, fixture: true }, null, 2))
} finally { await context.close(); await browser.close() }

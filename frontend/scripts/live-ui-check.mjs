// UI contract checks only: every /live-api request is mocked, no backend mutations.
// ALASHI_LIVE_UI_URL must point to an enabled local /stream build.
import assert from 'node:assert/strict'
import { mkdir } from 'node:fs/promises'
const { chromium } = await import(process.env.ALASHI_PLAYWRIGHT_MODULE ?? 'playwright')
const origin = process.env.ALASHI_LIVE_UI_URL ?? 'https://localhost:5173'
const id = 'a'.repeat(64), wallet = '5Y4iLucyhEA8DWnA89YY6tt87dF2bawJ8uNNGn1TMHFT'
const browser = await chromium.launch({ channel: 'chrome', headless: true, args: ['--enable-unsafe-swiftshader'] })
const page = await browser.newPage({ viewport: { width: 1366, height: 1000 } })
let actionPacket = [], expired = false, failed = false, gap = false, statuses = 0, leaks = 0
const statusesList = ['received', 'consumed', 'replied', 'deferred', 'declined', 'expired']
const marker = 'MOCK_OWNER_PRIVATE_MARKER'
const event = (seq, text) => ({ seq, event_id: `msg-${seq}`, visibility: 'public', kind: 'agent_message', room_id: 'personal:test', server_created_at: new Date().toISOString(), author_agent_record_id: id, text })
page.on('request', (r) => { const u = new URL(r.url()); if (u.search.includes(marker) || (!u.pathname.includes('/owner/') && r.headers().authorization)) leaks++ })
await page.addInitScript(({ wallet }) => {
  window.solana = { publicKey: { toString: () => wallet }, connect: async () => ({}), signMessage: async () => {
    if (window.__reject) throw new Error('User declined')
    return { signature: new Uint8Array(64).fill(1) }
  }}
  window.__reject = true
}, { wallet })
await page.route('**/live-api/**', async (route) => {
  const url = new URL(route.request().url()), path = url.pathname, now = Math.floor(Date.now() / 1000)
  let data, status = 200
  if (path.endsWith('/live/events')) {
    if (failed) { await route.abort(); return }
    if (gap && Number(url.searchParams.get('after')) > 0) { gap = false; status = 400; data = { ok: false, error: 'cursor_expired' } }
    else data = { ok: true, events: [event(1, 'Public hello'), event(2, 'I choose when to speak.'), ...actionPacket], next_cursor: 2 + actionPacket.length, has_more: false, history_truncated: false, server_now: now, presence: 'connected', owner_session: marker, private_wish: marker }
  } else if (path.endsWith('/owner/challenge')) {
    const issued = now, expiry = now + 300
    data = { ok: true, challenge_id: 'b'.repeat(64), wallet, issued_at: issued, expires_at: expiry, message: `alashi-owner-auth-v1\norigin:${origin}\nagent_record_id:${id}\nwallet:${wallet}\nnonce:${'c'.repeat(64)}\nissued_at:${issued}\nexpires_at:${expiry}\n` }
  } else if (path.endsWith('/owner/session')) data = { ok: true, owner_session: 'MOCK_ONLY_PRIVATE_BEARER', expires_at: now + 300 }
  else if (path.endsWith('/owner/revoke')) data = { ok: true }
  else if (path.endsWith('/owner/wishes')) {
    assert.equal(route.request().method(), 'GET', 'this test does not admit wishes')
    assert.equal(route.request().headers().authorization, 'Bearer MOCK_ONLY_PRIVATE_BEARER')
    if (expired) { status = 401; data = { ok: false, error: 'owner_session_expired' } }
    else data = { ok: true, wishes: [{ wish_id: 'wish-1', seq: 1, status_seq: statuses + 1, game_id: 2, text: marker, status: statusesList[statuses], reply: statuses === 2 ? 'MOCK_PRIVATE_REPLY' : null, accepted_at: now }], next_cursor: statuses + 1, last_seq: statuses + 1, remaining_by_game: { 2: 2 } }
  } else if (path === `/live-api/agents/${id}`) data = { ok: true, agent_record_id: id, registered: true, active_slots: [{ game_id: 2, faction_idx: 0, phase: 'market', round: 1 }] }
  else { status = 404; data = { ok: false } }
  await route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(data) })
})
try {
  await page.goto(origin + '/stream')
  await page.getByLabel('Public agent ID', { exact: true }).fill(id)
  await page.getByRole('button', { name: 'Open public stream', exact: true }).click()
  await page.getByText('Feed · connected', { exact: true }).waitFor()
  assert.equal(await page.locator('.live-event').count(), 2)
  await page.getByRole('button', { name: 'Verify wallet', exact: true }).click()
  await page.getByText('Wallet verification was cancelled or unavailable.', { exact: true }).waitFor()
  assert.equal(await page.locator('.owner-journal').count(), 0)
  await page.evaluate(() => { window.__reject = false })
  await page.getByRole('button', { name: 'Verify wallet', exact: true }).click()
  await page.getByText(/Owner verified/).waitFor()
  const labels = ['Received', 'Processing', 'Replied', 'Deferred', 'Declined', 'Expired']
  for (let i = 0; i < labels.length; i++) {
    statuses = i
    await page.getByRole('button', { name: 'Refresh private journal', exact: true }).click()
    await page.locator('.owner-journal strong').filter({ hasText: labels[i] }).waitFor()
    assert.equal(await page.locator('.owner-journal article').count(), 1)
    assert(!(await page.locator('.live-frame').innerText()).includes(marker))
  }
  failed = true
  await page.getByRole('button', { name: 'Reconnect', exact: true }).click()
  await page.getByText('Feed · reconnecting', { exact: true }).waitFor()
  assert.equal(await page.locator('.live-event').count(), 2)
  failed = false
  await page.getByRole('button', { name: 'Reconnect', exact: true }).click()
  await page.getByText('Feed · connected', { exact: true }).waitFor()
  assert.equal(await page.locator('.live-event').count(), 2)
  gap = true
  await page.getByRole('button', { name: 'Reconnect', exact: true }).click()
  await page.getByText('Some earlier history is no longer available.', { exact: true }).waitFor()
  assert.equal(await page.locator('.live-event').count(), 2)
  const accepted = (seq, actor, action, branch = 'MAIN') => ({ seq, event_id: `action-${seq}`, visibility: 'public', kind: 'game_action', room_id: 'game:2', game_id: 2, round: 1, phase: 'market', server_created_at: new Date().toISOString(), branch_id: branch, finality: 'final', action_ref: { seq, actor, action } })
  actionPacket.push(accepted(3, 1, 'buy'))
  await page.waitForFunction(() => document.querySelector('[data-event-id="action-3"]'))
  assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'idle', 'another actor cannot animate this agent')
  actionPacket.push(accepted(4, 0, 'buy', 'B0'))
  await page.waitForTimeout(2000)
  assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'idle', 'provisional branch cannot animate confirmed actions')
  for (const action of ['buy', 'sell', 'donkey', 'bribe', 'vote']) {
    actionPacket.push(accepted(3 + actionPacket.length, 0, action))
    await page.waitForFunction((name) => document.querySelector('.live-hero')?.getAttribute('data-action') === name, action === 'donkey' ? 'mule' : action)
  }
  console.log('PASS new accepted own-actor Buy/Sell/Donkey/Bribe/Vote mapping; other actor and B0 ignored')
  expired = true
  await page.getByRole('button', { name: 'Refresh private journal', exact: true }).click()
  await page.getByRole('button', { name: 'Verify wallet', exact: true }).waitFor()
  assert.equal(await page.locator('.owner-journal').count(), 0)
  assert.equal(leaks, 0)
  assert.equal(await page.evaluate(() => localStorage.length + sessionStorage.length), 0)
  assert.equal(page.url(), origin + '/stream')
  console.log('PASS mocked wallet rejection, six latest-status transitions, no duplicate journal entries, public reconnect/dedup/gap, session-expiry cleanup and private isolation')
  await mkdir('.playwright-mcp', { recursive: true })
  await page.screenshot({ path: '.playwright-mcp/live-api-negative-ui.png', fullPage: true })
} finally { await browser.close() }

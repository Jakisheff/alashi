// Pass the default function to Playwright MCP (omit export default); isolated Vite on port 5204.
// Synthetic public GET fixtures only. No real Games, signing, pairing or transactions.
export default async (page) => {
  const game = 'GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b'
  const fixture = { ok: true, cluster: 'devnet', program_id: '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC', game_pda: game,
    game: { id: '1', phase: 'Finished', round: 6, phase_ends_at: 2000000000, settled: true, epoch: 0 },
    snapshot_slot: 10, journal_through_slot: 10, commitment: 'confirmed', factions: [], events: [],
    history_complete: true, fetched_at: '2026-10-13T06:00:00Z' }
  const results = []
  for (const width of [1920, 390]) for (const recovery of ['manual', 'online', 'visible', 'rate-limit', 'partial']) {
    const context = await page.context().browser().newContext({ viewport: { width, height: 844 } })
    const p = await context.newPage(), errors = []
    let requests = 0, writes = 0, unavailable = true, complete = recovery !== 'partial'
    p.on('pageerror', e => errors.push(e.message))
    try {
      await p.route('**/chain/devnet/**', async route => {
        if (route.request().method() !== 'GET') { writes++; return route.abort() }
        const path = new URL(route.request().url()).pathname
        if (path.endsWith('/conversations')) return route.fulfill({ json: { ok: true, schema: 'alashi.chain_conversations.v1', game_pda: game, entries: [], next_cursor: '0', latest_seq: '0', has_more: false, history_complete: false, recording_started_at: null } })
        if (path.endsWith('/dialogue')) return route.fulfill({ status: 404, json: { ok: false } })
        requests++
        return unavailable ? route.fulfill({ status: recovery === 'rate-limit' ? 429 : 503, headers: recovery === 'rate-limit' ? { 'Retry-After': '60' } : {}, json: { ok: false } })
          : route.fulfill({ json: { ...fixture, history_complete: complete } })
      })
      await p.clock.install()
      await p.goto(`http://127.0.0.1:5204/devnet?game=${game}`)
      await p.getByText('Unavailable', { exact: true }).waitFor()
      const retry = p.getByRole('button', { name: 'Retry connection', exact: true })
      if (!await retry.isVisible()) throw new Error('Retry must be visible on Setup before any snapshot loads')
      const before = requests
      unavailable = false
      if (recovery === 'online') await p.evaluate(() => window.dispatchEvent(new Event('online')))
      else if (recovery === 'visible') await p.evaluate(() => {
        Object.defineProperty(document, 'hidden', { configurable: true, value: true }); document.dispatchEvent(new Event('visibilitychange'))
        delete document.hidden; document.dispatchEvent(new Event('visibilitychange'))
      })
      else await retry.click()
      await p.clock.runFor(1000); await p.waitForTimeout(100)
      if (recovery === 'rate-limit') {
        // Explicit Retry and foreground/network signals must all respect the server limit.
        await p.evaluate(() => { window.dispatchEvent(new Event('online')); window.dispatchEvent(new Event('pageshow')); document.dispatchEvent(new Event('visibilitychange')) })
        await p.clock.runFor(58_000); await p.waitForTimeout(100)
        if (requests !== before) throw new Error('Recovery bypassed Retry-After')
        await p.clock.runFor(1500); await p.waitForTimeout(100)
      }
      if (requests <= before) throw new Error(`${recovery} did not recover without reloading`)
      await retry.waitFor({ state: 'detached' })
      if (!(await p.locator('body').innerText()).includes('Finished')) throw new Error('Recovered terminal snapshot did not render')
      if (recovery === 'partial') {
        const partialRequests = requests
        complete = true
        await p.clock.fastForward(12_100); await p.waitForTimeout(100)
        if (requests <= partialRequests) throw new Error('Settled partial history stopped before completion')
      }
      const atFinal = requests
      await p.clock.fastForward(240_000); await p.waitForTimeout(100)
      await p.evaluate(() => { window.dispatchEvent(new Event('online')); window.dispatchEvent(new Event('pageshow')) })
      await p.clock.runFor(1000)
      if (requests !== atFinal) throw new Error('Complete settled game continued polling')
      if (writes || errors.length) throw new Error(JSON.stringify({ writes, errors }))
      if (await p.evaluate(() => document.documentElement.scrollWidth > innerWidth)) throw new Error('Horizontal overflow')
      results.push({ width, recovery, requests, finished: true, terminalPollingStopped: true, writes, errors })
    } finally { await context.close() }
  }
  return results
}

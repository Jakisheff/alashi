// Pass the default function to the Playwright MCP code runner (omit export default). Use an isolated local preview on port 5203.
// Synthetic public GET fixtures only: no games, owner sessions, signing or transactions.
export default async (page) => {
  const equal = (actual, expected, message = 'Unexpected browser result') => { if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error(`${message}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`) }
  const result = []
  for (const width of [1920, 390]) {
    const context = await page.context().browser().newContext({ viewport: { width, height: width === 390 ? 844 : 1080 } })
    try {
      const tab = await context.newPage(), errors = [], writes = []
      tab.on('pageerror', e => errors.push(e.message))
      const game = 'GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b'
      const a = '8RzVGTdFCqXGoyASbtStHpbin6gi6jcSH6gxQcWPHnqB', b = 'GaU1akfgaMRwqQZDZFh35g8EacCTUdZQMNBRxqzeyxtD'
      const signature = '1'.repeat(64)
      const event = (n, fields) => ({ id: `${signature}:${n}`, signature, slot: 10, event_index: n, log_index: n, block_time: null, game, ...fields })
      const events = [event(0, { type: 'phase_advanced', phase: 1, round: 1 }), event(1, { type: 'produced', faction: a, goods: 2 }), event(2, { type: 'phase_advanced', phase: 2, round: 1 })]
      const faction = (pda, name) => ({ pda, name, wallet: pda, cash: '0', hard: '0', alive: true, goods: 2, influence: 1, vote: 'Abstain' })
      const snapshot = { ok: true, cluster: 'devnet', program_id: '3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC', game_pda: game,
        game: { id: '1', phase: 'Action', round: 1, phase_ends_at: 2000000000, settled: false, epoch: 1 }, snapshot_slot: 10, journal_through_slot: 10,
        commitment: 'confirmed', factions: [faction(a, 'Local test A'), faction(b, 'Local test B')], events, history_complete: true, fetched_at: '2026-10-12T19:00:00Z' }
      const entries = events.map((e, i) => ({ message_id: String(i + 1).repeat(64), seq: String(i + 1), game_pda: game,
        author_faction_pda: i % 2 ? b : a, to_faction_pda: i % 2 ? a : b, text: `LOCAL UI TEST message ${i + 1}`, reply_to_message_id: i ? String(i).repeat(64) : null,
        related_offer_entry_id: null, round: 1, phase: 'Action', phase_ends_at: 2000000000, created_at: '2026-10-12T19:00:00Z', observed_chain_event_id: e.id, source: 'runner_reported' }))
      await tab.route('**/chain/devnet/**', route => {
        if (route.request().method() !== 'GET') { writes.push(route.request().method()); return route.abort() }
        const url = new URL(route.request().url()), dialogue = url.pathname.endsWith('/dialogue'), journal = url.pathname.endsWith('/conversations')
        const after = Number(url.searchParams.get('after') ?? '0')
        return route.fulfill({ json: dialogue ? { ok: true, schema: 'alashi.chain_dialogue.v1', game_pda: game, entries: entries.filter(e => Number(e.seq) > after), next_cursor: '3', latest_seq: '3', has_more: false }
          : journal ? { ok: true, schema: 'alashi.chain_conversations.v1', game_pda: game, entries: [], next_cursor: '0', latest_seq: '0', has_more: false, history_complete: false, recording_started_at: null } : snapshot })
      })
      await tab.goto(`http://127.0.0.1:5203/devnet?game=${game}&player=${a}`)
      const dialogue = tab.getByRole('region', { name: 'Agent dialogue' })
      await dialogue.locator('li').nth(2).waitFor()
      await tab.getByRole('button', { name: 'Replay confirmed actions' }).click()
      await tab.getByRole('button', { name: 'Pause', exact: true }).click()
      equal(await dialogue.locator('li').count(), 1, 'replay start hides future anchored dialogue')
      const range = tab.getByRole('slider', { name: 'Replay action' })
      await range.fill('2'); await range.dispatchEvent('input'); await range.dispatchEvent('change')
      await tab.getByRole('button', { name: 'Resume', exact: true }).waitFor()
      equal(await dialogue.locator('li').count(), 3, 'seeking while paused shows the selected dialogue anchor')
      await tab.waitForTimeout(2200)
      equal(await range.inputValue(), '2', 'seeking while paused does not resume playback')
      equal(await dialogue.locator('li').count(), 3, 'paused seek retains its selected transcript')
      await range.fill('0'); await range.dispatchEvent('input'); await range.dispatchEvent('change')
      await tab.getByRole('button', { name: 'Resume', exact: true }).waitFor()
      equal(await dialogue.locator('li').count(), 1, 'rewinding a paused replay hides future messages again')
      await tab.getByRole('button', { name: 'Return to latest state', exact: true }).click()
      equal(await dialogue.locator('li').count(), 3, 'latest restores the full recorded transcript')
      await tab.getByRole('button', { name: 'Replay confirmed actions' }).click()
      await range.fill('2'); await range.dispatchEvent('input'); await range.dispatchEvent('change')
      await tab.getByRole('button', { name: 'Play', exact: true }).waitFor({ timeout: 15000 })
      equal(await dialogue.locator('li').count(), 3, 'repeated replay completion retains dialogue')
      equal(await tab.locator('.live-hero').getAttribute('data-action'), 'idle', 'completion stops animations while retaining the transcript')
      await range.fill('0'); await range.dispatchEvent('input'); await range.dispatchEvent('change')
      await tab.getByRole('button', { name: 'Resume', exact: true }).waitFor()
      equal(await dialogue.locator('li').count(), 1, 'rewinding after completion keeps the replay paused')
      equal(await dialogue.locator('a[href*="explorer"]').count(), 0, 'speech is not a chain transaction')
      equal(await tab.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, 'no horizontal overflow')
      equal(errors, []); equal(writes, [])
      await tab.screenshot({ path: `/tmp/alashi-dialogue-replay-evidence/local-end-${width}.png`, fullPage: true })
      result.push({ width, messagesAtEnd: 3, rewindMessages: 1, idle: true, errors, writes })
    } finally { await context.close() }
  }
  return result
}

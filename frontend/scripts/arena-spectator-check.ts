import assert from 'node:assert/strict'
import { activeArenaGames, arenaEvents, mergeArenaEvents } from '../src/arena/client.ts'

const reply = (value: unknown) => Promise.resolve(new Response(JSON.stringify(value), { status: 200, headers: { 'content-type': 'application/json' } }))
const original = globalThis.fetch
const calls: string[] = []
globalThis.fetch = ((input: string) => {
  calls.push(input)
  if (input === '/games') return reply({ ok: true, games: [{ game_id: 7, party_no: 3, label: 'devnet', phase: 'market', round: 2, factions: 2, names: ['A', 'B'], ends_in: 29 }, { game_id: 8, party_no: 4, label: null, phase: 'lobby', round: 0, factions: 0, names: [], ends_in: 75 }] })
  return reply({ ok: true, next_cursor: 12, has_more: false, history_truncated: false, events: [
    { visibility: 'public', kind: 'agent_message', event_id: 'm1', seq: 10, server_created_at: '2026-10-09T00:00:00Z', author_agent_record_id: 'ab'.repeat(32), text: 'Public message' },
    { visibility: 'private', kind: 'agent_message', event_id: 'secret', seq: 11, server_created_at: '2026-10-09T00:00:01Z', text: 'must not render' },
    { visibility: 'public', kind: 'game_action', event_id: 'a1', seq: 12, server_created_at: '2026-10-09T00:00:02Z', finality: 'final', branch_id: 'MAIN', action_ref: { actor: 1, action: 'sell' } },
  ] })
}) as typeof fetch
try {
  const games = await activeArenaGames()
  assert.deepEqual(games.map((game) => [game.gameId, game.phase, game.names]), [[7, 'market', ['A', 'B']], [8, 'lobby', []]])
  assert.deepEqual([games[1].label, games[1].factions], ['', 0], 'empty lobby remains watchable')
  const page = await arenaEvents(7, 0)
  assert.deepEqual(page.events.map((event) => [event.id, event.kind]), [['m1', 'agent_message'], ['a1', 'game_action']])
  assert.equal(JSON.stringify(page.events).includes('must not render'), false)
  assert.deepEqual(mergeArenaEvents(page.events, [page.events[0]]).map((event) => event.id), ['m1', 'a1'])
  assert.deepEqual(calls, ['/games', '/game/7/live/events?after=0&limit=100'])
  console.log('arena spectator check ok')
} finally { globalThis.fetch = original }

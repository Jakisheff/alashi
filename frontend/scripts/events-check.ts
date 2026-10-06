// Run: npm run check:events. Cursor, dedupe and gap rules from PUBLIC_EVENTS_FOR_DIN (docs a3d8ed8).
import assert from 'node:assert/strict'
import { followGame, parseAgentId, type AgentProfile } from '../src/agent.ts'
import { advance, eventText, resultText, standings, type ArenaEvent, type ArenaState } from '../src/events.ts'

const ev = (seq: number, action = 'produce', ok = true): ArenaEvent => ({
  seq, event_id: `2:20:${seq}`, round: 1, phase: 'action', actor: 0, action, by: 'fallback', ok, ts: seq,
})
const state = (seqs: number[], party_no = 20): ArenaState => ({
  game_id: 2, party_no, round: 1, phase: 'action',
  factions: [{ idx: 0, name: 'Aitore' }],
  recent_actions: seqs.map((s) => ev(s)),
  recent_actions_range: { first_seq: seqs[0] ?? null, last_seq: seqs.at(-1) ?? null, retained_first_seq: 1, limit: 12 },
})

// First look: the window is history, nothing animates.
let u = advance(null, state([1, 2, 3]))
assert.deepEqual([u.history.length, u.fresh.length, u.gap, u.cursor?.lastSeq], [3, 0, false, 3])
// Same snapshot again (passive poll): nothing new.
u = advance(u.cursor, state([1, 2, 3]))
assert.deepEqual([u.fresh.length, u.gap], [0, false])
// New events only, in order.
u = advance(u.cursor, state([2, 3, 4, 5]))
assert.deepEqual(u.fresh.map((e) => e.seq), [4, 5])
// Window jumped past seq 6: gap, show what is there.
u = advance(u.cursor, state([8, 9]))
assert.deepEqual([u.gap, u.fresh.map((e) => e.seq).join(), u.cursor?.lastSeq], [true, '8,9', 9])
// New party in the same game: new scope, history again.
u = advance(u.cursor, state([1], 21))
assert.deepEqual([u.history.length, u.fresh.length, u.cursor?.scope], [1, 0, '2:21'])
// Empty log.
assert.equal(advance(null, state([])).cursor?.lastSeq, 0)
// Text never invents amounts or targets.
assert.equal(eventText(ev(1, 'sell'), state([]).factions), '📦 Aitore sold goods')
assert.equal(eventText(ev(1, 'vote', false), state([]).factions), "⚠️ Aitore's move was rejected")
// Finished game: ranks[place] = faction index (shape from live game 1, party 19).
const result = { game_id: 1, party_no: 19, ranks: [1, 0], agents: [{ name: 'CodexSolAgent' }, { name: 'OpenCodeGLMFlash' }] }
assert.deepEqual(standings(result), ['OpenCodeGLMFlash', 'CodexSolAgent'])
assert.equal(resultText(result), '🏁 Game over. OpenCodeGLMFlash wins.')
// Watch link: only a 64 lowercase hex id is ever sent; the scene follows the agent's game and keeps the last one.
assert.equal(parseAgentId('ab'.repeat(32)), 'ab'.repeat(32))
for (const bad of [null, '', 'AB'.repeat(32), 'ab'.repeat(31), 'ab'.repeat(32) + '/../x']) assert.equal(parseAgentId(bad), null)
const slot = { game_id: 2, party_no: 20, label: 'x', phase: 'market', round: 1, faction_idx: 0, faction_name: 'A' }
const profile = (slots: (typeof slot)[]): AgentProfile =>
  ({ ok: true, agent_record_id: 'ab'.repeat(32), character_id: 'cd'.repeat(32), registered: true, active_slots: slots })
assert.equal(followGame(null, undefined), null)
assert.equal(followGame(null, profile([])), null)
assert.equal(followGame(null, profile([slot])), 2)
assert.equal(followGame(2, profile([])), 2) // game finished: keep showing its result
assert.equal(followGame(2, { ok: false, error: 'unknown_agent' }), 2)
console.log('events check ok')

import assert from 'node:assert/strict'
import { decodeDialogue, dialogueView } from '../src/devnet/dialogueClient.ts'
import { ChainApiError, type ChainSnapshot } from '../src/devnet/client.ts'
const game = 'GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b'
const a = '8RzVGTdFCqXGoyASbtStHpbin6gi6jcSH6gxQcWPHnqB'
const b = 'GaU1akfgaMRwqQZDZFh35g8EacCTUdZQMNBRxqzeyxtD'
const first = { message_id: 'a'.repeat(64), seq: '1', game_pda: game, author_faction_pda: a, to_faction_pda: b,
  text: 'What price works for you?', reply_to_message_id: null, related_offer_entry_id: null, round: 2, phase: 'Market', phase_ends_at: 2000000000,
  created_at: '2026-10-12T18:00:00Z', observed_chain_event_id: 'chain:1', source: 'runner_reported' }
const second = { ...first, message_id: 'b'.repeat(64), seq: '2', author_faction_pda: b, to_faction_pda: a, text: 'I will inspect the offer.', reply_to_message_id: first.message_id, observed_chain_event_id: 'chain:2' }
const page = (entries: unknown[], cursor = '2', latest = '2') => ({ ok: true, schema: 'alashi.chain_dialogue.v1', game_pda: game, entries, next_cursor: cursor, latest_seq: latest, has_more: BigInt(latest) > BigInt(cursor) })
const full = decodeDialogue(page([first, second]), game)
assert.deepEqual(full.entries.map((e) => e.author_faction_pda), [a, b])
assert.equal(full.entries[1].reply_to_message_id, first.message_id)
for (const change of [{ source: 'onchain_event' }, { receipt: {} }, { round: 0 }, { phase: 'Finished' }, { text: '   ' }, { text: 'x\ny' }, { text: 'x\u0085y' }, { message_id: 'A'.repeat(64) }, { reply_to_message_id: 'not-canonical' }, { related_offer_entry_id: 4 }, { to_faction_pda: a }, { game_pda: b }]) {
  assert.throws(() => decodeDialogue(page([{ ...first, ...change }], '1', '1'), game), ChainApiError)
}
assert.throws(() => decodeDialogue({ ...page([first, second]), schema: 'wrong' }, game), ChainApiError)
assert.throws(() => decodeDialogue(page([first, second]), b), ChainApiError)
assert.throws(() => decodeDialogue(page([first, first]), game), ChainApiError)
const p1 = decodeDialogue(page([first], '1', '2'), game)
assert.equal(p1.has_more, true)
const p2 = decodeDialogue(page([second]), game, p1.next_cursor)
assert.equal(p2.next_cursor, '2'); assert.equal(p2.has_more, false)
assert.throws(() => decodeDialogue(page([first], '1', '2'), game, '1'), ChainApiError, 'cursor cannot replay earlier rows')
assert.deepEqual(decodeDialogue(page([], '2', '2'), game, '2').entries, [])
// Different journals can share numeric sequence values; dialogue only compares its own cursor.
assert.equal(decodeDialogue(page([first], '1', '1'), game).next_cursor, '1')
const snapshot = { pda: game, factions: [{ pda: a, name: 'A' }, { pda: b, name: 'B' }], events: [{ id: 'chain:1' }, { id: 'chain:2' }] } as unknown as ChainSnapshot
assert.equal(dialogueView(full.entries, snapshot, '').timeline.length, 0)
assert.equal(dialogueView(full.entries, snapshot, 'chain:1').timeline.length, 1)
assert.equal(dialogueView(full.entries, snapshot, 'chain:2').timeline.length, 2)
assert.equal(dialogueView(full.entries, snapshot, 'chain:1').timeline.length, 1, 'rewinding hides future messages')
assert.equal(dialogueView(full.entries, snapshot, null).timeline.length, 2)
const absent = { ...first, observed_chain_event_id: null }
const unknown = { ...second, observed_chain_event_id: 'unknown-anchor' }
const detached = dialogueView([absent, unknown], snapshot, 'chain:1')
assert.equal(detached.timeline.length, 0); assert.equal(detached.separate.length, 2)
assert.equal(dialogueView([{ ...first, game_pda: b }], snapshot, null).timeline.length, 0)
console.log('Dialogue decoder, independent pagination and replay boundaries passed.')

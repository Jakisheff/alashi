import assert from 'node:assert/strict'
import { CHAIN_PROGRAM, chainSnapshot, decodeSnapshot, formatCash, validBase58, ChainApiError } from '../src/devnet/client.ts'
import { defaultWinner, eventLabel, liveAdditions, newLiveCursor, playerEvents, visualAction } from '../src/devnet/playback.ts'
const pda = 'GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b'
const faction = { pda: '11111111111111111111111111111111', wallet: 'DmqGyi9wBz1RQyueGFRmF7ZuZ4p44YbQKR9LHysbduFo', name: 'Unit fixture', cash: '68800000', hard: '1000000', alive: true, goods: 2, influence: 2, vote: 'Yes' }
const event = { id: `${'1'.repeat(64)}:0`, signature: '1'.repeat(64), slot: 1, event_index: 0, log_index: 0, block_time: null, type: 'sold', game: pda, faction: faction.pda, units: 1 }
const fixture = () => ({ ok: true, cluster: 'devnet', program_id: CHAIN_PROGRAM, game_pda: pda, game: { id: '1791548943', phase: 'Finished', round: 6, phase_ends_at: 0, settled: true, epoch: 0 }, snapshot_slot: 1, journal_through_slot: 1, commitment: 'confirmed', factions: [faction], events: [event], history_complete: true, fetched_at: '2026-10-09T13:00:00Z' })
const snapshot = decodeSnapshot(fixture(), pda), actor = snapshot.factions[0], accepted = snapshot.events[0]
assert.ok(validBase58(pda)); assert.ok(!validBase58('not-a-key')); assert.ok(!validBase58('1'.repeat(33)))
assert.equal(formatCash('18446744073709551615'), '18446744073709.551615 alashi')
assert.equal(visualAction(accepted, actor), 'sell')
for (const [type, action] of Object.entries({ goods_bought: 'buy', donkey_bought: 'mule', shuttled_ev: 'mule', bribe_given: 'bribe', vote_cast: 'vote' })) assert.equal(visualAction({ ...accepted, type }, actor), action)
assert.equal(visualAction({ ...accepted, faction: pda }, actor), null)
assert.equal(visualAction({ ...accepted, type: 'produced' }, actor), null, 'production does not invent an emotional cue')
assert.equal(visualAction({ ...accepted, type: 'payout', wallet: actor.wallet, rank: 1 }, actor), null)
assert.equal(visualAction({ ...accepted, type: 'payout', wallet: actor.wallet, rank: 0 }, actor), 'victory')
assert.match(eventLabel({ ...accepted, type: 'vote_cast', choice: 1 }, actor), /No$/)
assert.equal(playerEvents([{ ...accepted, faction: pda }], actor).length, 0)
assert.throws(() => decodeSnapshot({ ...fixture(), cluster: 'mainnet' }, pda), ChainApiError)
assert.throws(() => decodeSnapshot({ ...fixture(), game_pda: faction.pda }, pda), ChainApiError)
assert.throws(() => decodeSnapshot({ ...fixture(), events: [{ ...event, game: faction.pda }] }, pda), ChainApiError)
assert.throws(() => decodeSnapshot({ ...fixture(), events: [event, event] }, pda), ChainApiError)
assert.throws(() => decodeSnapshot({ ...fixture(), events: [{ ...event, event_index: 1 }, { ...event, id: `${'1'.repeat(64)}:1`, log_index: 1 }] }, pda), ChainApiError)
assert.throws(() => decodeSnapshot({ ...fixture(), factions: [{ ...faction, cash: '18446744073709551616' }] }, pda), ChainApiError)
const projected = decodeSnapshot({ ...fixture(), private_wish: 'MUST NOT LEAK', events: [{ ...event, private_wish: 'MUST NOT LEAK' }] }, pda)
assert.ok(!JSON.stringify(projected).includes('MUST NOT LEAK'))
assert.equal(decodeSnapshot({ ...fixture(), history_complete: false }, pda).complete, false)
let options: RequestInit | undefined
await chainSnapshot(pda, undefined, async (_url, init) => { options = init; return new Response(JSON.stringify(fixture()), { headers: { 'Content-Type': 'application/json' } }) })
assert.equal(options?.credentials, 'omit'); assert.equal(options?.redirect, 'error'); assert.equal(options?.referrerPolicy, 'no-referrer')
await assert.rejects(chainSnapshot(pda, undefined, async () => new Response('', { status: 429, headers: { 'Retry-After': '45' } })), (error: unknown) => error instanceof ChainApiError && error.code === 'rate_limit' && error.retryMs === 45000)
await assert.rejects(chainSnapshot(pda, undefined, async () => new Response('<html/>', { status: 200 })), ChainApiError)
console.log('devnet viewer projection, identity, action mapping, safe money, private-field exclusion and HTTP retry checks passed')

assert.equal(defaultWinner(snapshot), undefined, 'no payout means explicit player choice')
const winner = decodeSnapshot({ ...fixture(), events: [{ ...event, type: 'payout', wallet: actor.wallet, rank: 0 }] }, pda)
assert.equal(defaultWinner(winner)?.pda, actor.pda)
assert.equal(defaultWinner({ ...winner, complete: false }), undefined)
assert.equal(defaultWinner({ ...winner, settled: false }), undefined)
assert.equal(defaultWinner({ ...winner, events: [...winner.events, { ...winner.events[0], id: 'ambiguous' }] }), undefined)
assert.throws(() => decodeSnapshot({ ...fixture(), commitment: 'processed' }, pda), ChainApiError)
assert.throws(() => decodeSnapshot({ ...fixture(), journal_through_slot: 2 }, pda), ChainApiError)
assert.throws(() => decodeSnapshot({ ...fixture(), events: [{ ...event, id: 'unstable' }] }, pda), ChainApiError)
assert.equal(snapshot.factions[0].hard, '1000000')
const e = accepted, older = { ...e, id: `${'2'.repeat(64)}:0`, signature: '2'.repeat(64), slot: 0 }
const f = { ...e, id: `${'3'.repeat(64)}:0`, signature: '3'.repeat(64), slot: 2 }
const cursor = newLiveCursor()
assert.deepEqual(liveAdditions(cursor, [e], true), [], 'first complete history is backfill')
assert.deepEqual(liveAdditions(cursor, [older], false), [], 'incomplete history never animates')
assert.deepEqual(liveAdditions(cursor, [older, e, f], true).map((row) => row.id), [f.id], 'only the newly confirmed tail animates')
const rebaseline = newLiveCursor()
liveAdditions(rebaseline, [e], true)
assert.deepEqual(liveAdditions(rebaseline, [older, f], true), [], 'missing prior tail rebaselines without animation')
assert.deepEqual(liveAdditions(rebaseline, [older, f, { ...f, id: `${'4'.repeat(64)}:0`, signature: '4'.repeat(64) }], true).length, 1)
console.log('counter-contract slot, stable receipt ID and explicit/default-winner selection checks passed')

import assert from 'node:assert/strict'
import { chainSnapshot, CHAIN_PROGRAM } from '../src/devnet/client.ts'
import { decodePublicDonkeySnapshot, donkeySnapshotInput, replayJournalPrefix } from '../src/devnet/donkey/adapter.ts'
import { projectDonkeyRisk } from '../src/devnet/donkey/model.ts'
import { eventLabel, playerEvents, visualAction } from '../src/devnet/playback.ts'

// Local DTO fixtures and fetch mocks; never a production outcome or transaction.
const game = 'GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b'
const faction = '8RzVGTdFCqXGoyASbtStHpbin6gi6jcSH6gxQcWPHnqB'
const other = 'GaU1akfgaMRwqQZDZFh35g8EacCTUdZQMNBRxqzeyxtD'
const sig = '1'.repeat(64)
const event = (type: string, i: number, extra = {}) => ({ type, game, signature: sig, id: `${sig}:${i}`, slot: 100, event_index: i, log_index: i, ...extra })
const attempt = event('donkey_attempted', 1, { faction, round: 2, fee: '5000000' })
const resolution = event('donkey_resolved', 2, { faction, round: 2, fee: '5000000', reason: 0, success: true, goods: 5 })
export function adapterFixture(kind = 'success', epoch = 2) {
  return { ok: true, cluster: 'devnet', program_id: CHAIN_PROGRAM, commitment: 'confirmed', game_pda: game,
    snapshot_slot: 101, journal_through_slot: 100, history_complete: kind !== 'partial', fetched_at: '2026-10-11T14:30:00Z',
    game: { id: '2', epoch, round: 2, phase: ['terms', 'pending', 'partial'].includes(kind) ? kind === 'terms' ? 'Action' : 'Law' : 'Finished',
      phase_ends_at: 0, settled: !['terms', 'pending', 'partial'].includes(kind) },
    factions: [{ pda: faction, wallet: faction, name: 'Local QA faction', cash: '10000000', hard: '0', alive: true, goods: 5, influence: 0, vote: 'Abstain' },
      { pda: other, wallet: other, name: 'Other QA faction', cash: '9000000', hard: '0', alive: true, goods: 0, influence: 0, vote: 'Abstain' }],
    events: epoch < 2 ? [event('phase_advanced', 0, { round: 2, phase: 'Action' }), event('donkey_bought', 1, { faction, round: 2 })]
      : [event('phase_advanced', 0, { round: 2, phase: 'Action' }), ...(kind === 'terms' ? [] : [attempt]),
        ...(['terms', 'pending'].includes(kind) ? [] : [{ ...resolution, success: kind === 'success' || kind === 'partial', goods: kind === 'success' || kind === 'partial' ? 5 : 0, reason: kind === 'abort' ? 1 : 0 }])],
  }
}
let checks = 0
const check = (condition: unknown, message: string) => { assert.ok(condition, message); checks++ }
for (const [kind, expected] of [['success', 'success'], ['empty', 'empty'], ['abort', 'abort'], ['pending', 'pending'], ['partial', 'unavailable'], ['terms', 'ineligible']] as const) {
  const snapshot = decodePublicDonkeySnapshot(adapterFixture(kind), game)
  const input = donkeySnapshotInput(snapshot, faction), view = projectDonkeyRisk(input)
  check(view.kind === expected, `${kind} projects independently`)
  check(input.contract === 'candidate' && snapshot.donkeyContract === 'candidate-only', 'Never auto-upgrade source to deployed verification')
  check(input.oracleReady === null && input.actionUsed === null && input.marketPriceUnits === null, 'Absent public account fields remain unknown')
  for (const e of snapshot.events.filter((e) => e.type.startsWith('donkey_'))) check(visualAction(e, snapshot.factions[0]) === null, 'No lottery choreography is invented')
}
// The confirmed API now publishes acted; live state must not discard it, while replay cannot use a future stamp.
for (const acted of [true, false]) {
  const raw = adapterFixture('terms')
  Object.assign(raw.factions[0], { acted })
  const snapshot = decodePublicDonkeySnapshot(raw, game)
  check(donkeySnapshotInput(snapshot, faction).actionUsed === acted, 'Live action stamp from the confirmed snapshot is retained')
  check(donkeySnapshotInput(snapshot, faction, []).actionUsed === null, 'Replay does not borrow the live action stamp')
}
const malformedActed = adapterFixture('terms')
Object.assign(malformedActed.factions[0], { acted: 'false' })
assert.throws(() => decodePublicDonkeySnapshot(malformedActed, game)); checks++
const snap = decodePublicDonkeySnapshot(adapterFixture(), game), f = snap.factions[0], selected = playerEvents(snap.events, f)
// Terminal completeness belongs to the full fetched journal, never its replay prefix.
for (const terminal of [{ phase: 'Law', settled: true }, { phase: 'Finished', settled: false }, { phase: 'Aborted', settled: false }]) {
  const raw = adapterFixture('pending'); Object.assign(raw.game, terminal)
  assert.throws(() => decodePublicDonkeySnapshot(raw, game), 'Complete terminal orphan fails closed'); checks++
}
for (const phase of ['Action', 'Law']) {
  const raw = adapterFixture('pending'); raw.game.phase = phase
  const live = decodePublicDonkeySnapshot(raw, game)
  check(projectDonkeyRisk(donkeySnapshotInput(live, faction)).kind === 'pending', 'Genuinely live attempt remains pending')
}
const terminalPartial = adapterFixture('pending')
Object.assign(terminalPartial.game, { phase: 'Finished', settled: true }); terminalPartial.history_complete = false
check(projectDonkeyRisk(donkeySnapshotInput(decodePublicDonkeySnapshot(terminalPartial, game), faction)).kind === 'unavailable', 'Partial terminal history does not infer an outcome')
for (const result of ['success', 'empty', 'abort']) {
  const full = decodePublicDonkeySnapshot(adapterFixture(result), game)
  const history = playerEvents(full.events, full.factions[0])
  check(projectDonkeyRisk(donkeySnapshotInput(full, faction, replayJournalPrefix(full, history, 1))).kind === 'pending', 'Valid terminal full journal preserves historical pending before resolution')
  check(projectDonkeyRisk(donkeySnapshotInput(full, faction, replayJournalPrefix(full, history, 2))).kind === result, 'Valid terminal full journal resolves after the cursor')
}
const earlierOrphan = adapterFixture('pending'); earlierOrphan.game.phase = 'Finished'; earlierOrphan.game.round = 3
assert.throws(() => decodePublicDonkeySnapshot(earlierOrphan, game), 'Terminal guard checks earlier rounds as well as the final round'); checks++
const otherOrphan = adapterFixture('pending'); otherOrphan.game.phase = 'Aborted'; otherOrphan.events[1] = { ...otherOrphan.events[1], faction: other }
assert.throws(() => decodePublicDonkeySnapshot(otherOrphan, game), 'Terminal guard checks other actors in the full journal'); checks++
for (const [position, kind] of [[-1, 'unavailable'], [0, 'ineligible'], [1, 'pending'], [2, 'success'], [3, 'success'], [1, 'pending'], [0, 'ineligible'], [2, 'success']] as const) {
  const prefix = replayJournalPrefix(snap, selected, position)
  check(projectDonkeyRisk(donkeySnapshotInput(snap, faction, prefix)).kind === kind, 'Repeated seek projects exactly frozen prefix')
  check(prefix.length <= snap.events.length, 'No future receipt beyond snapshot')
}
check(projectDonkeyRisk(donkeySnapshotInput(snap, other)).kind === 'ineligible', 'Player change clears outcome')
check(eventLabel(snap.events[1], f).includes('pending'), 'Commitment caption remains pending')
check(eventLabel(snap.events[2], f).includes('+5 goods'), 'Success caption has explicit goods')
const abort = decodePublicDonkeySnapshot(adapterFixture('abort'), game)
check(eventLabel(abort.events[2], f).includes('no random draw'), 'Abort caption is distinct from empty draw')
check(projectDonkeyRisk(donkeySnapshotInput(decodePublicDonkeySnapshot(adapterFixture('empty'), game), faction)).detail.includes('candidate'), 'Draw is indexer candidate reported')
for (const epoch of [0, 1]) {
  const legacy = decodePublicDonkeySnapshot(adapterFixture('success', epoch), game)
  check(projectDonkeyRisk(donkeySnapshotInput(legacy, faction)).kind === 'legacy', 'Legacy labels kept')
  check(visualAction(legacy.events[1], f) === 'mule', 'Legacy existing animation retained')
}
for (const patch of [{ success: 'false' }, { reason: 2 }, { fee: 5000000 }, { goods: 4 }, { faction: 'invalid' }]) {
  const raw = adapterFixture(); raw.events[2] = { ...raw.events[2], ...patch } as typeof raw.events[2]
  assert.throws(() => decodePublicDonkeySnapshot(raw, game)); checks++
}
assert.throws(() => donkeySnapshotInput(snap, faction, [snap.events[2]])); checks++
assert.throws(() => donkeySnapshotInput(snap, 'invalid')); checks++
const orphan = adapterFixture(); orphan.events.splice(1, 1)
assert.throws(() => decodePublicDonkeySnapshot(orphan, game)); checks++
const duplicate = adapterFixture(); duplicate.events.push({ ...duplicate.events[2], id: `${sig}:3`, event_index: 3, log_index: 3 })
assert.throws(() => decodePublicDonkeySnapshot(duplicate, game)); checks++
check(replayJournalPrefix(snap, [{ ...snap.events[1], id: 'missing' }], 0).length === 0, 'Missing replay anchor fails closed')
let request: { url: string; options?: RequestInit } | null = null
const fetcher = (async (url, options) => { request = { url: String(url), options }; return new Response(JSON.stringify(adapterFixture()), { status: 200 }) }) as typeof fetch
const fetched = await chainSnapshot(game, undefined, fetcher, decodePublicDonkeySnapshot)
check(fetched.donkeyReceipts.length === 2, 'Actual public fetch path normalizes new receipts')
const req = request as unknown as { url: string; options: RequestInit }
check(req.url === `/chain/devnet/games/${game}`, 'Only actual public Game route')
check(req.options.credentials === 'omit' && req.options.redirect === 'error' && req.options.cache === 'no-store', 'Anonymous transport preserved')
check(!req.options.body && !req.options.method && !('Authorization' in (req.options.headers as object)), 'No invented write/auth')
for (const status of [404, 429, 503]) {
  await assert.rejects(chainSnapshot(game, undefined, (async () => new Response('', { status })) as typeof fetch, decodePublicDonkeySnapshot)); checks++
}
console.log(`donkey-risk-adapter: ${checks} checks passed; local DTO/fetch mocks, no chain/owner calls`)

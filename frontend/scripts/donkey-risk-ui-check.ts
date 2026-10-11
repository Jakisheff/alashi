import assert from 'node:assert/strict'
import { projectDonkeyRisk, type DonkeyRiskInput, type DonkeyReceipt } from '../src/devnet/donkey/model.ts'
import { decodeCandidateDonkeySnapshot, DONKEY_CANDIDATE_SOURCE } from '../src/devnet/donkey/candidate.ts'
import { ChainApiError, CHAIN_PROGRAM } from '../src/devnet/client.ts'

// Isolated local fixtures only; no chain call, signing, real game or owner session.
const game = 'GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b'
const faction = '8RzVGTdFCqXGoyASbtStHpbin6gi6jcSH6gxQcWPHnqB'
const other = 'GaU1akfgaMRwqQZDZFh35g8EacCTUdZQMNBRxqzeyxtD'
const signature = '1'.repeat(64), secondSignature = '2' + '1'.repeat(87)
const base: DonkeyRiskInput = {
  source: 'confirmed-chain', contract: 'verified', game, faction, epoch: 2, round: 2,
  phase: 'Action', cashUnits: '5000000', goods: 0, alive: true, actionUsed: false, oracleReady: true,
  marketPriceUnits: '12000000', journalComplete: true, journalThroughSlot: 101, events: [],
}
const attempt: DonkeyReceipt = {
  kind: 'attempt', epoch: 2, game, faction, round: 2, attemptKey: 'local-attempt-key',
  id: `${signature}:0`, signature, slot: 100, logIndex: 0, journalIndex: 0, feeUnits: '5000000',
}
const success: DonkeyReceipt = { ...attempt, kind: 'resolution', signature: secondSignature, id: `${secondSignature}:0`, logIndex: 0, journalIndex: 1, slot: 101, reason: 0, success: true, goods: 5 }
const empty: DonkeyReceipt = { ...success, success: false, goods: 0 }
const abort: DonkeyReceipt = { ...empty, reason: 1 }
let checks = 0
const check = (condition: unknown, message: string) => { assert.ok(condition, message); checks++ }
const project = (events: readonly DonkeyReceipt[], patch: Partial<DonkeyRiskInput> = {}) => projectDonkeyRisk({ ...base, ...patch, events })
const failed = (events: readonly DonkeyReceipt[], patch: Partial<DonkeyRiskInput> = {}) => {
  const v = project(events, patch)
  check(v.kind === 'unavailable' && !v.attempt && !v.resolution, 'Invalid evidence must not leak fee, result or transaction proof')
}
check(project([]).kind === 'eligible', 'Exactly five alashi permits the proposed attempt')
check(project([attempt]).kind === 'pending', 'A confirmed commitment is not yet a win or loss')
check(project([attempt], { phase: 'Law', actionUsed: true }).kind === 'pending', 'Pending remains pending in Law')
for (const [receipt, kind] of [[success, 'success'], [empty, 'empty'], [abort, 'abort']] as const) {
  const result = project([attempt, receipt])
  check(result.kind === kind && result.attempt?.feeUnits === '5000000', 'Resolution keeps the fee on either result')
  check(project([attempt, { ...attempt }, receipt, { ...receipt }]).kind === kind, 'Repeated delivery is idempotent')
  check(project([attempt, receipt], { phase: 'Finished', actionUsed: true }).kind === kind, 'Historical receipt survives current phase changes')
}
check(project([attempt, abort]).detail.includes('no random draw'), 'Oracle abort is not a random 40% loss')
check(project([attempt, empty]).detail.includes('No refund or same-turn retry'), 'Empty costs fee and turn')
check(project([attempt]).detail.includes('goods are pending'), 'No goods appear during commitment')
check(project([attempt, success]).detail.includes('after Law reveal'), 'Goods arrive at reveal')
// Backward seek is a prefix supplied by the caller, without stateful outcome carryover.
check(project([attempt, success]).kind === 'success' && project([attempt]).kind === 'pending' && project([]).kind === 'eligible', 'Backseek removes future result and commitment')
check(project([attempt, { ...success, faction: other }]).kind === 'pending', 'Another player cannot resolve selected player')
check(project([{ ...attempt, faction: other }, { ...success, faction: other }]).kind === 'eligible', 'Foreign player history is not shown')
check(project([attempt, { ...success, game: other }]).kind === 'pending', 'Another Game result cannot resolve this attempt')
check(project([attempt, { ...success, round: 3 }]).kind === 'pending', 'Another round cannot resolve this attempt')
failed([success])
failed([success, attempt])
failed([attempt, { ...success, attemptKey: 'different-attempt' }])
failed([attempt, success, { ...success, id: `${secondSignature}:1`, logIndex: 1, journalIndex: 2 }])
failed([attempt, { ...attempt, id: `${signature}:2`, logIndex: 2, journalIndex: 2 }])
failed([attempt, success, { ...success, goods: 0 }])
failed([attempt, { ...abort, success: true, goods: 5 }])
failed([attempt, { ...success, goods: 4 }])
failed([attempt, { ...empty, goods: 5 }])
failed([{ ...attempt, feeUnits: '1000000' }])
failed([attempt, { ...success, reason: 2 } as unknown as DonkeyReceipt])
failed([attempt, { ...success, signature: 'invalid', id: 'invalid:1' }])
failed([attempt, { ...success, id: 'not-signature-log' }])
failed([attempt, { ...success, slot: 102 }])
failed([attempt, { ...success, slot: 99 }])
failed([attempt, { ...success, journalIndex: 0 }])
failed([attempt, { ...success, logIndex: -1 }])
failed([attempt, { ...success, signature, id: `${signature}:1`, logIndex: 1 }])
failed([attempt], { journalComplete: false })
failed([attempt, success], { contract: 'draft' })
failed([attempt, success], { epoch: 3 })
failed([attempt], { game: 'invalid' })
failed([attempt], { round: 0 })
failed([attempt], { round: 7 })
// Same-slot, different-signature ordering comes from the supplied journal, never lexicographic sort.
const tx = { ...success, signature: secondSignature, id: `${secondSignature}:0`, slot: 100, logIndex: 0 }
check(project([attempt, tx]).kind === 'success', 'A later same-slot transaction resolves in journal order')
failed([tx, attempt])
for (const epoch of [0, 1]) {
  const v = project([attempt, success], { epoch, contract: 'draft' })
  check(v.kind === 'legacy' && !v.attempt && v.detail.includes('1 good for 1'), 'Legacy deterministic purchase is never a lottery win')
}
for (const [patch, reason] of [
  [{ phase: 'Market' }, 'phase'], [{ round: 6 }, 'finalRound'],
  [{ cashUnits: '4999999' }, 'cash'], [{ goods: 65531 }, 'overflow'],
  [{ actionUsed: true }, 'acted'], [{ oracleReady: false }, 'oracle'],
  [{ alive: false }, 'inactive'], [{ alive: null }, 'unknown'],
  [{ cashUnits: null }, 'unknown'], [{ cashUnits: '18446744073709551616' }, 'unknown'],
  [{ goods: null }, 'unknown'], [{ actionUsed: null }, 'unknown'], [{ oracleReady: null }, 'unknown'],
] as [Partial<DonkeyRiskInput>, string][]) {
  const v = project([], patch)
  check(v.kind === 'ineligible' && v.reasons.includes(reason as typeof v.reasons[number]), `Eligibility: ${reason}`)
}
check(project([], { goods: 65530 }).kind === 'eligible', 'Exact inventory boundary accepted')
check(project([], { cashUnits: '18446744073709551615' }).kind === 'eligible', 'u64 cash handled without number rounding')
check(project([], { marketPriceUnits: null }).marketPriceUnits === null, 'Missing dynamic price is not invented')
check(project([], { marketPriceUnits: '-1' }).marketPriceUnits === null, 'Invalid quote remains unavailable')
check(project([attempt, success], { source: 'local-fixture', contract: 'draft' }).kind === 'success', 'Only explicitly local fixtures may exercise draft results')
check(!JSON.stringify(project([attempt, success])).includes('private'), 'Presentation output has no private owner fields')

// Exact source-candidate public JSON, never a deployed API response.
const wireAttempt = { type: 'donkey_attempted', game, faction, round: 2, fee: '5000000', id: `${signature}:0`, signature, slot: 100, log_index: 0, event_index: 0, block_time: null }
const wireResolution = { ...wireAttempt, type: 'donkey_resolved', signature: secondSignature, id: `${secondSignature}:0`, slot: 101, success: true, goods: 5, reason: 0 }
const wire = {
  ok: true, cluster: 'devnet', commitment: 'confirmed', program_id: CHAIN_PROGRAM, game_pda: game,
  game: { id: '1', epoch: 2, phase: 'Law', round: 2, phase_ends_at: 1791720000, settled: false },
  factions: [{ pda: faction, wallet: other, name: 'LOCAL FIXTURE', cash: '0', hard: '0', alive: true, goods: 0, influence: 0, vote: 'Abstain' }],
  snapshot_slot: 101, journal_through_slot: 101, history_complete: true,
  fetched_at: '2026-10-11T13:48:00Z', events: [wireAttempt, wireResolution],
}
const candidate = decodeCandidateDonkeySnapshot(wire, game)
check(candidate.contract === 'candidate-only' && candidate.source === DONKEY_CANDIDATE_SOURCE, 'Candidate decode cannot silently mark the public contract verified')
check(candidate.receipts[1].journalIndex === 1 && candidate.snapshot.events[1].index === 0, 'Normalize full journal position, not transaction event_index')
check(project(candidate.receipts).kind === 'success', 'Candidate successful resolution reaches presentation')
for (const [reason, success, goods, kind] of [[0, false, 0, 'empty'], [1, false, 0, 'abort']] as const) {
  const parsed = decodeCandidateDonkeySnapshot({ ...wire, events: [wireAttempt, { ...wireResolution, reason, success, goods }] }, game)
  check(project(parsed.receipts).kind === kind, 'Candidate verified empty and technical abort stay distinct')
}
const badWire = (value: unknown) => {
  assert.throws(() => decodeCandidateDonkeySnapshot(value, game), (e: unknown) => e instanceof ChainApiError && e.code === 'invalid'); checks++
}
for (const patch of [{ fee: 5000000 }, { fee: '05000000' }, { fee: '1000000' }, { fee: undefined }, { round: 6 }, { faction: other }, { game: other }])
  badWire({ ...wire, events: [{ ...wireAttempt, ...patch }, wireResolution] })
for (const patch of [{ reason: '0' }, { reason: 2 }, { reason: undefined }, { reason: 1, success: true }, { success: 'true' }, { goods: 4 }, { goods: undefined }, { success: false, goods: 5 }])
  badWire({ ...wire, events: [wireAttempt, { ...wireResolution, ...patch }] })
badWire({ ...wire, commitment: 'processed' })
badWire({ ...wire, program_id: other })
badWire({ ...wire, game: { ...wire.game, epoch: 1 } })
badWire({ ...wire, events: [wireResolution, wireAttempt] })
badWire({ ...wire, journal_through_slot: 100 })
const partial = decodeCandidateDonkeySnapshot({ ...wire, history_complete: false }, game)
check(project(partial.receipts, { journalComplete: partial.snapshot.complete }).kind === 'unavailable', 'Partial source candidate cannot prove a result')
const legacy = decodeCandidateDonkeySnapshot({ ...wire, game: { ...wire.game, epoch: 1 }, events: [{ ...wireAttempt, type: 'donkey_bought', price: '1000000' }] }, game)
check(legacy.receipts.length === 0 && legacy.snapshot.events[0].type === 'donkey_bought', 'Historical deterministic event remains outside risk receipts')
console.log(`donkey-risk-ui: ${checks} semantic checks passed; local fixtures only, no API/chain/owner operations`)

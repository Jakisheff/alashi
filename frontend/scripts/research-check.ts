import assert from 'node:assert/strict'
import { LiveApiError } from '../src/live/api/client.ts'
import { localPublication, researchWorkspace, researchLibrary, researchError } from '../src/strategies/research-client.ts'

const owner = 'RegisteredOwner', peer = 'InvitedPeer', hash = (digit: string) => digit.repeat(64)
const forecast = { probability_bps: 6000, reason: 'The narrower instruction could make the audience clearer.', submitted_at: 101 }
const round = { comparison_id: hash('a'), hypothesis: 'A narrower audience instruction makes the ad clearer.', decision_criterion: 'Owner judges it clearer against the fixed rubric.', is_owner: true, participants: [owner, peer], submitted_count: 1, missing_count: 1, created_at: 100, closed_at: null as number | null, own_forecast: forecast, forecasts: null as null | (typeof forecast & { wallet: string })[], summary: null as null | { submitted_count: number; missing_count: number; min_probability_bps: number; max_probability_bps: number; mean_probability_bps: number; mean_rounding: string }, forecast_subject: 'candidate_satisfies_decision_criterion' }
const card = { id: hash('b'), title: 'Compare one instruction', mechanism: 'Hold the task fixed and change one instruction.', task: 'Choose an instruction for an advertisement.', conditions: 'One paired model run on one static brief.', result: 'The owner preferred the candidate.', limitations: 'No customer behavior was measured.', evidence_status: 'model', license: 'CC0-1.0', created_at: 200 }
const base = { ok: true, backlog: [{ id: hash('c'), client_id: 'idea-1', title: 'Narrow the audience', hypothesis: round.hypothesis, priority: 'now', comparison_id: hash('a'), source_card_id: null, created_at: 90, updated_at: 91 }], rounds: [round], applied: null as null | { comparison_id: string; attempt_id: string; version_id: string; applied_at: number }, digest: { fingerprint: hash('d'), day: '2026-10-26', timezone: 'Asia/Almaty', unread: true, items: [] as unknown[], next_actions: [{ kind: 'forecast', comparison_id: hash('a'), title: 'Submit your forecast' }] }, library: [] as typeof card[] }
assert.equal(researchWorkspace(base, owner).rounds[0].own_forecast?.probability_bps, 6000)
assert.equal(researchWorkspace(base, owner).rounds[0].forecasts, null)
const peerView = structuredClone(base); peerView.rounds[0].is_owner = false; peerView.rounds[0].own_forecast = null as unknown as typeof forecast
assert.equal(researchWorkspace(peerView, peer).rounds[0].own_forecast, null)
const closed = structuredClone(base)
Object.assign(closed.rounds[0], { closed_at: 105, forecasts: [{ wallet: owner, ...forecast }], summary: { submitted_count: 1, missing_count: 1, min_probability_bps: 6000, max_probability_bps: 6000, mean_probability_bps: 6000, mean_rounding: 'floor' } })
closed.applied = { comparison_id: hash('a'), attempt_id: hash('e'), version_id: hash('f'), applied_at: 120 }
closed.digest.items.push({ kind: 'forecasts_closed', comparison_id: hash('a'), attempt_id: null, title: 'Forecasts revealed', at: 105, summary: closed.rounds[0].summary })
closed.library.push(card)
assert.equal(researchWorkspace(closed, owner).rounds[0].summary?.mean_probability_bps, 6000)
assert.equal(researchWorkspace(closed, owner).digest.items[0].summary?.missing_count, 1)
for (const mutate of [
  (value: typeof base) => { value.rounds[0].forecasts = [{ wallet: peer, ...forecast }] },
  (value: typeof base) => { value.rounds[0].own_forecast.probability_bps = 10001 },
  (value: typeof base) => { value.rounds[0].participants = [peer] },
  (value: typeof base) => { value.rounds[0].missing_count = 10 },
  (value: typeof base) => { value.rounds[0].forecast_subject = 'Unrelated probability' },
  (value: typeof base) => { value.digest.fingerprint = 'stale' },
]) { const next = structuredClone(base); mutate(next); assert.throws(() => researchWorkspace(next, owner), LiveApiError) }
const falseMean = structuredClone(closed); falseMean.rounds[0].summary!.mean_probability_bps = 5000
assert.throws(() => researchWorkspace(falseMean, owner), LiveApiError, 'summary cannot contradict disclosed forecasts')
const publicView = researchLibrary({ cards: [{ ...card, instructions: 'PRIVATE', owner }] })
assert.equal('instructions' in publicView[0], false)
assert.equal('owner' in publicView[0], false)
assert.throws(() => researchLibrary({ cards: [{ ...card, evidence_status: 'proven_real_world' }] }), LiveApiError)
assert.throws(() => researchLibrary({ cards: [{ ...card, license: 'All rights reserved' }] }), LiveApiError)
const imported = localPublication({ ...card, client_id: 'local-public-card-1', consent: true, journal: { private: 'do not send' }, provider_key: 'never import' })
assert.equal('consent' in imported, false, 'file import is not publication permission')
assert.equal('journal' in imported, false)
assert.equal('provider_key' in imported, false)
assert.equal(imported.client_id, 'local-public-card-1')
assert.throws(() => localPublication({ ...card, client_id: 'bad id with spaces' }), LiveApiError)
assert.throws(() => localPublication({ ...card, client_id: 'local-public', title: 'Я'.repeat(51) }), LiveApiError, 'backend UTF-8 byte limit is respected')
assert.match(researchError(new LiveApiError('forecast_round_open')), /Close and reveal/)
assert.match(researchError(new LiveApiError('digest_changed')), /summary changed/)
console.log('PASS: sealed role projections, disclosed summary integrity, applied strategy, digest CAS, bounded public cards and local-card consent isolation')

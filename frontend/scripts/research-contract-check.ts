import assert from 'node:assert/strict'
import { researchWorkspace, researchLibrary } from '../src/strategies/research-client.ts'

let input = ''
for await (const chunk of process.stdin) input += chunk
const fixture = JSON.parse(input)
assert.ok(Array.isArray(fixture.states) && fixture.states.length > 0)
let sealed = 0, disclosed = 0, applied = 0
for (const entry of fixture.states) {
  const wallet = entry.wallet ?? fixture.owner, raw = entry.response ?? entry, view = researchWorkspace(raw, wallet)
  for (const round of view.rounds) {
    const source = raw.rounds.find((item: { comparison_id: string }) => item.comparison_id === round.comparison_id)
    for (const field of ['prediction', 'actual_diff', 'profile', 'instructions', 'plan', 'attempts', 'reports']) assert.equal(field in source, false, `round must not expose ${field}`)
    if (round.closed_at === null) { sealed++; assert.equal(source.forecasts, null); assert.equal(source.summary, null) }
    else { disclosed++; assert.equal(round.forecasts?.length, round.submitted_count); assert.ok(round.summary); assert.equal(round.summary?.submitted_count, round.submitted_count) }
    if (round.own_forecast) assert.ok(round.participants.includes(wallet))
  }
  if (view.applied) { applied++; assert.match(view.applied.version_id, /^[a-f0-9]{64}$/) }
  assert.equal(view.digest.timezone, 'Asia/Almaty')
  assert.match(view.digest.fingerprint, /^[a-f0-9]{64}$/)
}
const cards = researchLibrary(fixture.public_response)
for (const card of fixture.public_response.cards) for (const field of ['owner', 'record', 'comparison_id', 'attempt_id', 'manifest', 'journal', 'plan', 'brief', 'instruction', 'calls', 'reports']) assert.equal(field in card, false, `public card must not expose ${field}`)
assert.ok(cards.every(card => card.license === 'CC0-1.0'))
console.log(`PASS: actual research HTTP responses parsed (${fixture.states.length} states, ${sealed} sealed rounds, ${disclosed} disclosed rounds, ${applied} applied defaults, ${cards.length} public cards); role projections and public-field boundaries verified`)

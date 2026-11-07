// Receives actual Rust API serialization on stdin. No synthetic server projection.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { comparisons, instructionDiff, projectComparison, type Run, type Version } from '../src/strategies/client.ts'
import { LiveApiError } from '../src/live/api/client.ts'

const { owner, record, states } = JSON.parse(readFileSync(0, 'utf8'))
assert.deepEqual(states.map((state: { stage: string }) => state.stage), ['prepared', 'started', 'completed', 'decided'])
for (const { stage, response } of states) {
  assert.equal(response.ok, true, stage)
  const raw = response.comparison ?? response.comparisons[0]
  const comparison = projectComparison(raw, owner, record)
  assert.equal(comparisons({ comparisons: [raw] }, owner, record).length, 1, stage)
  assert.equal(comparison.attempts.length, stage === 'prepared' ? 0 : 1, stage)
  if (stage === 'prepared') continue
  const attempt = comparison.attempts[0]
  const version = (run: Run): Version => ({ id: run.version, published: false, reuseOwners: 0, content: { owner, parent: null, title: '', change: '', plan: run.plan } })
  assert.deepEqual(instructionDiff(version(attempt.baseline), version(attempt.candidate)), comparison.actual_diff, stage)
  if (response.attempt) assert.deepEqual(projectComparison({ ...raw, attempts: [response.attempt] }, owner, record).attempts[0], attempt, stage)
  if (stage === 'completed' || stage === 'decided') {
    assert.equal(attempt.baseline.status, 'completed')
    assert.equal(attempt.candidate.status, 'completed')
  }
  assert.equal(attempt.decision?.outcome ?? null, stage === 'decided' ? 'inconclusive' : null)
  const missing = structuredClone(raw)
  for (const side of ['baseline', 'candidate']) {
    for (const task of missing.attempts[0][side].plan) delete task.instruction
  }
  assert.throws(() => projectComparison(missing, owner, record), LiveApiError, 'redacted plans cannot substantiate the private instruction diff')
  const tampered = structuredClone(raw)
  tampered.actual_diff.after += ' Unverified change.'
  assert.throws(() => projectComparison(tampered, owner, record), LiveApiError, 'an unverified diff is rejected')
}
console.log('PASS: actual server responses parse from prepared comparison through owner decision; missing instructions and tampered diff are rejected')

import assert from 'node:assert/strict'
import { comparisonFixture } from './comparison-fixture.mjs'
import { comparisonDecisionOptions, comparisons, instructionDiff, projectComparison, projectVersion, strategyRequest, errorText } from '../src/strategies/client.ts'
import { LiveApiError } from '../src/live/api/client.ts'

const fixture = comparisonFixture(), [baseline, candidate] = fixture.versions.map(projectVersion), raw = fixture.comparison
const copy = () => structuredClone(raw)
assert.deepEqual(instructionDiff(baseline, candidate), raw.actual_diff, 'private own versions support an instruction-only change')
assert.equal(instructionDiff(baseline, baseline), null)
const modified = structuredClone(candidate)
modified.content.plan[0].depends_on = ['other']
assert.equal(instructionDiff(baseline, modified), null, 'dependency edits cannot masquerade as an instruction change')
modified.content.plan[0].depends_on = []; modified.content.owner = 'OtherOwner'
assert.equal(instructionDiff(baseline, modified), null)
const additional = structuredClone(candidate)
additional.content.plan.push({id:'extra',kind:'model',depends_on:[],instruction:'Extra',tool:null})
assert.equal(instructionDiff(baseline, additional), null)
const twoA = structuredClone(baseline), twoB = structuredClone(candidate)
twoA.content.plan.push({id:'second',kind:'model',depends_on:[],instruction:'One',tool:null})
twoB.content.plan.push({id:'second',kind:'model',depends_on:[],instruction:'Two',tool:null})
assert.equal(instructionDiff(twoA,twoB),null,'multiple changed instructions are rejected')
const comparison = projectComparison(raw,fixture.owner)
assert.equal(comparisons({ok:true,comparisons:[raw]},fixture.owner).length,1)
assert.throws(()=>projectComparison(raw,fixture.owner,'9'.repeat(64)),LiveApiError,'the owner response must belong to the current agent record')
assert.equal(comparison.attempts[0].candidate.result?.checks.passed,true)
const redacted = copy()
for (const run of [redacted.attempts[0].baseline, redacted.attempts[0].candidate]) {
  run.plan = run.plan.map(({ instruction: _instruction, ...task }) => task)
}
assert.throws(()=>projectComparison(redacted,fixture.owner),LiveApiError,'private comparisons require the frozen instructions, not the public run projection')
for (const mutate of [
  (v: typeof raw) => {v.content.owner='OtherOwner'},
  (v: typeof raw) => {v.content.profile.temperature=1},
  (v: typeof raw) => {v.content.baseline_version=v.content.candidate_version},
  (v: typeof raw) => {v.attempts[0].candidate.owner='OtherOwner'},
  (v: typeof raw) => {v.attempts[0].candidate.record='9'.repeat(64)},
  (v: typeof raw) => {v.attempts[0].candidate.challenge='9'.repeat(64)},
  (v: typeof raw) => {v.attempts[0].candidate.budget=1},
  (v: typeof raw) => {v.attempts[0].candidate.plan[0].instruction='Unrelated instruction'},
  (v: typeof raw) => {v.attempts[0].candidate.brief={...v.attempts[0].candidate.brief,task:'Different task'}},
]) {const value=copy(); mutate(value); assert.throws(()=>projectComparison(value,fixture.owner),LiveApiError)}
const attempt = structuredClone(comparison.attempts[0]), now = Math.floor(Date.now()/1000)
assert.deepEqual(comparisonDecisionOptions(attempt,now),['keep_baseline','adopt_candidate','inconclusive'])
attempt.candidate.status='running'
assert.deepEqual(comparisonDecisionOptions(attempt,now),[],'cannot decide while a side can still run')
attempt.candidate.status='quality_failed'
assert.deepEqual(comparisonDecisionOptions(attempt,now),['keep_baseline','inconclusive'],'failure does not force an adoption claim')
attempt.baseline.unknown_usage=true
assert.deepEqual(comparisonDecisionOptions(attempt,now),['inconclusive'])
attempt.candidate.calls.deliver.status='reserved'
assert.deepEqual(comparisonDecisionOptions(attempt,now),[],'in-flight reserved calls block a decision')
const failed = copy()
failed.attempts[0].candidate.status='quality_failed'; failed.attempts[0].candidate.result=null
failed.attempts[0].decision={outcome:'inconclusive',reason:'The candidate did not finish.',at:now}
assert.equal(projectComparison(failed,fixture.owner).attempts[0].decision?.outcome,'inconclusive')
failed.attempts[0].decision.outcome='adopt_candidate'
assert.throws(()=>projectComparison(failed,fixture.owner),LiveApiError)

const originalFetch=globalThis.fetch
try {
  let request:RequestInit|undefined
  globalThis.fetch=async (_url,init)=>{request=init;return new Response(JSON.stringify({ok:true,comparison:raw}),{headers:{'Content-Type':'application/json'}})}
  await strategyRequest('/agents/'+fixture.record+'/owner/comparisons',{hypothesis:'A written prediction'})
  assert.equal(request?.method,'POST');assert.equal(request?.credentials,'same-origin');assert.equal(request?.redirect,'error')
  assert.equal(JSON.parse(request?.body as string).hypothesis,'A written prediction')
  globalThis.fetch=async()=>new Response(JSON.stringify({ok:false,error:'comparison_requires_one_instruction_change'}),{status:400})
  await assert.rejects(()=>strategyRequest('/private'),(error:unknown)=>error instanceof LiveApiError&&error.code==='comparison_requires_one_instruction_change')
  assert.match(errorText(new LiveApiError('comparison_provider_mismatch')),/frozen/)
} finally {globalThis.fetch=originalFetch}
console.log('PASS: private instruction diff, structural guards, owner/record/brief isolation, frozen settings, failed/unknown/in-flight decision limits and request error handling')

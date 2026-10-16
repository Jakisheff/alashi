import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { timeline, point } from '../src/strategies/timeline.ts'
import type { Run } from '../src/strategies/client.ts'
const run = { created_at: 100, plan: [
  { id: 'audience', kind: 'model', depends_on: [] },
  { id: 'angle', kind: 'model', depends_on: [] },
  { id: 'deliver', kind: 'model', depends_on: ['audience', 'angle'] },
], calls: Object.fromEntries(['audience', 'angle', 'deliver'].map(task => [task, { task, started_at: 100, finished_at: 100, quality_passed: true }])) } as unknown as Run
const stages = timeline(run)
assert.equal(stages[0].start, stages[1].start, 'independent tasks must overlap')
assert.ok(stages[2].start > stages[0].end! && stages[2].start > stages[1].end!, 'delivery must wait for both returned reports, even when server timestamps share one second')
assert.ok(stages.every(s => s.end! > s.work && s.work > s.start), 'preparation, work and return must remain visible')
assert.deepEqual(point([[0, 0], [1, 0], [2, 1], [3, 1]], 0), [0, 0])
assert.deepEqual(point([[0, 0], [1, 0], [2, 1], [3, 1]], 1), [3, 1])
run.calls.angle.finished_at = null
assert.equal(timeline(run)[1].end, null, 'missing completion must not invent a returned report')
console.log('PASS: parallel task staging, causal handoff, route endpoints and pending completion')
run.calls.angle.finished_at = 100
run.result = {artifact: {action: 'sell', params: {units: 2}}, checks: {passed: true}, finished_at: 101, trade_receipt: {mode: 'http', game: '1', event_id: 'live-1', action: 'sell', params: {units: 2}, cash_delta: '22000000', goods_delta: -2, confirmed_at: 101}}
const trade = timeline(run)
assert.equal(trade.at(-1)?.task, 'sell confirmed')
assert.equal(trade.at(-1)?.route, 0)
assert.ok(trade.at(-1)!.start > trade.at(-2)!.end!, 'a game receipt cannot arrive before the task report on the staged map')
run.result = null
assert.equal(timeline(run).length, 3, 'no game receipt means no invented trade journey')
console.log('PASS: confirmed trade follows task delivery; pending trade stays absent')

for (const [copy, original] of [['terrain.png', 'assets/alashi-terrain.png'], ['degenie.png', 'assets/degenie-atlas.png'], ['map-layout.json', 'map-layout.json']]) {
  const hash = (path: URL) => createHash('sha256').update(readFileSync(path)).digest('hex')
  assert.equal(hash(new URL(`../public/strategies/${copy}`, import.meta.url)), hash(new URL(`../../experiments/alashi-watch/rts-overview/${original}`, import.meta.url)), 'owner-approved visual reference must remain exact')
}
console.log('PASS: original terrain, Degenie atlas and route geometry preserved')

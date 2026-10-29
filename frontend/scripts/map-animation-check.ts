import assert from 'node:assert/strict'
import { test } from 'node:test'
import { chiefSprite, REPLAY_TAIL_SECONDS, workerMotion } from '../src/strategies/atlasMotion.ts'
import type { Stage } from '../src/strategies/timeline.ts'

const stage: Stage = { task: 'sell', route: 0, start: 9, work: 9.8, end: 13.2, ok: true, tool: false }
const sprite = ({ clip, frame }: { clip: number; frame: number }) => ({ clip, frame })

test('work begins with preparation, independent of map time and worker idle offset', () => {
  assert.deepEqual(sprite(workerMotion(stage, stage.work, .3)), { clip: 1, frame: 0 })
  assert.deepEqual(sprite(workerMotion({ ...stage, start: 109, work: 109.8, end: 113.2 }, 109.8, .1)), { clip: 1, frame: 0 })
  assert.deepEqual(sprite(workerMotion(stage, 10.2, .3)), { clip: 1, frame: 8 })
})

test('successful handoff begins accepted at its first pose and ends in idle', () => {
  const handoff = stage.end! - .16
  assert.deepEqual(sprite(workerMotion(stage, handoff, .3)), { clip: 2, frame: 0 })
  assert.equal(workerMotion(stage, handoff + .9).clip, 2, 'the full accepted gesture must finish before idle')
  assert.deepEqual(sprite(workerMotion(stage, handoff + 1.234)), { clip: 0, frame: 0 })
  assert.equal(workerMotion(stage, handoff + 3).clip, 0, 'accepted must not loop')
})

test('chief celebrates completion once, then the replay tail settles in idle', () => {
  const completed = 13.2
  assert.deepEqual(chiefSprite(completed, completed), { clip: 2, frame: 0 })
  assert.equal(chiefSprite(completed, completed + .9).clip, 2)
  assert.deepEqual(chiefSprite(completed, completed + REPLAY_TAIL_SECONDS), { clip: 0, frame: 1 })
  assert.equal(chiefSprite(completed, completed + 3).clip, 0)
  assert.equal(chiefSprite(null, completed).clip, 0, 'unfinished teams cannot invent acceptance')
  assert.equal(chiefSprite(completed, completed - .1).clip, 0, 'acceptance cannot precede the completion boundary')
  assert.deepEqual(chiefSprite(0, 1.233), { clip: 0, frame: 0 }, 'the exact accepted endpoint must not read the next atlas row')
})

test('reverse seek and replay reconstruct the same pose without keeping completion state', () => {
  const work = workerMotion(stage, 10.2, .2), accepted = workerMotion(stage, 13.44, .2)
  workerMotion(stage, 20, .2)
  assert.deepEqual(workerMotion(stage, 13.44, .2), accepted)
  assert.deepEqual(workerMotion(stage, 10.2, .2), work)
  assert.deepEqual(sprite(workerMotion(stage, stage.work, .2)), { clip: 1, frame: 0 })
})

test('travel, failed work and reduced motion retain their original meaning', () => {
  assert.equal(workerMotion(stage, stage.start).progress, 0)
  assert.equal(workerMotion(stage, stage.work).progress, 1)
  assert.ok(Math.abs(workerMotion(stage, stage.start + .48).progress - .5) < 1e-10)
  assert.ok(Math.abs(workerMotion(stage, stage.end! - .48).progress - .5) < 1e-10, 'the return path retains its midpoint')
  const returning = workerMotion(stage, 12.8)
  assert.equal(returning.clip, 0); assert.equal(returning.carrying, true)
  assert.ok(returning.progress > 0 && returning.progress < 1)
  assert.equal(workerMotion({ ...stage, ok: false }, 13.2).clip, 0)
  assert.deepEqual(sprite(workerMotion(stage, 13.44, 0, true)), { clip: 2, frame: 14 })
  assert.equal(workerMotion(stage, 13.44, 0, true).progress, 0)
  assert.equal(workerMotion({ ...stage, end: null }, 30, 0, true).progress, 1)
})

test('idle offsets stay in idle, and returning starts without retaining the work pose', () => {
  assert.deepEqual(workerMotion(undefined, 0), { progress: 0, carrying: false, clip: 0, frame: 0 })
  assert.deepEqual(sprite(workerMotion(undefined, 0, .4)), { clip: 0, frame: 2 })
  assert.deepEqual(sprite(workerMotion(undefined, 20, 0, true)), { clip: 0, frame: 0 })
  const working = workerMotion({ ...stage, end: null }, stage.work + 1.153)
  assert.deepEqual(sprite(working), { clip: 1, frame: 0 }, 'ongoing work repeats from its own preparation')
  assert.equal(working.carrying, false, 'a report is carried only on the return trip')
  const returning = workerMotion(stage, stage.end! - .8)
  assert.equal(returning.clip, 0); assert.equal(returning.progress, 1); assert.equal(returning.carrying, true)
})

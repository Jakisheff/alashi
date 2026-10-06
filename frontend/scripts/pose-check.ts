// Run: npm run check:pose. One-shot clips must start and end on the idle pose, with finite values.
import assert from 'node:assert/strict'
import { CLIP_SECONDS, poseAt, type GenieClip } from '../src/genie/pose.ts'

const time = 2.3
for (const clip of ['act', 'accepted', 'rejected'] as GenieClip[]) {
  const idle = poseAt('idle', 0, time)
  for (const t of [0, CLIP_SECONDS[clip]]) {
    assert.deepEqual(poseAt(clip, t, time), idle, `${clip} at t=${t} is not the idle pose`)
  }
  for (let t = 0; t <= CLIP_SECONDS[clip]; t += 0.05) {
    for (const [k, v] of Object.entries(poseAt(clip, t, time))) assert.ok(Number.isFinite(v), `${clip}.${k} at ${t}`)
  }
}
console.log('pose check ok')

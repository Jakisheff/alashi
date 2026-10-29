import { smooth, type Stage } from './timeline.ts'

export type AtlasSprite = { clip: 0 | 1 | 2; frame: number }
const DURATIONS = [4.033, 1.133, 1.233]
export const REPLAY_TAIL_SECONDS = DURATIONS[2] + .2

function sampleAtlas(clip: AtlasSprite['clip'], time: number, reduced: boolean): AtlasSprite {
  // Accepted is a complete one-shot; its next pose starts idle at time zero.
  if (clip === 2 && time >= DURATIONS[2]) { time -= DURATIONS[2]; clip = 0 }
  const frame = reduced ? clip === 2 ? 14 : 0 : Math.floor((time % DURATIONS[clip]) / DURATIONS[clip] * 24)
  return { clip, frame }
}

export function workerMotion(stage: Stage | undefined, time: number, idleOffset = 0, reduced = false) {
  let progress = 0, clip: AtlasSprite['clip'] = 0, carrying = false, elapsed = time + idleOffset
  if (stage) {
    if (time < stage.work) progress = smooth((time - stage.start - .16) / .64)
    else if (stage.end === null || time < stage.end - .8) { progress = 1; clip = 1; elapsed = time - stage.work }
    else if (time < stage.end - .16) { progress = 1 - smooth((time - stage.end + .8) / .64); carrying = stage.ok }
    else if (stage.ok) { clip = 2; elapsed = time - (stage.end - .16) }
    if (reduced) progress = stage.end === null ? 1 : 0
  }
  return { progress, carrying, ...sampleAtlas(clip, elapsed, reduced) }
}

export function chiefSprite(completedAt: number | null, time: number, reduced = false): AtlasSprite {
  return completedAt !== null && time >= completedAt
    ? sampleAtlas(2, time - completedAt, reduced) : sampleAtlas(0, time, reduced)
}

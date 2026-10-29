import { playbackFrame } from '../playbackFrame.ts'
import { REACTION_REDUCED_FRAME, REACTION_SECONDS, type ReactionPreview } from './definitions.ts'

export type ReactionClock = {
  key: string
  seek: number | null
  time: number
  idleTime: number
  base: number
  reported: number
  finished: boolean
  shown: number | null
}

const clampTime = (value: number) => Number.isFinite(value) ? Math.max(0, Math.min(REACTION_SECONDS, value)) : 0

/** A new reaction starts at its current idle pose; seeking is edge-triggered. */
export function prepareReactionFrame(clock: ReactionClock, reaction: ReactionPreview) {
  const key = `${reaction.kind}:${reaction.take}`
  const seek = reaction.seek === null ? null : clampTime(reaction.seek)
  const restarted = clock.key !== key
  const sought = restarted || clock.seek !== seek
  if (restarted) {
    clock.key = key; clock.base = clock.idleTime; clock.time = 0; clock.reported = -1; clock.finished = false; clock.shown = null
  }
  if (sought) {
    clock.seek = seek
    if (seek !== null) { clock.time = seek; clock.finished = false; clock.reported = -1 }
  }
  return sought
}

/** Advance the source clock and select the pose without changing scene objects. */
export function advanceReactionFrame(clock: ReactionClock, reaction: ReactionPreview, elapsed: number, reduced: boolean, sought: boolean) {
  const speed = Number.isFinite(reaction.speed) ? Math.max(0, reaction.speed) : 1
  if (reaction.playing && !clock.finished && !sought) clock.time = clampTime(clock.time + elapsed * speed)
  clock.idleTime = clock.base + clock.time
  const selected = reduced && reaction.playing && clock.time < REACTION_SECONDS ? REACTION_REDUCED_FRAME[reaction.kind] : clock.time
  const shown = playbackFrame(clock.time, selected, clock.shown, reaction.playing, sought && reaction.seek !== null)
  clock.shown = shown
  return shown
}

/** Mark reports before calling the parent so synchronous callbacks see current state. */
export function shouldReportReactionTime(clock: ReactionClock) {
  if (clock.reported < 0 || Math.abs(clock.time - clock.reported) >= .09) { clock.reported = clock.time; return true }
  return false
}

/** Complete once; a near-end report still needs an exact final timestamp. */
export function completeReaction(clock: ReactionClock) {
  clock.finished = true
  if (clock.reported === REACTION_SECONDS) return false
  clock.reported = REACTION_SECONDS
  return true
}

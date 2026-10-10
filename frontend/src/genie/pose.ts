// Pure pose math for DeskGenie: clip + time -> pose. No React, no three.js.

export type GenieClip = 'idle' | 'act' | 'accepted' | 'rejected' | 'fuckOff'

export type Pose = {
  y: number // hover height
  tiltX: number // + leans forward
  tiltZ: number
  yaw: number
  rArmOut: number // shoulder abduction, + raises the arm sideways
  rArmFwd: number // shoulder flexion, + raises the arm forward
  rElbow: number
  lArmOut: number
  lArmFwd: number
  lElbow: number
  rGrip: number // 0 open hand, 1 fist
  lGrip: number
  rPoint: number // 1 keeps index + middle straight while the rest curl
  lPoint: number
  crank: number // blend into the two-hand wind-up gesture
  crankTurn: number
  rMiddle: number // 1 extends only the middle finger
  lMiddle: number
  browL: number // + raises
  browR: number
  browTilt: number // + worried (inner ends up), - scheming
  eyeOpen: number // blink: 1 open, 0 closed
  lid: number // upper lid coverage: 0 wide, ~0.3 smug, 1 shut
  lookX: number
  lookY: number
  smile: number // 1 grin, -1 frown
  tailSway: number
}

export const CLIP_SECONDS: Record<GenieClip, number> = {
  idle: 4,
  act: 1.1,
  accepted: 1.2,
  rejected: 1.6,
  fuckOff: 5.8,
}

export const CLIP_REDUCED_FRAME: Record<GenieClip, number> = {
  idle: 0, act: .45, accepted: .55, rejected: .95, fuckOff: 3.9,
}

const REST: Pose = {
  y: 0,
  tiltX: 0,
  tiltZ: 0,
  yaw: 0.35,
  rArmOut: 0.45,
  rArmFwd: 0.35,
  rElbow: 0.9,
  lArmOut: 0.4,
  lArmFwd: 0.3,
  lElbow: 0.8,
  rGrip: 0.25,
  lGrip: 0.25,
  crank: 0,
  crankTurn: 0,
  rMiddle: 0,
  lMiddle: 0,
  rPoint: 0,
  lPoint: 0,
  browL: 0,
  browR: 0.35, // asymmetric smirk is the character's default
  browTilt: 0,
  eyeOpen: 1,
  lid: 0.3,
  lookX: 0,
  lookY: 0,
  smile: 0.6,
  tailSway: 1,
}

type Key = [time: number, value: number]

const smooth = (x: number) => x * x * (3 - 2 * x)

// Keyframed offset with smoothstep easing; 0 outside the keyed range.
function track(keys: Key[], t: number): number {
  if (t <= keys[0][0] || t >= keys[keys.length - 1][0]) return 0
  for (let i = 1; i < keys.length; i++) {
    const [t1, v1] = keys[i]
    if (t <= t1) {
      const [t0, v0] = keys[i - 1]
      return v0 + (v1 - v0) * smooth((t - t0) / (t1 - t0))
    }
  }
  return 0
}

// Every idle frequency is a multiple of 2π/4 s, so the exported GLB idle clip loops seamlessly.
const W = (2 * Math.PI) / 4

function idle(time: number, motion: number): Pose {
  const blinkPhase = time % 4
  return {
    ...REST,
    y: 0.07 * Math.sin(time * 2 * W) * motion,
    tiltZ: 0.04 * Math.sin(time * W) * motion,
    tiltX: 0.03 * Math.sin(time * 2 * W - 0.6) * motion,
    rArmFwd: REST.rArmFwd + 0.08 * Math.sin(time * 2 * W - 0.9) * motion,
    lArmFwd: REST.lArmFwd + 0.08 * Math.sin(time * 2 * W - 1.1) * motion,
    eyeOpen: blinkPhase > 1.8 && blinkPhase < 1.94 ? 0.1 : 1,
    lookX: 0.25 * Math.sin(time * W) * motion,
    lookY: 0.1 * Math.sin(time * W + 1.2) * motion,
    tailSway: 1,
  }
}

// One-shot clips are offsets on top of idle that start and end at zero,
// so every clip returns to the idle pose without a snap.
type Offsets = Partial<Record<keyof Pose, Key[]>>

const OFFSETS: Record<Exclude<GenieClip, 'idle'>, Offsets> = {
  // Anatomical left fist stays palm-up; the right hand winds it before the reveal.
  fuckOff: {
    crank: [[0, 0], [0.75, 1], [5.0, 1], [5.8, 0]],
    crankTurn: [[0, 0], [1.0, 0], [3.9, Math.PI * 6], [5.8, Math.PI * 6]],
    rGrip: [[0, 0], [0.65, 0.75], [5.0, 0.75], [5.8, 0]],
    lGrip: [[0, 0], [0.65, 0.75], [5.0, 0.75], [5.8, 0]],
    rMiddle: [[0, 0], [1.65, 0], [2.3, 0.25], [3.0, 0.65], [3.9, 1], [5.0, 1], [5.8, 0]],
    browTilt: [[0, 0], [0.4, -0.12], [5.0, -0.12], [5.8, 0]],
    browR: [[0, 0], [0.4, 0.25], [5.0, 0.25], [5.8, 0]],
    lid: [[0, 0], [0.4, 0.12], [5.0, 0.12], [5.8, 0]],
    smile: [[0, 0], [0.4, 0.3], [5.0, 0.3], [5.8, 0]],
    lookY: [[0, 0], [0.75, -0.4], [3.6, -0.4], [4.0, 0], [5.8, 0]],
  },
  // Anticipation, confident point, settle.
  act: {
    tiltX: [[0, 0], [0.25, -0.2], [0.45, 0.25], [1.1, 0]],
    y: [[0, 0], [0.25, -0.05], [0.45, 0.08], [1.1, 0]],
    rArmOut: [[0, 0], [0.25, 0.5], [0.45, 0.6], [0.8, 0.5], [1.1, 0]],
    rArmFwd: [[0, 0], [0.25, -0.4], [0.45, 1.5], [0.8, 1.4], [1.1, 0]],
    rElbow: [[0, 0], [0.25, 0.6], [0.45, -0.7], [0.8, -0.6], [1.1, 0]],
    browR: [[0, 0], [0.4, 0.3], [0.9, 0.2], [1.1, 0]],
    browTilt: [[0, 0], [0.25, -0.3], [0.6, -0.2], [1.1, 0]],
    lookX: [[0, 0], [0.3, 0.4], [0.9, 0.3], [1.1, 0]],
    rGrip: [[0, 0], [0.3, 0.75], [0.9, 0.75], [1.1, 0]],
    rPoint: [[0, 0], [0.3, 1], [0.9, 1], [1.1, 0]],
    lid: [[0, 0], [0.25, 0.15], [0.45, -0.1], [1.1, 0]],
  },
  // Nod, near (left) hand up in a quick "yes", little hop, pleased squint.
  accepted: {
    y: [[0, 0], [0.2, -0.06], [0.45, 0.16], [0.8, 0.04], [1.2, 0]],
    tiltX: [[0, 0], [0.2, 0.25], [0.35, -0.1], [0.55, 0.2], [0.8, 0], [1.2, 0]],
    lArmFwd: [[0, 0], [0.3, 1.1], [0.55, 1.4], [0.9, 1.0], [1.2, 0]],
    lArmOut: [[0, 0], [0.3, 0.7], [0.55, 0.8], [0.9, 0.6], [1.2, 0]],
    lElbow: [[0, 0], [0.3, 0.8], [0.55, 1.1], [0.9, 0.8], [1.2, 0]],
    lGrip: [[0, 0], [0.25, 0.75], [0.9, 0.75], [1.2, 0]],
    browL: [[0, 0], [0.3, 0.5], [0.9, 0.4], [1.2, 0]],
    browR: [[0, 0], [0.3, 0.3], [0.9, 0.2], [1.2, 0]],
    lid: [[0, 0], [0.3, 0.12], [0.9, 0.1], [1.2, 0]],
    smile: [[0, 0], [0.25, 0.4], [1.0, 0.4], [1.2, 0]],
  },
  // Bounce back, look at the hand, shrug, recover dignity.
  rejected: {
    y: [[0, 0], [0.12, 0.1], [0.3, -0.12], [0.6, -0.04], [1.6, 0]],
    tiltX: [[0, 0], [0.12, -0.35], [0.4, -0.1], [0.9, 0.05], [1.6, 0]],
    tiltZ: [[0, 0], [0.4, 0.12], [0.9, -0.08], [1.6, 0]],
    rArmFwd: [[0, 0], [0.3, 0.9], [0.7, 0.9], [1.0, 0.3], [1.6, 0]],
    rElbow: [[0, 0], [0.3, 0.6], [0.7, 0.6], [1.6, 0]],
    rArmOut: [[0, 0], [0.7, 0], [0.95, 0.7], [1.2, 0.6], [1.6, 0]],
    lArmOut: [[0, 0], [0.7, 0], [0.95, 0.7], [1.2, 0.6], [1.6, 0]],
    lElbow: [[0, 0], [0.7, 0], [0.95, 0.5], [1.2, 0.5], [1.6, 0]],
    rGrip: [[0, 0], [0.7, 0], [0.95, -0.25], [1.2, -0.25], [1.6, 0]],
    lGrip: [[0, 0], [0.7, 0], [0.95, -0.25], [1.2, -0.25], [1.6, 0]],
    lookX: [[0, 0], [0.3, 0.6], [0.7, 0.6], [0.95, -0.2], [1.6, 0]],
    lookY: [[0, 0], [0.3, -0.5], [0.7, -0.5], [0.95, 0.1], [1.6, 0]],
    browTilt: [[0, 0], [0.15, 0.5], [1.2, 0.45], [1.6, 0]],
    browL: [[0, 0], [0.15, 0.5], [1.2, 0.3], [1.6, 0]],
    lid: [[0, 0], [0.12, -0.3], [0.5, -0.25], [1.0, 0.1], [1.6, 0]],
    smile: [[0, 0], [0.15, -1.3], [0.9, -0.9], [1.2, -0.3], [1.6, 0]],
  },
}

/** Pose at `t` seconds into `clip`; `time` drives the idle layer, `motion` in [0,1] scales amplitude. */
export function poseAt(clip: GenieClip, t: number, time: number, motion = 1): Pose {
  const pose = idle(time, motion)
  if (clip === 'idle') return pose
  const offsets = OFFSETS[clip]
  for (const key of Object.keys(offsets) as (keyof Pose)[]) {
    pose[key] += track(offsets[key]!, t) * motion
  }
  if (clip === 'fuckOff') {
    // The winding fist should stay still while the other hand circles it.
    pose.y *= 1 - pose.crank
    pose.tiltX *= 1 - pose.crank
    pose.tiltZ *= 1 - pose.crank
  }
  return pose
}

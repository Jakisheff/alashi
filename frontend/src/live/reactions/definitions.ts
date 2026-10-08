export type ReactionKind = 'thumbsUp' | 'realization' | 'facepalm'
export type ReactionPreview = {
  kind: ReactionKind
  take: number
  playing: boolean
  speed: number
  seek: number | null
}

/** Same duration for every reaction so a parent can share one scrubber. */
export const REACTION_SECONDS = 5.2
export const LIVING_IDLE_SECONDS = 24
export const REACTION_KEYFRAMES = {
  thumbsUp: [
    { time: 0, label: 'Idle' },
    { time: .45, label: 'Anticipation / weight back' },
    { time: 1.25, label: 'Thumb raised / body opens' },
    { time: 1.9, label: 'Confident full-body nod' },
    { time: 2.65, label: 'Hold / smile' },
    { time: 3.8, label: 'Release' },
    { time: 5.2, label: 'Idle restored' },
  ],
  realization: [
    { time: 0, label: 'Idle' },
    { time: .45, label: 'Notice mistake / recoil' },
    { time: 1.3, label: 'Both palms outside temples' },
    { time: 2.3, label: 'Bow / eyes down / hands on head' },
    { time: 3.1, label: 'Hold / small head shake' },
    { time: 3.9, label: 'Recover / hands release' },
    { time: 5.2, label: 'Idle restored' },
  ],
  facepalm: [
    { time: 0, label: 'Idle' },
    { time: .5, label: 'Sigh / weight drops' },
    { time: 1.45, label: 'Open palm in front of upper face' },
    { time: 2.25, label: 'Bow / shoulders settle' },
    { time: 3.1, label: 'Hold / eyes down' },
    { time: 3.9, label: 'Hand withdraws before recovering' },
    { time: 5.2, label: 'Idle restored' },
  ],
} as const satisfies Record<ReactionKind, readonly { time: number; label: string }[]>

export const REACTION_REDUCED_FRAME: Record<ReactionKind, number> = {
  thumbsUp: 2.65, realization: 2.3, facepalm: 2.25,
}
export const IDLE_KEYFRAMES = [
  { time: 0, label: 'Normal idle' },
  { time: 4, label: 'Eyes anticipate left turn' },
  { time: 5.3, label: 'Whole body turned left' },
  { time: 6.3, label: 'Curious tilt' },
  { time: 8.2, label: 'Return to center' },
  { time: 14.8, label: 'Eyes anticipate right turn' },
  { time: 16.3, label: 'Whole body turned right' },
  { time: 19, label: 'Return to center' },
  { time: 24, label: 'Seamless cycle boundary' },
] as const

export const smooth = (x: number) => { const k = Math.max(0, Math.min(1, x)); return k * k * (3 - 2 * k) }
export const ramp = (t: number, start: number, end: number) => smooth((t - start) / (end - start))
export const pulse = (t: number, start: number, rise: number, hold: number, end: number) => ramp(t, start, rise) * (1 - ramp(t, hold, end))

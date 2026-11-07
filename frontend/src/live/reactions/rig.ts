import { Bone, Euler, Matrix4, Object3D, PropertyBinding, Quaternion, Vector3, type AnimationClip } from 'three'
import { CLIP_SECONDS } from '../../genie/pose.ts'
import { LIVING_IDLE_SECONDS, pulse, ramp, type ReactionKind } from './definitions.ts'

type Side = 'left' | 'right'
const X = new Vector3(1, 0, 0), DOWN = new Vector3(0, -1, 0)
const IDENTITY = new Quaternion()
const PALM_UP = new Quaternion().setFromAxisAngle(X, -Math.PI / 2)
// Fingers follow local -Y; palm/button faces local +Z. These orientations keep
// fingertips outside the casing while bringing the button toward the head.
function uprightPalm(normal: Vector3) {
  return new Quaternion().setFromRotationMatrix(new Matrix4().makeBasis(new Vector3().crossVectors(DOWN, normal), DOWN, normal))
}
const TEMPLE_LEFT = uprightPalm(new Vector3(-1, 0, 0))
const TEMPLE_RIGHT = uprightPalm(new Vector3(1, 0, 0))
const FACE_PALM = uprightPalm(new Vector3(0, 0, -1))
const DANCE_RATE = Math.PI * 128 / 60

/** Own rig layer: it only rotates arm joints, never relocates finger sockets. */
export function createLivingRig(hero: Object3D, clips: AnimationClip[]) {
  const bones = new Map<string, Bone>()
  hero.traverse((o) => { if ((o as Bone).isBone) bones.set(o.name.replace(/_\d+$/, ''), o as Bone) })
  function bone(name: string) {
    const found = bones.get(name)
    if (!found) throw new Error(`LivingGenie requires hero bone: ${name}`)
    return found
  }
  for (const side of ['left', 'right']) {
    for (const part of ['arm', 'forearm', 'hand', 'thumb', 'thumb-tip', 'pupil', 'lid', 'brow', 'point', 'middle', 'curl']) bone(`${side}-${part}`)
    for (const part of ['point', 'middle', 'curl']) for (const suffix of ['-mid', '-tip']) bone(`${side}-${part}${suffix}`)
  }
  bone('body'); bone('mouth')
  const idle = clips.find((clip) => clip.name === 'idle')
  if (!idle) throw new Error('LivingGenie requires the idle clip')
  const rest = Array.from(bones.values(), (node) => ({ node, position: node.position.clone(), quaternion: node.quaternion.clone(), scale: node.scale.clone() }))
  // Restore *all* sampled properties, including constant tracks. Mixer caches
  // otherwise leave a previous reaction's hand/eyelid behind on backward seeks.
  const tracks = idle.tracks.flatMap((track) => {
    const binding = PropertyBinding.parseTrackName(track.name)
    const node = PropertyBinding.findNode(hero, binding.nodeName) as Object3D | null
    if (!node || !['position', 'quaternion', 'scale'].includes(binding.propertyName)) return []
    return [{ node, property: binding.propertyName, sample: track.InterpolantFactoryMethodLinear() }]
  })
  function restore(time: number, reduced: boolean) {
    for (const { node, position, quaternion, scale } of rest) {
      node.position.copy(position); node.quaternion.copy(quaternion); node.scale.copy(scale)
    }
    for (const { node, property, sample } of tracks) {
      // GLB includes one extra export frame (4.0333s); the authored idle cycle
      // is 4s. Match Genie/MarketTrade's phase so the 24s living cycle closes.
      const duration = CLIP_SECONDS.idle
      const values = sample.evaluate(reduced ? 0 : ((time % duration) + duration) % duration)
      if (property === 'quaternion') node.quaternion.fromArray(values)
      else if (property === 'scale') node.scale.fromArray(values)
      else node.position.fromArray(values)
    }
    bone('body').position.y = reduced ? 0 : .07 * Math.sin(time * Math.PI)
  }
  function reach(side: Side, target: Vector3, palm: Quaternion | 'neutral' | 'chin' | 'thumb', weight: number, elbowHint?: Vector3) {
    const arm = bone(`${side}-arm`), forearm = bone(`${side}-forearm`), hand = bone(`${side}-hand`)
    const shoulder = arm.position.clone(), l1 = forearm.position.length(), l2 = hand.position.length()
    const delta = target.clone().sub(shoulder)
    const distance = Math.max(Math.abs(l1 - l2) + .003, Math.min(delta.length(), l1 + l2 - .003))
    const axis = delta.lengthSq() > 1e-8 ? delta.normalize() : DOWN.clone()
    const neutral = typeof palm === 'string'
    const pole = elbowHint?.clone() ?? new Vector3(side === 'left' ? 1 : -1, neutral ? -.9 : -.4, palm === 'thumb' ? -.5 : neutral ? .9 : -.25)
    pole.addScaledVector(axis, -pole.dot(axis))
    if (pole.lengthSq() < 1e-8) pole.copy(new Vector3(0, 0, 1)).addScaledVector(axis, -axis.z)
    pole.normalize()
    const along = (l1 * l1 - l2 * l2 + distance * distance) / (2 * distance)
    const elbow = shoulder.clone().addScaledVector(axis, along).addScaledVector(pole, Math.sqrt(Math.max(0, l1 * l1 - along * along)))
    const wrist = shoulder.clone().addScaledVector(axis, distance)
    const qa = new Quaternion().setFromUnitVectors(DOWN, elbow.clone().sub(shoulder).normalize())
    const qf = new Quaternion().setFromUnitVectors(DOWN, wrist.clone().sub(elbow).normalize())
    if (typeof palm === 'string') {
      const y = wrist.clone().sub(elbow).normalize().negate()
      const facing = palm === 'thumb' ? new Vector3(side === 'left' ? -1 : 1, 0, .25).normalize() : new Vector3(0, 0, palm === 'chin' ? -1 : 1)
      const z = facing.addScaledVector(y, -facing.dot(y)).normalize()
      // Forearm takes the roll; the hand itself stays neutral at its socket.
      qf.setFromRotationMatrix(new Matrix4().makeBasis(new Vector3().crossVectors(y, z), y, z))
      palm = qf.clone()
    }
    arm.quaternion.slerp(qa, weight)
    forearm.quaternion.slerp(qa.clone().invert().multiply(qf), weight)
    hand.quaternion.slerp(qf.clone().invert().multiply(palm), weight)
    for (const part of ['point', 'middle', 'curl']) for (const suffix of ['', '-mid', '-tip']) bone(`${side}-${part}${suffix}`).quaternion.slerp(IDENTITY, weight)
    bone(`${side}-thumb`).quaternion.slerp(IDENTITY, weight)
    bone(`${side}-thumb-tip`).quaternion.slerp(IDENTITY, weight)
  }
  function gaze(x: number, y: number, weight: number) {
    for (const side of ['left', 'right']) {
      const pupil = bone(`${side}-pupil`)
      const origin = rest.find((item) => item.node === pupil)!.position
      pupil.position.x += (origin.x + x * .042 - pupil.position.x) * weight
      pupil.position.y += (origin.y + y * .036 - pupil.position.y) * weight
    }
  }
  function bodyOffset(pitch: number, yaw: number, roll: number, x: number, y: number, z = 0) {
    const body = bone('body')
    body.quaternion.multiply(new Quaternion().setFromEuler(new Euler(pitch, yaw, roll, 'YXZ')))
    body.position.add(new Vector3(x, y, z))
  }
  function idleAt(time: number, reduced = false) {
    restore(time, reduced)
    if (reduced) return
    const t = ((time % LIVING_IDLE_SECONDS) + LIVING_IDLE_SECONDS) % LIVING_IDLE_SECONDS
    const left = pulse(t, 4, 5.3, 6.7, 8.2), right = pulse(t, 14.8, 16.3, 17.5, 19)
    const turn = -.40 * left + .34 * right
    const curious = pulse(t, 5.5, 6.3, 6.7, 7.6)
    // Body is parent of casing, face, arms and smoke tail: the entire character
    // turns, with a quiet weight shift. Eyes anticipate the turn by ~0.35 sec.
    bodyOffset(-.035 * curious, turn, -.07 * curious + .025 * right, .028 * (right - left), -.016 * curious)
    const look = -pulse(t, 3.65, 4.3, 6.7, 8.2) + pulse(t, 14.45, 15.1, 17.5, 19)
    gaze(look * .8, curious * .15, Math.min(1, Math.abs(look)))
    bone('left-brow').position.y += .014 * curious
    for (let i = 0; i <= 7; i++) {
      const delayed = t - .25 - i * .035
      const drag = -.40 * pulse(delayed, 4, 5.3, 6.7, 8.2) + .34 * pulse(delayed, 14.8, 16.3, 17.5, 19)
      bone(i ? `tail-${i}` : 'tail').quaternion.multiply(new Quaternion().setFromAxisAngle(new Vector3(0, 1, 0), -.09 * drag))
    }
  }
  function reactionAt(kind: ReactionKind, t: number, idleTime = t, reduced = false) {
    idleAt(idleTime, reduced)
    const hold = pulse(t, .15, 1.25, 3.25, 4.85)
    const bow = pulse(t, .65, 2.25, 3.1, 4.6)
    if (kind === 'thumbsUp') {
      const anticipation = pulse(t, 0, .35, .45, 1)
      const nod = pulse(t, 1.35, 1.9, 2.05, 2.6)
      bodyOffset(-.08 * hold + .16 * nod + .05 * anticipation, -.16 * hold, -.075 * hold,
        -.055 * hold, -.035 * anticipation + .045 * hold)
      reach('left', new Vector3(.92, -.28, .58), 'thumb', hold)
      reach('right', new Vector3(-.85, -.70, .32), PALM_UP, hold * .7)
      for (const part of ['point', 'middle', 'curl']) {
        for (const [suffix, bend] of [['', -1.10], ['-mid', -1.2], ['-tip', -.85]] as const) {
          bone(`left-${part}${suffix}`).quaternion.slerp(new Quaternion().setFromAxisAngle(X, bend), hold)
        }
      }
      // A real thumbs-up extends the thumb across the fist, perpendicular to
      // the forward-reaching forearm. The wrist stays aligned with that arm.
      const thumbDirection = bone('left-thumb-tip').position.clone().normalize()
      bone('left-thumb').quaternion.slerp(new Quaternion().setFromUnitVectors(thumbDirection, X), hold)
      bone('left-thumb-tip').quaternion.slerp(IDENTITY, hold)
      gaze(.15, .1, pulse(t, .05, .35, 3.25, 4.75))
      bone('mouth').scale.y += .26 * hold
    } else if (kind === 'realization') {
      const recoil = pulse(t, .05, .4, .55, 1.05)
      const shake = Math.sin((t - 2.3) * 5) * .045 * pulse(t, 2.3, 2.6, 3, 3.4)
      bodyOffset(.31 * bow - .10 * recoil, shake, .025 * bow, 0, -.10 * bow + .025 * recoil, .035 * bow)
      reach('left', new Vector3(.94, .035, .17), TEMPLE_LEFT, hold)
      reach('right', new Vector3(-.94, .035, .17), TEMPLE_RIGHT, hold)
      for (const side of ['left', 'right']) {
        // Keep the thumb beside the upright fingers, clear of the orange ear controls.
        const direction = bone(`${side}-thumb-tip`).position.clone().normalize()
        bone(`${side}-thumb`).quaternion.slerp(new Quaternion().setFromUnitVectors(direction, DOWN), hold)
      }
      gaze(0, -.95, pulse(t, .45, 2.05, 3.1, 4.6))
      bone('mouth').scale.y += (-.40 - bone('mouth').scale.y) * bow
      for (const side of ['left', 'right']) {
        bone(`${side}-brow`).position.y += .025 * recoil - .013 * bow
        bone(`${side}-brow`).quaternion.multiply(new Quaternion().setFromAxisAngle(new Vector3(0, 0, 1), (side === 'left' ? -.22 : .22) * bow))
        const lid = bone(`${side}-lid`)
        lid.scale.y += (.08 - lid.scale.y) * recoil
        lid.scale.y += (.48 - lid.scale.y) * bow
      }
    } else if (kind === 'facepalm') {
      // The wrist stays in front of the screen; fingers point up, palm points
      // backward. Lower body / counterbalancing left hand sells the whole sigh.
      bodyOffset(.22 * bow, -.10 * hold, -.075 * bow, -.04 * bow, -.085 * bow, .025 * bow)
      reach('right', new Vector3(-.44, -.025, .70), FACE_PALM, hold)
      reach('left', new Vector3(.89, -.78, .22), PALM_UP, hold * .7)
      gaze(-.15, -.8, pulse(t, .05, .65, 3.1, 4.6))
      bone('mouth').scale.y += (-.28 - bone('mouth').scale.y) * bow
      for (const side of ['left', 'right']) bone(`${side}-brow`).quaternion.multiply(new Quaternion().setFromAxisAngle(new Vector3(0, 0, 1), (side === 'left' ? -.12 : .12) * bow))
      for (const side of ['left', 'right']) bone(`${side}-lid`).scale.y = Math.max(bone(`${side}-lid`).scale.y, .3 + .35 * bow)
    } else if (kind === 'thinking') {
      const ponder = pulse(t, .4, 1.5, 3.1, 4.6)
      const nod = pulse(t, 2.05, 2.6, 2.75, 3.25)
      bodyOffset(.075 * nod, -.12 * ponder, .07 * ponder, -.025 * ponder, -.025 * ponder)
      reach('right', new Vector3(-.38, -.25, .65), 'chin', hold)
      for (const part of ['point', 'middle', 'curl']) for (const [suffix, bend] of [['', -1.0], ['-mid', -1.15], ['-tip', -.85]] as const) {
        bone(`right-${part}${suffix}`).quaternion.slerp(new Quaternion().setFromAxisAngle(X, bend), hold)
      }
      const thumbDirection = bone('right-thumb-tip').position.clone().normalize()
      bone('right-thumb').quaternion.slerp(new Quaternion().setFromUnitVectors(thumbDirection, new Vector3(.85, -.4, .35).normalize()), hold)
      bone('right-thumb-tip').quaternion.slerp(new Quaternion().setFromAxisAngle(X, -.55), hold)
      reach('left', new Vector3(.96, -.77, .32), PALM_UP, hold * .65)
      gaze(-.45, .35, pulse(t, .05, .6, 3.1, 4.4))
      bone('left-brow').position.y += .022 * ponder
      bone('right-brow').position.y -= .009 * ponder
    } else if (kind === 'shrug') {
      const tilt = pulse(t, .8, 1.8, 3, 4.5)
      const prepare = pulse(t, 0, .3, .4, .9)
      const otherHand = pulse(t, .27, 1.37, 3.25, 4.85)
      bodyOffset(-.035 * hold + .03 * prepare, .045 * tilt, .14 * tilt, .025 * tilt, .035 * hold - .025 * prepare)
      const levelPalm = bone('body').quaternion.clone().invert().multiply(PALM_UP)
      reach('left', new Vector3(1.09, -.20, .20), levelPalm, hold, new Vector3(.35, -.8, -.1))
      reach('right', new Vector3(-1.09, -.20, .20), levelPalm, otherHand, new Vector3(-.35, -.8, -.1))
      for (const side of ['left', 'right']) bone(`${side}-brow`).position.y += .025 * hold
      gaze(.25, .2, pulse(t, .05, .45, 3, 4.5))
      bone('mouth').scale.y += (.35 - bone('mouth').scale.y) * hold
    } else if (kind === 'lookout') {
      const left = pulse(t, .15, .8, 1.05, 1.7), right = pulse(t, 1.15, 1.8, 2.25, 3.0)
      const lean = pulse(t, 2.05, 2.8, 3.1, 4.4)
      bodyOffset(.07 * lean, -.38 * left + .38 * right, -.035 * left + .04 * right,
        .04 * (right - left), -.035 * lean, .04 * lean)
      const eyesLeft = pulse(t, 0, .45, 1.05, 1.7), eyesRight = pulse(t, .9, 1.45, 2.25, 3.0)
      gaze(-.85 * eyesLeft + .85 * eyesRight, -.1, Math.max(eyesLeft, eyesRight))
      reach('right', new Vector3(-1.0, -.42, .5), PALM_UP, lean * .8)
      bone('left-brow').position.y += .014 * lean
      bone('mouth').scale.y += .18 * lean
    } else if (kind === 'dance') {
      const prepare = pulse(t, .12, .35, .5, .85)
      const groove = pulse(t, .2, .65, 3.9, 4.9)
      const phase = (t - .65) * DANCE_RATE, sway = Math.sin(phase)
      const pout = pulse(t, .85, 1.2, 1.5, 1.9) + pulse(t, 2.2, 2.6, 2.95, 3.45)
      // Replace the idle hover while dancing; weight and fists share one beat.
      bone('body').position.y *= 1 - groove
      bodyOffset(.065 * prepare + (.035 * Math.cos(phase * 2) - .04) * groove,
        .13 * Math.sin(phase - .2) * groove, .07 * Math.sin(phase - .12) * groove,
        .07 * sway * groove, -.035 * prepare + (.015 + .022 * Math.cos(phase * 2)) * groove)
      for (const side of ['left', 'right'] as const) {
        const sign = side === 'left' ? 1 : -1, beat = sign * sway
        reach(side, new Vector3(sign * (.92 + .04 * beat), -.32 + .13 * beat, .57 + .14 * beat), 'thumb', groove,
          new Vector3(sign * .7, -.7, -.2))
        for (const part of ['point', 'middle', 'curl']) for (const [suffix, bend] of [['', -.85], ['-mid', -1.15], ['-tip', -.8]] as const) {
          bone(`${side}-${part}${suffix}`).quaternion.slerp(new Quaternion().setFromAxisAngle(X, bend), groove)
        }
        const thumbDirection = bone(`${side}-thumb-tip`).position.clone().normalize()
        bone(`${side}-thumb`).quaternion.slerp(new Quaternion().setFromUnitVectors(thumbDirection, new Vector3(-sign * .85, -.4, .35).normalize()), groove)
        bone(`${side}-thumb-tip`).quaternion.slerp(new Quaternion().setFromAxisAngle(X, -.55), groove)
        const lid = bone(`${side}-lid`)
        lid.scale.y += (.36 + .12 * pout - lid.scale.y) * groove
        bone(`${side}-brow`).position.y += (side === 'left' ? .014 : -.006) * groove
        bone(`${side}-brow`).quaternion.multiply(new Quaternion().setFromAxisAngle(new Vector3(0, 0, 1), (side === 'left' ? .12 : -.08) * groove))
      }
      gaze(.45 * Math.sin(phase + .5), .1, pulse(t, 0, .2, 3.9, 4.75))
      const mouth = bone('mouth')
      mouth.scale.x += (.62 - mouth.scale.x) * pout
      mouth.scale.y += (-.32 - mouth.scale.y) * pout
      mouth.quaternion.multiply(new Quaternion().setFromAxisAngle(new Vector3(0, 0, 1), -.14 * groove * (1 - pout)))
    } else if (kind === 'victory') {
      const windup = pulse(t, 0, .4, .5, 1.1)
      const cheer = pulse(t, .45, 1.25, 3.15, 4.6)
      const rock = Math.sin((t - 1.25) * 5) * .065 * pulse(t, 1.25, 1.7, 2.75, 3.45)
      bodyOffset(.09 * windup - .1 * cheer, rock, rock * .7,
        rock * .35, -.055 * windup + .055 * cheer)
      for (const side of ['left', 'right'] as const) {
        const armCheer = side === 'left' ? cheer : pulse(t, .57, 1.37, 3.27, 4.72)
        reach(side, new Vector3(side === 'left' ? .98 : -.98, .05, .48), 'neutral', armCheer)
        for (const part of ['point', 'middle', 'curl']) for (const [suffix, bend] of [['', -.85], ['-mid', -1.15], ['-tip', -.8]] as const) {
          bone(`${side}-${part}${suffix}`).quaternion.slerp(new Quaternion().setFromAxisAngle(X, bend), armCheer)
        }
      }
      gaze(0, .4, pulse(t, .15, .65, 3.15, 4.6))
      bone('mouth').scale.y += .4 * cheer
      for (const side of ['left', 'right']) bone(`${side}-brow`).position.y += .025 * cheer
    }
    // Follow this reaction's effort instead of applying a bow to every gesture.
    for (let i = 0; i <= 7; i++) {
      const delayed = t - .2 - i * .035
      const danceDrag = .11 * Math.sin((delayed - .65) * DANCE_RATE) * pulse(delayed, .2, .65, 3.9, 4.9)
      const drag = kind === 'dance' ? danceDrag
        : kind === 'victory' ? -.09 * pulse(delayed, 0, .4, .5, 1.1) + .1 * pulse(delayed, .45, 1.25, 3.15, 4.6)
        : kind === 'thumbsUp' ? -.16 * pulse(delayed, 1.35, 1.9, 2.05, 2.6)
        : kind === 'thinking' ? -.075 * pulse(delayed, 2.05, 2.6, 2.75, 3.25)
        : kind === 'shrug' ? .035 * pulse(delayed, .15, 1.25, 3.25, 4.85)
        : kind === 'lookout' ? -.07 * pulse(delayed, 2.05, 2.8, 3.1, 4.4)
        : -(kind === 'realization' ? .31 : .22) * pulse(delayed, .65, 2.25, 3.1, 4.6)
      const tail = bones.get(i ? `tail-${i}` : 'tail')
      tail?.quaternion.multiply(new Quaternion().setFromAxisAngle(X, drag * .22 * (1 - ramp(t, 4.95, 5.2))))
    }
  }
  return { bones, bone, idleAt, reactionAt }
}
export type LivingRig = ReturnType<typeof createLivingRig>

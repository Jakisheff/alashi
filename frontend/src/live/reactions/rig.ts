import { Bone, Euler, Matrix4, Object3D, PropertyBinding, Quaternion, Vector3, type AnimationClip } from 'three'
import { CLIP_SECONDS } from '../../genie/pose.ts'
import { LIVING_IDLE_SECONDS, pulse, type ReactionKind } from './definitions.ts'

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
const THUMB_PALM = new Quaternion().setFromAxisAngle(new Vector3(0, 0, 1), Math.PI / 2)

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
  function reach(side: Side, target: Vector3, palm: Quaternion, weight: number) {
    const arm = bone(`${side}-arm`), forearm = bone(`${side}-forearm`), hand = bone(`${side}-hand`)
    const shoulder = arm.position.clone(), l1 = forearm.position.length(), l2 = hand.position.length()
    const delta = target.clone().sub(shoulder)
    const distance = Math.max(Math.abs(l1 - l2) + .003, Math.min(delta.length(), l1 + l2 - .003))
    const axis = delta.lengthSq() > 1e-8 ? delta.normalize() : DOWN.clone()
    const pole = new Vector3(side === 'left' ? 1 : -1, -.4, -.25)
    pole.addScaledVector(axis, -pole.dot(axis))
    if (pole.lengthSq() < 1e-8) pole.copy(new Vector3(0, 0, 1)).addScaledVector(axis, -axis.z)
    pole.normalize()
    const along = (l1 * l1 - l2 * l2 + distance * distance) / (2 * distance)
    const elbow = shoulder.clone().addScaledVector(axis, along).addScaledVector(pole, Math.sqrt(Math.max(0, l1 * l1 - along * along)))
    const wrist = shoulder.clone().addScaledVector(axis, distance)
    const qa = new Quaternion().setFromUnitVectors(DOWN, elbow.clone().sub(shoulder).normalize())
    const qf = new Quaternion().setFromUnitVectors(DOWN, wrist.clone().sub(elbow).normalize())
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
      reach('left', new Vector3(.87, -.06, .49), THUMB_PALM, hold)
      reach('right', new Vector3(-.85, -.70, .32), PALM_UP, hold * .7)
      for (const part of ['point', 'middle', 'curl']) {
        for (const [suffix, bend] of [['', -1.10], ['-mid', -1.2], ['-tip', -.85]] as const) {
          bone(`left-${part}${suffix}`).quaternion.slerp(new Quaternion().setFromAxisAngle(X, bend), hold)
        }
      }
      // Mesh thumb is diagonally modelled: its actual tip offset, not -Y,
      // determines extension. Map that direction to local +X (world up in this palm).
      const thumbDirection = bone('left-thumb-tip').position.clone().normalize()
      bone('left-thumb').quaternion.slerp(new Quaternion().setFromUnitVectors(thumbDirection, X), hold)
      bone('left-thumb-tip').quaternion.slerp(IDENTITY, hold)
      gaze(.15, .1, hold)
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
      gaze(0, -.95, bow)
      bone('mouth').scale.y += (.16 - bone('mouth').scale.y) * hold
      for (const side of ['left', 'right']) {
        bone(`${side}-brow`).position.y += .025 * recoil - .013 * bow
        bone(`${side}-lid`).scale.y = Math.max(bone(`${side}-lid`).scale.y, .3 + .18 * bow)
      }
    } else {
      // The wrist stays in front of the screen; fingers point up, palm points
      // backward. Lower body / counterbalancing left hand sells the whole sigh.
      bodyOffset(.22 * bow, -.10 * hold, -.075 * bow, -.04 * bow, -.085 * bow, .025 * bow)
      reach('right', new Vector3(-.44, -.025, .70), FACE_PALM, hold)
      reach('left', new Vector3(.89, -.78, .22), PALM_UP, hold * .7)
      gaze(-.15, -.8, bow)
      bone('mouth').scale.y += (.14 - bone('mouth').scale.y) * hold
      for (const side of ['left', 'right']) bone(`${side}-lid`).scale.y = Math.max(bone(`${side}-lid`).scale.y, .3 + .35 * bow)
    }
    // Tail already inherits torso motion; a subtle delayed counterbend prevents
    // the body and smoke from reading as a rigid object. Restored every frame.
    const tail = bones.get('tail')
    if (tail) tail.quaternion.multiply(new Quaternion().setFromAxisAngle(X, -.07 * bow))
  }
  return { bones, bone, idleAt, reactionAt }
}
export type LivingRig = ReturnType<typeof createLivingRig>

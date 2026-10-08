import { Bone, Box3, Object3D, PropertyBinding, Quaternion, Vector3, type AnimationClip } from 'three'
import { ramp } from './definitions.ts'

export type Key = readonly [number, number, number, number]
export type Side = 'left' | 'right'
const DOWN = new Vector3(0, -1, 0)
const IDENTITY = new Quaternion()
export const PALM_UP = new Quaternion().setFromAxisAngle(new Vector3(1, 0, 0), -Math.PI / 2)

export function path(time: number, keys: readonly Key[]): Vector3 {
  const first = keys[0]
  if (time <= first[0]) return new Vector3(first[1], first[2], first[3])
  for (let i = 1; i < keys.length; i++) {
    const a = keys[i - 1], b = keys[i]
    if (time <= b[0]) return new Vector3(a[1], a[2], a[3]).lerp(new Vector3(b[1], b[2], b[3]), ramp(time, a[0], b[0]))
  }
  const last = keys[keys.length - 1]
  return new Vector3(last[1], last[2], last[3])
}

export function createRig(hero: Object3D, clips: AnimationClip[]) {
  const bones = new Map<string, Bone>()
  hero.traverse((o) => { if ((o as Bone).isBone) bones.set(o.name.replace(/_\d+$/, ''), o as Bone) })
  const bone = (name: string) => {
    const found = bones.get(name)
    if (!found) throw new Error(`Scenario preview requires hero joint: ${name}`)
    return found
  }
  for (const side of ['left', 'right']) {
    for (const part of ['arm', 'forearm', 'hand', 'lid', 'point', 'point-mid', 'point-tip', 'middle', 'middle-mid', 'middle-tip', 'curl', 'curl-mid', 'curl-tip']) bone(`${side}-${part}`)
  }
  bone('body'); bone('mouth')
  const idle = clips.find((clip) => clip.name === 'idle')
  if (!idle) throw new Error('Scenario preview requires the idle clip')
  const rest = Array.from(bones.values(), (node) => ({ node, position: node.position.clone(), quaternion: node.quaternion.clone(), scale: node.scale.clone() }))
  // Directly sample the existing idle tracks. Unlike mixer property caching, this
  // restores constant tracks too, so backward scrubs cannot retain a previous pose.
  const tracks = idle.tracks.flatMap((track) => {
    const binding = PropertyBinding.parseTrackName(track.name)
    const node = PropertyBinding.findNode(hero, binding.nodeName) as Object3D | null
    const property = binding.propertyName
    if (!node || !['position', 'quaternion', 'scale'].includes(property)) return []
    return [{ node, property, sample: track.InterpolantFactoryMethodLinear() }]
  })
  function idleAt(time: number, reduced: boolean) {
    for (const item of rest) {
      item.node.position.copy(item.position); item.node.quaternion.copy(item.quaternion); item.node.scale.copy(item.scale)
    }
    const phase = reduced ? 0 : time % idle!.duration
    for (const { node, property, sample } of tracks) {
      const values = sample.evaluate(phase)
      if (property === 'quaternion') node.quaternion.fromArray(values)
      else if (property === 'position') node.position.fromArray(values)
      else node.scale.fromArray(values)
    }
    bone('body').position.y = reduced ? 0 : .07 * Math.sin(time * Math.PI)
  }

  // Same two-bone rotation-only solver as MarketTrade; preserve every joint length.
  function reach(side: Side, target: Vector3, weight: number, palm = PALM_UP) {
    const arm = bone(`${side}-arm`), forearm = bone(`${side}-forearm`), hand = bone(`${side}-hand`)
    const shoulder = arm.position.clone(), l1 = forearm.position.length(), l2 = hand.position.length()
    const delta = target.clone().sub(shoulder)
    const distance = Math.max(Math.abs(l1 - l2) + .003, Math.min(delta.length(), l1 + l2 - .003))
    const axis = delta.lengthSq() > 1e-8 ? delta.normalize() : DOWN.clone()
    const pole = new Vector3(side === 'left' ? 1 : -1, -.4, -.25)
    pole.addScaledVector(axis, -pole.dot(axis)).normalize()
    const along = (l1 * l1 - l2 * l2 + distance * distance) / (2 * distance)
    const elbow = shoulder.clone().addScaledVector(axis, along).addScaledVector(pole, Math.sqrt(Math.max(0, l1 * l1 - along * along)))
    const wrist = shoulder.clone().addScaledVector(axis, distance)
    const qa = new Quaternion().setFromUnitVectors(DOWN, elbow.clone().sub(shoulder).normalize())
    const qf = new Quaternion().setFromUnitVectors(DOWN, wrist.clone().sub(elbow).normalize())
    arm.quaternion.slerp(qa, weight)
    forearm.quaternion.slerp(qa.clone().invert().multiply(qf), weight)
    hand.quaternion.slerp(qf.clone().invert().multiply(palm), weight)
    for (const part of ['point', 'middle', 'curl']) for (const suffix of ['', '-mid', '-tip']) bone(`${side}-${part}${suffix}`).quaternion.slerp(IDENTITY, weight)
  }

  const bounds = new Box3()
  function palmPoint(side: Side) {
    hero.updateMatrixWorld(true)
    const hand = bone(`${side}-hand`)
    const point = hero.worldToLocal(hand.localToWorld(new Vector3(0, -.29, .19)))
    // Use the actual hand mesh envelope as a floor, including the thumb. Props
    // have bottom-origin geometry and rest above all fingers, never inside them.
    bounds.setFromObject(hand)
    const top = hero.worldToLocal(new Vector3(bounds.max.x, bounds.max.y, bounds.max.z)).y
    point.y = Math.max(point.y, top + .018)
    return point
  }
  return { hero, bones, bone, idleAt, reach, palmPoint }
}
export type ScenarioRig = ReturnType<typeof createRig>

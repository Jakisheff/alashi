import { Bone, Quaternion, Vector3 } from 'three'
const smooth = (v: number) => { const x = Math.min(1, Math.max(0, v)); return x * x * (3 - 2 * x) }
const ramp = (t: number, a: number, b: number) => smooth((t - a) / (b - a))
type Key = [number, number, number, number]
function path(t: number, keys: Key[]) {
  if (t <= keys[0][0]) return new Vector3(...keys[0].slice(1) as [number, number, number])
  for (let i = 1; i < keys.length; i++) {
    if (t <= keys[i][0]) {
      const a = keys[i - 1], b = keys[i], k = ramp(t, a[0], b[0])
      return new Vector3(a[1], a[2], a[3]).lerp(new Vector3(b[1], b[2], b[3]), k)
    }
  }
  const last = keys.at(-1)!
  return new Vector3(last[1], last[2], last[3])
}

// Preview pose layer shared by buy and sell. Bone lengths/positions stay fixed: solve the elbow and rotate joints.
function reach(bones: Map<string, Bone>, side: string, target: Vector3, palm: Quaternion, weight: number) {
  const arm = bones.get(`${side}-arm`)!, forearm = bones.get(`${side}-forearm`)!, hand = bones.get(`${side}-hand`)!
  const shoulder = arm.position.clone(), l1 = forearm.position.length(), l2 = hand.position.length()
  const delta = target.clone().sub(shoulder), distance = Math.min(delta.length(), l1 + l2 - .003)
  const axis = delta.normalize(), pole = new Vector3(side === 'left' ? 1 : -1, -.4, -.25)
  pole.addScaledVector(axis, -pole.dot(axis)).normalize()
  const along = (l1 * l1 - l2 * l2 + distance * distance) / (2 * distance)
  const elbow = shoulder.clone().addScaledVector(axis, along).addScaledVector(pole, Math.sqrt(Math.max(0, l1 * l1 - along * along)))
  const wrist = shoulder.clone().addScaledVector(axis, distance)
  const down = new Vector3(0, -1, 0)
  const qa = new Quaternion().setFromUnitVectors(down, elbow.clone().sub(shoulder).normalize())
  const qf = new Quaternion().setFromUnitVectors(down, wrist.clone().sub(elbow).normalize())
  arm.quaternion.slerp(qa, weight)
  forearm.quaternion.slerp(qa.clone().invert().multiply(qf), weight)
  hand.quaternion.slerp(qf.clone().invert().multiply(palm), weight)
  for (const part of ['point', 'middle', 'curl']) {
    for (const [suffix, bend] of [['', 0], ['-mid', 0], ['-tip', 0]] as const) {
      const bone = bones.get(`${side}-${part}${suffix}`)!
      bone.quaternion.slerp(new Quaternion().setFromAxisAngle(new Vector3(1, 0, 0), bend), weight)
    }
  }
}

export function poseTradeHands(bones: Map<string, Bone>, buying: boolean, t: number, palm: Quaternion) {
  const weight = ramp(t, .55, 1.1) * (1 - ramp(t, buying ? 5.5 : 4.9, buying ? 6.1 : 5.6))
  reach(bones, 'left', path(t, buying ? [[0,.82,-.8,.42],[.9,.88,-.45,.36],[1.35,1.05,-.08,.34],[1.85,1.22,-.08,.32],[2.3,.66,-.55,.44],[2.85,.34,-.82,.42],[3.65,.28,-.80,.43],[4.25,.28,-.76,.43],[4.7,.28,-.80,.43],[5.5,.28,-.80,.43],[6.1,.82,-.8,.42]] : [[0,.82,-.8,.42],[1.45,.82,-.8,.42],[1.85,.82,-.50,.34],[2.15,.83,-.36,.32],[2.8,1.23,-.32,.29],[3.2,1.16,-.48,.32],[3.65,1.02,-.48,.34],[4.15,.94,-.27,.33],[4.65,.94,-.27,.33],[5.5,.82,-.8,.42]]), palm, weight)
  reach(bones, 'right', buying ? path(t, [[0,-.79,-.72,.4],[2,-.79,-.72,.4],[2.8,-.36,-.82,.42],[3.65,-.28,-.80,.43],[4.25,-.28,-.76,.43],[4.7,-.28,-.80,.43],[5.5,-.28,-.80,.43],[6.1,-.79,-.72,.4]]) : new Vector3(-.79, -.72, .4), palm, weight)
}

// Keep the paying arm in front of the stock and clear of the counter.
export const BUY_COUNTER = new Vector3(1.45, -.61, .22)
export const BUY_STOCK = new Vector3(1.45, -.555, -.5)
export function purchaseCratePosition(t: number, leftPalm: Vector3, rightPalm: Vector3) {
  const supported = leftPalm.clone().add(rightPalm).multiplyScalar(.5)
  supported.y = Math.max(leftPalm.y, rightPalm.y)
  // First clear the paying arm in depth, then cross in front and settle onto the palms.
  const across = ramp(t, 3.05, 3.4)
  const position = BUY_STOCK.clone()
  position.z += (1.15 - position.z) * ramp(t, 2.7, 3.05)
  position.x += (supported.x - position.x) * across
  position.y += (supported.y - position.y) * across + .08 * Math.sin(across * Math.PI)
  position.z += (supported.z - position.z) * ramp(t, 3.4, 3.65)
  return position
}
export const purchaseCrateOpacity = (t: number) => 1 - ramp(t, 4.9, 5.5)

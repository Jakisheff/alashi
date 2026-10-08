import { Euler, Quaternion, Vector3 } from 'three'
import { ACTION_META, ACTION_SECONDS, clampTime, ramp, type ScenarioActionName, type ScenarioPreview } from './definitions.ts'
import { path, type Key, type ScenarioRig } from './rig.ts'
import type { ScenarioProps } from './props.ts'

const LEFT: Record<ScenarioActionName, readonly Key[]> = {
  mule: [[0,.83,-.8,.38],[1.1,.66,-.65,.43],[1.8,.27,-.38,.46],[2.45,-.18,-.40,.48],[2.9,-.18,-.46,.48],[3.4,.2,-.68,.43],[4.4,.3,-.64,.43],[5.4,.8,-.73,.4],[6.3,.83,-.8,.38]],
  bribe: [[0,.55,-.6,.3],[1.2,.79,-.44,.42],[1.8,.86,-.38,.40],[2.6,1.05,-.32,.34],[3.7,1.12,-.32,.34],[4.15,1.1,-.32,.34],[4.6,.6,-.30,.25],[5.8,.55,-.60,.3],[6.3,.83,-.8,.38]],
  vote: [[0,.55,-.6,.25],[.9,.65,-.45,.34],[1.25,.86,-.40,.39],[2.0,1.14,-.20,.33],[2.8,1.27,-.20,.30],[3.35,1.27,-.20,.30],[3.5,.78,-.32,.15],[3.65,.65,-.30,.10],[4.3,.55,-.5,.22],[5.8,.55,-.60,.3],[6.3,.83,-.8,.38]],
}
const RIGHT: Record<ScenarioActionName, readonly Key[]> = {
  mule: [[0,-.79,-.75,.4],[1.2,-.67,-.75,.4],[2.45,-.24,-.83,.43],[3.25,-.30,-.79,.45],[3.85,-.46,-.72,.41],[4.4,-.93,-.59,.35],[4.9,-1.02,-.58,.32],[5.35,-.76,-.64,.33],[5.5,-.75,-.75,.4],[6.3,-.79,-.75,.4]],
  bribe: [[0,-.79,-.75,.4],[1.6,-.70,-.70,.42],[2.4,-.64,-.54,.42],[3.6,-.66,-.61,.41],[4.5,-.68,-.74,.4],[6.3,-.79,-.75,.4]],
  vote: [[0,-.79,-.75,.4],[1.4,-.65,-.70,.44],[2.1,-.57,-.64,.46],[3.5,-.64,-.62,.44],[4.55,-.72,-.47,.37],[5.25,-.70,-.55,.41],[6.3,-.79,-.75,.4]],
}
// Each key is [seconds, yaw, side lean, forward lean]. Only small additive
// offsets are applied to the sampled idle; hovering never accumulates a drift.
const BODY: Record<ScenarioActionName, readonly Key[]> = {
  mule: [[0,0,0,0],[.85,.10,.025,-.025],[1.4,-.12,-.025,-.01],[2.45,.06,-.035,.055],[3.3,-.055,.035,.02],[4.45,-.17,.075,.035],[5.1,-.13,.05,.02],[5.65,.055,-.02,-.035],[6.6,0,0,0]],
  bribe: [[0,0,0,0],[1.0,.055,-.02,-.025],[1.65,.09,-.035,-.025],[2.8,.16,-.065,.045],[3.85,.18,-.07,.055],[4.75,.11,-.035,.015],[5.45,-.04,.02,-.03],[6.6,0,0,0]],
  vote: [[0,0,0,0],[1.0,-.035,.02,-.03],[1.7,.055,-.025,-.025],[2.8,.13,-.055,.035],[3.4,.14,-.065,.055],[4.2,.08,-.03,.005],[5.0,-.025,.025,-.025],[6.6,0,0,0]],
}

function pose(rig: ScenarioRig, action: ScenarioActionName, time: number, reduced: boolean) {
  // End pose is sampled from idle time zero, exactly matching the starting pose.
  rig.idleAt(time >= ACTION_SECONDS ? 0 : time, reduced)
  const weight = ramp(time, .0, .65) * (1 - ramp(time, 5.85, 6.6))
  const body = rig.bone('body'), turn = path(time, BODY[action])
  body.quaternion.multiply(new Quaternion().setFromEuler(new Euler(turn.z, turn.x, turn.y, 'YXZ')))
  // Follow the reach with the whole silhouette, including the tail. Vertical
  // motion remains exclusively the familiar ±.07 idle hover from the rig.
  body.position.x += -.5 * turn.y
  body.position.z += .35 * turn.z
  rig.reach('left', path(time, LEFT[action]), weight)
  rig.reach('right', path(time, RIGHT[action]), weight)
  if (time > .55 && time < 6.45) {
    // Keep the familiar sly eyes; no triumph or economy-result signal.
    rig.bone('left-lid').scale.y = .3
    rig.bone('right-lid').scale.y = .3
    rig.bone('mouth').scale.y = .6 + .16 * ramp(time, 4.7, 5.0) * (1 - ramp(time, 5.55, 6.15))
  }
  rig.hero.updateMatrixWorld(true)
}

/** Stateless sampling: forward playback, reverse scrubbing and action switches agree. */
export function applyScenario(rig: ScenarioRig, props: ScenarioProps, preview: Pick<ScenarioPreview, 'action' | 'entry'>, time: number, reduced = false) {
  const t = clampTime(time), action = preview.action
  // Sampling the release pose makes the drop anchor independent of frame history.
  // The ballot box stays under the release point throughout the entire scene.
  let ballotAnchor = new Vector3()
  if (action === 'vote') {
    pose(rig, action, 3.35, reduced)
    ballotAnchor = rig.palmPoint('left')
  }
  pose(rig, action, t, reduced)
  const left = rig.palmPoint('left'), right = rig.palmPoint('right')
  const opacity = ramp(t, .35, 1.1) * (1 - ramp(t, 5.75, 6.5))
  const entering = 1 - ramp(t, .35, 1.1)
  const leaving = ramp(t, 5.75, 6.5)
  const offset = preview.entry === 'bottom' ? new Vector3(0, -1.6 * (entering + leaving), 0) : new Vector3(2.5 * (entering + (action === 'mule' ? -leaving : leaving)), 0, 0)
  props.hideAll()
  props.root.position.copy(offset)
  props.fade(opacity)

  if (action === 'mule') {
    props.hat.visible = true
    props.hat.position.copy(rig.bone('body').position).add(new Vector3(0, .825, -.025)).sub(offset)
    props.hat.quaternion.copy(rig.bone('body').quaternion)
    props.bag.visible = true
    props.bag.position.copy(right).add(new Vector3(0, .025, 0))
    const pass = ramp(t, 4.9, 5.25)
    const lower = ramp(t, 5.35, 5.55)
    const receivingY = Math.max(-.58, right.y + .04)
    const receiving = new Vector3(-1.36, receivingY * (1 - lower) - .58 * lower, .58)
    props.bag.position.lerp(receiving, pass)
    props.bag.position.y += .04 * Math.sin(pass * Math.PI)
    // Receiving hand appears before the bag leaves the robot's support.
    props.receiver.visible = t >= 4.1 && t < 5.95
    props.receiver.position.copy(receiving).add(new Vector3(-.65 * (1 - ramp(t, 4.1, 4.8)), -.018, 0))
    const depart = ramp(t, 5.5, 5.95)
    props.bag.position.x -= .9 * depart
    props.receiver.position.x -= .9 * depart
    props.bag.scale.setScalar(1 - .12 * ramp(t, 2.75, 3.1))
    const sackMouth = right.clone().add(new Vector3(0, .595, 0))
    const stash = ramp(t, 1.9, 2.55), inside = ramp(t, 2.55, 2.95)
    props.parcel.visible = t >= .65 && t < 2.95
    props.parcel.position.copy(left).lerp(sackMouth, stash)
    props.parcel.position.y += .075 * Math.sin(stash * Math.PI) - .31 * inside
    if (t < 1.1) { props.bag.position.sub(offset); props.parcel.position.sub(offset) }
    // At the end of the insertion it is fully hidden inside the opaque sack.
  } else if (action === 'bribe') {
    const top = -.64
    props.desk.visible = props.official.visible = true
    props.desk.position.set(1.59, top, .10)
    props.official.position.set(1.75, top, -.36)
    props.official.rotation.y = -.28
    const accept = ramp(t, 3.05, 3.8), withdraw = ramp(t, 4.3, 5.25)
    const offered = new Vector3(1.70, -.13, .61)
    const resting = new Vector3(1.75, top + .085, .05)
    const contact = offered.clone().lerp(resting, withdraw)
    props.officialHand.visible = t >= .85 && t < 5.8
    props.officialHand.position.copy(new Vector3(1.71, -.22, .18).lerp(contact, accept))
    // Envelope initially rides the open palm; the official's palm then supports
    // its bottom while withdrawing across the desk. A small arc separates hands.
    const give = ramp(t, 3.65, 4.15)
    props.envelope.visible = t >= .65 && t < 5.95
    props.envelope.position.copy(left).lerp(contact.clone().add(new Vector3(-.08, .025, .06)), give)
    props.envelope.position.y = Math.max(props.envelope.position.y, left.y + .018 * (1 - ramp(t, 4.25, 4.6)))
    props.envelope.position.y += .055 * Math.sin(give * Math.PI)
    props.envelope.rotation.y = -.08 * ramp(t, 1.6, 2.5)
    if (t < 1.1) props.envelope.position.sub(offset)
  } else {
    props.urn.visible = true
    const top = -.58
    props.urn.position.set(ballotAnchor.x, top, ballotAnchor.z)
    props.ballot.visible = t >= .65 && t < 5.95
    const release = ramp(t, 3.5, 4.1)
    props.ballot.position.copy(t < 3.35 ? left : ballotAnchor)
    // Gravity-like ease-in; the opening has real clearance on all four sides.
    props.ballot.position.y -= (ballotAnchor.y - top + .45) * release * release
    // Below the opaque front wall by 4.1; it rests inside, not through the floor.
    props.ballot.position.y = Math.max(top - .45, props.ballot.position.y)
    if (t < 1.1) props.ballot.position.sub(offset)
  }
  props.root.updateMatrixWorld(true)
  return { time: t, opacity, leftPalm: left, rightPalm: right, ballotAnchor }
}

export function displayTime(action: ScenarioActionName, time: number, reduced: boolean, playing: boolean) {
  if (!reduced || !playing) return clampTime(time)
  return time < 6 ? ACTION_META[action].reducedFrame : ACTION_SECONDS
}

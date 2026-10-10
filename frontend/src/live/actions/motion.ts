import { Euler, Quaternion, Vector3, type Object3D } from 'three'
import { ACTION_META, ACTION_SECONDS, clampTime, ramp, type ScenarioActionName, type ScenarioPreview } from './definitions.ts'
import { path, type Key, type ScenarioRig } from './rig.ts'
import type { ScenarioProps } from './props.ts'

const LEFT: Record<ScenarioActionName, readonly Key[]> = {
  mule: [[0,.83,-.8,.38],[1.1,.78,-.45,.43],[2,.98,-.25,.34],[2.7,1.02,-.5,.36],[3.4,1.02,-.5,.36],[4.3,.65,-.6,.4],[5.4,.65,-.55,.42],[6.3,.83,-.8,.38]],
  bribe: [[0,.55,-.6,.3],[1.2,.79,-.44,.42],[1.8,.86,-.38,.40],[2.6,1.05,-.32,.34],[3.7,1.12,-.32,.34],[4.15,1.1,-.32,.34],[4.6,.6,-.30,.25],[5.8,.55,-.60,.3],[6.3,.83,-.8,.38]],
  vote: [[0,.55,-.6,.25],[.9,.65,-.45,.34],[1.25,.86,-.40,.39],[2.0,1.14,-.20,.33],[2.8,1.27,-.20,.30],[3.35,1.27,-.20,.30],[3.5,.78,-.32,.15],[3.65,.65,-.30,.10],[4.3,.55,-.5,.22],[5.8,.55,-.60,.3],[6.3,.83,-.8,.38]],
}
const RIGHT: Record<ScenarioActionName, readonly Key[]> = {
  mule: [[0,-.79,-.75,.4],[1.2,-.72,-.72,.42],[2.5,-.70,-.72,.42],[3.5,-.70,-.65,.40],[4.3,-.70,-.70,.40],[5.4,-.76,-.72,.40],[6.3,-.79,-.75,.4]],
  bribe: [[0,-.79,-.75,.4],[1.6,-.70,-.70,.42],[2.4,-.64,-.54,.42],[3.6,-.66,-.61,.41],[4.5,-.68,-.74,.4],[6.3,-.79,-.75,.4]],
  vote: [[0,-.79,-.75,.4],[1.4,-.65,-.70,.44],[2.1,-.57,-.64,.46],[3.5,-.64,-.62,.44],[4.55,-.72,-.47,.37],[5.25,-.70,-.55,.41],[6.3,-.79,-.75,.4]],
}
// Each key is [seconds, yaw, side lean, forward lean]. Only small additive
// offsets are applied to the sampled idle; hovering never accumulates a drift.
const BODY: Record<ScenarioActionName, readonly Key[]> = {
  mule: [[0,0,0,0],[.8,-.20,.04,-.035],[1.4,.18,-.035,-.015],[2.2,.22,-.06,.07],[2.65,.13,-.035,.035],[3.4,.20,-.07,.085],[4.3,.08,-.02,.02],[4.9,-.14,.06,-.05],[5.4,.04,-.01,-.02],[6.6,0,0,0]],
  bribe: [[0,0,0,0],[.85,.12,-.025,-.04],[1.35,-.19,.035,-.02],[1.9,.08,-.04,.03],[2.8,.20,-.075,.08],[3.4,.22,-.085,.09],[3.8,.18,-.065,.06],[4.2,.20,-.06,.065],[4.7,.10,-.025,.02],[5.3,-.12,.04,-.045],[5.65,.03,-.015,-.025],[6.6,0,0,0]],
  vote: [[0,0,0,0],[1.0,-.035,.02,-.03],[1.7,.055,-.025,-.025],[2.8,.13,-.055,.035],[3.4,.14,-.065,.055],[4.2,.08,-.03,.005],[5.0,-.025,.025,-.025],[6.6,0,0,0]],
}

function pose(rig: ScenarioRig, action: ScenarioActionName, time: number, reduced: boolean) {
  // End pose is sampled from idle time zero, exactly matching the starting pose.
  rig.idleAt(time >= ACTION_SECONDS ? 0 : time, reduced)
  const weight = ramp(time, .0, .65) * (1 - ramp(time, 5.85, 6.6))
  const body = rig.bone('body'), turn = path(Math.max(0, time - .25), BODY[action]).multiplyScalar(1 - ramp(time, 6.35, 6.6))
  body.quaternion.multiply(new Quaternion().setFromEuler(new Euler(turn.z, turn.x, turn.y, 'YXZ')))
  // Quiet the idle hover while acting, so the parcel's catch supplies the dip.
  body.position.x += -.5 * turn.y
  body.position.z += .35 * turn.z
  const catchDip = action === 'mule' ? ramp(time, 3.3, 3.55) * (1 - ramp(time, 3.65, 4.15)) : 0
  body.position.y *= 1 - .7 * weight
  body.position.y -= .04 * catchDip
  rig.reach('left', path(time, LEFT[action]), weight)
  rig.reach('right', path(time, RIGHT[action]), weight)
  if (time > .55 && time < 6.45) {
    // Keep the familiar sly eyes; no triumph or economy-result signal.
    rig.bone('left-lid').scale.y = .3
    rig.bone('right-lid').scale.y = .3
    rig.bone('mouth').scale.y = .6 + .16 * ramp(time, 4.7, 5.0) * (1 - ramp(time, 5.55, 6.15))
  }
  if (action !== 'vote') {
    const glance = action === 'mule'
      ? path(time + .2, [[0,0,0,0],[.8,-.055,.01,0],[1.4,.06,0,0],[2.5,.035,-.035,0],[4.35,.03,-.045,0],[4.75,-.055,.015,0],[5.3,.035,0,0],[6.6,0,0,0]])
      : path(time + .2, [[0,0,0,0],[.85,.045,0,0],[1.35,-.06,.015,0],[2.4,.055,-.035,0],[3.4,.045,-.02,0],[4.8,.055,-.04,0],[5.35,-.045,.01,0],[6.6,0,0,0]])
    const attention = ramp(time, 0, .35) * (1 - ramp(time, 5.85, 6.6))
    for (const side of ['left', 'right']) {
      rig.bone(`${side}-pupil`).position.x += glance.x * attention
      rig.bone(`${side}-pupil`).position.y += glance.y * attention
      rig.bone(`${side}-brow`).position.y += (side === 'left' ? .03 : -.01) * weight
    }
    const surprise = action === 'mule' ? ramp(time, 3.55, 3.8) * (1 - ramp(time, 4.1, 4.4)) : 0
    rig.bone('left-lid').scale.y *= 1 - .8 * surprise
    rig.bone('right-lid').scale.y *= 1 - .8 * surprise
    rig.bone('mouth').scale.y += .28 * surprise
  } else {
    const look = ramp(time, .05, .6) * (1 - ramp(time, 4.7, 5.5))
    const down = ramp(time, 3.35, 3.8)
    for (const side of ['left', 'right']) {
      rig.bone(`${side}-pupil`).position.x += .035 * look
      rig.bone(`${side}-pupil`).position.y -= (.016 + .022 * down) * look
    }
  }
  // The tail trails the torso; its tip follows last, including after the ballot drop.
  for (let i = 0; i <= 7; i++) {
    const lag = .38 + i * .035
    const follow = path(Math.max(0, time - lag), BODY[action])
    const tail = rig.bone(i ? `tail-${i}` : 'tail')
    tail.rotation.z += weight * follow.y * -.55
    tail.rotation.x += weight * (follow.z * .16 + .018 * catchDip)
  }
  if (action === 'mule') {
    const contact = path(time, [[0,.8,-.5,.6],[1.5,.99,-.22,.64],[2.2,.99,-.22,.64],[2.65,1,-.43,.64],[3.3,1,-.43,.64],[3.55,.92,-.50,.64],[3.85,.80,-.43,.64],[4.3,.62,-.50,.64],[5.65,.58,-.55,.64],[6.6,.8,-.6,.64]])
    const hold = ramp(time, .65, 1.5) * (1 - ramp(time, 5.8, 6.4))
    if (hold > 0) rig.support('left', rig.palmPoint('left').lerp(contact, hold))
    const wink = ramp(time, 4.65, 4.82) * (1 - ramp(time, 4.95, 5.12))
    rig.bone('right-lid').scale.y += .7 * wink
    rig.bone('left-brow').rotation.z += .12 * wink
  }
  rig.hero.updateMatrixWorld(true)
}

const UP = new Vector3(0, 1, 0)
function sleeveBetween(node: Object3D, start: Vector3, end: Vector3) {
  node.visible = true
  node.position.copy(start)
  const direction = end.clone().sub(start)
  node.quaternion.setFromUnitVectors(UP, direction.clone().normalize())
  node.scale.y = direction.length()
}

/** The reference cutout is the vendor, while the hero and exchange stay articulated. */
function vendorAt(props: ScenarioProps, t: number) {
  const breathe = .012 * Math.sin(t * 2.5) * ramp(t, .8, 1.3)
  props.donkeyBillboard.visible = props.hasDonkeyTexture
  props.donkeyBillboard.position.set(1.52, -.03 + breathe, 0)
  props.donkeyBillboard.rotation.set(-.05, -.17, -.012 * ramp(t, 3.2, 3.6) * (1 - ramp(t, 4.0, 4.4)))
  // Preserve the supplied 640 × 768 reconstruction's aspect ratio.
  props.donkeyPicture.scale.set(1.85 * 640 / 768, 1.85, 1)
  props.root.updateMatrixWorld(true)
  // Image UVs measured at the upper surface of each illustrated hoof. These
  // anchors also let numerical QA check contact with a flat cutout explicitly.
  const onHoof = (u: number, v: number) => props.root.worldToLocal(props.donkeyPicture.localToWorld(new Vector3(u - .5, v - .5, .022)))
  return { payment: onHoof(.228, .400), parcel: onHoof(.390, .352) }
}

/** Stateless sampling: forward playback, reverse scrubbing and action switches agree. */
export function applyScenario(rig: ScenarioRig, props: ScenarioProps, preview: Pick<ScenarioPreview, 'action' | 'entry'>, time: number, reduced = false) {
  const t = clampTime(time), action = preview.action
  // Sampling the release pose makes the drop anchor independent of frame history.
  // The ballot box stays under the release point throughout the entire scene.
  let ballotAnchor = new Vector3()
  let paymentAnchor = new Vector3()
  if (action === 'mule') {
    pose(rig, action, 2.0, reduced)
    paymentAnchor = rig.contactPoint('left')
  }
  if (action === 'vote') {
    pose(rig, action, 3.35, reduced)
    ballotAnchor = rig.palmPoint('left')
  }
  pose(rig, action, t, reduced)
  let left = action === 'vote' ? rig.palmPoint('left') : rig.contactPoint('left')
  const right = action === 'vote' ? rig.palmPoint('right') : rig.contactPoint('right')
  const opacity = ramp(t, .35, 1.1) * (1 - ramp(t, 5.75, 6.5))
  const entering = 1 - ramp(t, .35, 1.1)
  const leaving = ramp(t, 5.75, 6.5)
  const offset = preview.entry === 'bottom' ? new Vector3(0, -1.6 * (entering + leaving), 0) : new Vector3(2.5 * (entering + leaving), 0, 0)
  props.hideAll()
  props.root.position.copy(offset)
  props.fade(opacity)

  if (action === 'mule') {
    const hooves = vendorAt(props, t)
    props.hat.visible = props.bag.visible = true
    props.hat.position.copy(rig.capPoint()).sub(offset)
    props.hat.scale.set(.91, .70, .92)
    props.hat.quaternion.copy(rig.bone('body').quaternion)
    // The sack belongs to the agent for the entire preview. Donkey costs one
    // peso and grants one good (rules/actions.rs); it is not the Shuttle action.
    props.bag.position.copy(right).add(new Vector3(0, .018, 0)).sub(offset)
    const pay = ramp(t, 2.0, 2.38)
    props.coin.visible = t >= .65 && t < 2.8
    // A short, deliberate flick travels back to the illustrated payment hoof.
    props.coin.position.copy(t < 2 ? left : paymentAnchor).lerp(hooves.payment, pay).add(new Vector3(0, .018, 0))
    props.coin.position.y += .12 * Math.sin(Math.PI * pay)
    props.coin.rotation.y = Math.PI * 2 * pay
    if (t < 1.1) props.coin.position.sub(offset)
    const receive = ramp(t, 2.8, 3.3)
    props.parcel.visible = t >= .8 && t < 6.5
    // The vendor returns one box with a short underarm toss. The agent catches
    // it, absorbs the weight and retains it; there is no outgoing goods delivery.
    props.parcel.position.copy(hooves.parcel).lerp(left, receive).add(new Vector3(0, .018, 0))
    props.parcel.position.y += .10 * Math.sin(Math.PI * receive)
    if (t >= 3.3) props.parcel.position.sub(offset)
  } else if (action === 'bribe') {
    const top = -.80
    props.desk.visible = props.official.visible = props.paperwork.visible = true
    props.desk.position.set(1.48, top, -.04)
    props.desk.scale.set(.86, .84, .86)
    props.official.position.set(1.49, top, -.27)
    props.official.scale.setScalar(1.15)
    const accept = ramp(t, 3.0, 3.65), withdraw = ramp(t, 4.5, 5.2)
    // A suspicious look away, a little bow, then a businesslike nod.
    props.officialTorso.rotation.z = .08 * ramp(t, 2.8, 3.65) * (1 - ramp(t, 4.4, 5.15))
    props.officialHead.rotation.y = path(t, [[0,0,0,0],[1.1,-.30,0,0],[2,.65,0,0],[2.7,-.65,0,0],[3.35,-.3,0,0],[4.5,-.3,0,0],[5.1,0,0,0]]).x
    props.officialHead.rotation.x = .15 * ramp(t, 4.5, 4.8) * (1 - ramp(t, 5.0, 5.25))
    props.officialOtherArm.visible = false
    // Exchange at the official's chest, leaving his face and the seal readable.
    const offered = new Vector3(.99, -.42, .64)
    const offer = ramp(t, 1.65, 2.8) * (1 - ramp(t, 4.05, 4.45))
    const donor = left.clone().lerp(offered, offer)
    donor.x -= .32 * ramp(t, 4.0, 4.35) * (1 - ramp(t, 4.5, 4.8))
    if (t >= .65 && t < 5.8) rig.support('left', donor)
    left = rig.contactPoint('left')
    const receive = offered.clone().add(new Vector3(.46, 0, .015))
    const resting = new Vector3(1.48, top + .11, .10)
    const recipient = new Vector3(1.70, top + .17, .10).lerp(receive, accept).lerp(resting, withdraw)
    props.officialHand.visible = true
    props.officialHand.position.copy(recipient)
    props.officialHand.scale.setScalar(1.15)
    props.official.updateMatrixWorld(true)
    const shoulder = props.root.worldToLocal(props.officialTorso.localToWorld(new Vector3(-.18, .44, .015)))
    const elbow = shoulder.clone().lerp(recipient, .52).add(new Vector3(.08, -.12, -.075))
    const wrist = recipient.clone().add(new Vector3(.015, -.045, -.11))
    sleeveBetween(props.officialUpperArm, shoulder, elbow)
    sleeveBetween(props.officialForearm, elbow, wrist)
    for (const sleeve of [props.officialUpperArm, props.officialForearm]) {
      sleeve.scale.x = sleeve.scale.z = 1.15
    }
    // Both palms share the envelope edge before the donor lets go. No flight
    // across the desk: the recipient's palm remains under it throughout withdrawal.
    const give = ramp(t, 3.7, 4.05)
    props.envelope.visible = t >= .65 && t < 5.95
    props.envelope.position.copy(left).add(new Vector3(.14 * accept, .018, 0))
    props.envelope.position.lerp(recipient.clone().add(new Vector3(-.12, .018, 0)), give)
    if (t < 1.1) props.envelope.position.sub(offset)
    props.paperwork.position.set(1.41, top + .01 + .28 * ramp(t, 4.1, 4.65) - .08 * ramp(t, 4.95, 5.4), .10)
    props.coverHand.visible = true
    props.coverHand.scale.setScalar(1.15)
    props.coverHand.position.copy(props.paperwork.position).add(new Vector3(.14, .064, 0))
    const coverShoulder = props.root.worldToLocal(props.officialTorso.localToWorld(new Vector3(.19, .43, .015)))
    const coverWrist = props.coverHand.position.clone().add(new Vector3(.01, .015, -.06))
    const coverElbow = coverShoulder.clone().lerp(coverWrist, .5).add(new Vector3(.10, -.09, -.05))
    sleeveBetween(props.coverUpperArm, coverShoulder, coverElbow)
    sleeveBetween(props.coverForearm, coverElbow, coverWrist)
    // Paperwork descends after the envelope arrives; the absurdly casual cover-up
    // leaves the red seal briefly visible at the edge of the stack.
    props.paperwork.rotation.y = -.08
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

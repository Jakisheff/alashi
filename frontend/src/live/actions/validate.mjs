// No browser/build/game calls. Run from frontend: node --experimental-strip-types src/live/actions/validate.mjs
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { Box3, Group, Matrix4, PerspectiveCamera, Raycaster, Texture, Vector3 } from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from 'meshoptimizer'
import { ACTION_SECONDS, ACTION_META, scenarioFraming } from './definitions.ts'
import { createRig } from './rig.ts'
import { createScenarioProps } from './props.ts'
import { applyScenario, displayTime } from './motion.ts'

const bytes = await readFile(new URL('../../../public/models/desk-genie.glb', import.meta.url))
const glb = await new GLTFLoader().setMeshoptDecoder(MeshoptDecoder).parseAsync(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), '')
const rig = createRig(glb.scene, glb.animations)
const props = createScenarioProps(), root = new Group()
root.add(glb.scene, props.root)
props.setDonkeyTexture(new Texture()) // Geometry-only stand-in for the browser-owned cached map.
const rest = new Map(Array.from(rig.bones, ([name, bone]) => [name, bone.position.toArray()]))
const snapshot = () => {
  const values = []
  root.traverse((o) => values.push([o.name, o.visible, ...o.position.toArray(), ...o.quaternion.toArray(), ...o.scale.toArray()]))
  return values
}
const bounds = (o) => new Box3().setFromObject(o)
const collisionWarnings = {}
const framing = new Group()
framing.rotation.y = .35
const studioFraming = process.env.ALASHI_STUDIO_FRAMING === '1'
const cameras = (studioFraming ? [.60, .80, .9375, 1.20, 2.1] : [.60, .80, 1, 1.20]).map((aspect) => {
  const camera = new PerspectiveCamera(32, aspect, .1, 100)
  const k = Math.max(1, .85 / aspect) * (studioFraming ? 1.14 : 1)
  const targetY = studioFraming ? -.7 : -.4
  camera.position.set(.15 + 1.05 * k, targetY + .3 * k, 5.8 * k)
  camera.lookAt(.15, targetY, 0); camera.updateMatrixWorld(true)
  return camera
})
let worstFrame = { extent: 0 }
function checkFraming(action, t) {
  if (t < 1.1 || t > 5.75) return
  const composition = scenarioFraming(action)
  framing.position.set(composition.x, .15, 0)
  framing.scale.setScalar(composition.scale); framing.updateMatrixWorld(true)
  root.traverseVisible((o) => {
    if (!o.isMesh) return
    o.geometry.computeBoundingBox()
    const box = o.geometry.boundingBox
    const transform = new Matrix4().multiplyMatrices(framing.matrixWorld, o.matrixWorld)
    for (const x of [box.min.x, box.max.x]) for (const y of [box.min.y, box.max.y]) for (const z of [box.min.z, box.max.z]) {
      const vertex = new Vector3(x, y, z).applyMatrix4(transform)
      for (const camera of cameras) {
        const projected = vertex.clone().project(camera)
        const extent = Math.max(Math.abs(projected.x), Math.abs(projected.y))
        if (extent > worstFrame.extent) worstFrame = { extent, action, t, mesh: o.name || o.parent.name, aspect: camera.aspect }
      }
    }
  })
}
function supportOverlap(prop, hand, message) {
  const item = bounds(prop), palm = bounds(hand), ray = new Raycaster()
  const centerSupport = prop === props.bag || prop === props.coin
  const center = prop.getWorldPosition(new Vector3())
  const loX = Math.max(item.min.x, palm.min.x), hiX = Math.min(item.max.x, palm.max.x)
  const loZ = Math.max(item.min.z, palm.min.z), hiZ = Math.min(item.max.z, palm.max.z)
  assert(loX < hiX && loZ < hiZ, message + ': no support footprint')
  let gap = Infinity
  for (const x of centerSupport ? [center.x] : [loX + .005, (loX + hiX) / 2, hiX - .005]) for (const z of centerSupport ? [center.z] : [loZ + .005, (loZ + hiZ) / 2, hiZ - .005]) {
    ray.set(new Vector3(x, item.min.y + .3, z), new Vector3(0, -1, 0))
    const hit = ray.intersectObject(hand, true)[0]
    if (hit) gap = Math.min(gap, item.min.y - hit.point.y)
  }
  assert(gap >= -.015 && gap <= .065, message + ': mesh surface gap ' + gap)
}
// Narrow phase tests actual finger/thumb surface vertices against the analytic
// solids used by these procedural props. A round sack's empty AABB corners are
// not collisions. The flat cutout has no collision volume; its contact is UV-based.
function penetrates(hand, prop) {
  const type = prop.geometry.type
  if (type === 'PlaneGeometry') return false
  const transform = new Matrix4().copy(prop.matrixWorld).invert().multiply(hand.matrixWorld)
  const scale = prop.getWorldScale(new Vector3())
  const epsilon = .015 / Math.min(scale.x, scale.y, scale.z)
  const attribute = hand.geometry.attributes.position
  for (let i = 0; i < attribute.count; i++) {
    const v = new Vector3().fromBufferAttribute(attribute, i).applyMatrix4(transform)
    if (type === 'BoxGeometry' && Math.max(Math.abs(v.x), Math.abs(v.y), Math.abs(v.z)) < .5 - epsilon) return true
    if (type === 'SphereGeometry' && v.length() < 1 - epsilon) return true
    if (type === 'CylinderGeometry' && Math.abs(v.y) < .5 - epsilon && Math.hypot(v.x, v.z) < 1 - epsilon) return true
    if (type === 'TorusGeometry' && Math.hypot(Math.hypot(v.x, v.y) - 1, v.z) < .06 - epsilon) return true
  }
  return false
}
let frames = 0
for (const action of Object.keys(ACTION_META)) {
  for (const entry of ['bottom', 'side']) for (const reduced of [false, true]) {
    const preview = { action, entry }
    applyScenario(rig, props, preview, 0, reduced)
    const initial = Array.from(rig.bones.values(), (bone) => [...bone.position.toArray(), ...bone.quaternion.toArray(), ...bone.scale.toArray()])
    for (let frame = 0; frame <= ACTION_SECONDS * 60; frame++) {
      const t = frame / 60
      applyScenario(rig, props, preview, t, reduced)
      frames++
      root.traverse((o) => assert([...o.position.toArray(), ...o.quaternion.toArray(), ...o.scale.toArray()].every(Number.isFinite), `${action}@${t}: nonfinite ${o.name}`))
      for (const [name, bone] of rig.bones) if (/^(left|right)-(arm|forearm|hand|thumb|point|middle|curl)/.test(name)) {
        assert(bone.position.toArray().every((value, i) => Math.abs(value - rest.get(name)[i]) < 1e-6), `${action}: joint length changed: ${name}`)
      }
      assert(Math.abs(rig.bone('body').position.y) <= .071, 'Body drifted outside the original hover range')
      if (frame % 3 === 0) checkFraming(action, t)
      if (action === 'mule' && t >= 1.1 && t < 2) supportOverlap(props.coin, rig.bone('left-hand'), `Payment@${t}`)
      if (action === 'mule' && t >= 1.1 && t <= 5.75) supportOverlap(props.bag, rig.bone('right-hand'), `Own sack@${t}`)
      if (action === 'mule' && t >= 3.3 && t <= 5.75) supportOverlap(props.parcel, rig.bone('left-hand'), `Received parcel@${t}`)
      if (action === 'mule') {
        const onHoof = (u, v) => props.donkeyPicture.localToWorld(new Vector3(u - .5, v - .5, .022))
        const atHoof = (prop, u, v) => assert(prop.getWorldPosition(new Vector3()).distanceTo(onHoof(u, v).add(new Vector3(0, .018, 0))) < 1e-6, `Cutout hoof contact@${t}`)
        if (t >= 2.38 && t < 2.8) atHoof(props.coin, .228, .400)
        if (t >= 1.1 && t <= 2.8) atHoof(props.parcel, .390, .352)
      }
      if (action === 'bribe' && t >= 1.1 && t < 3.7) supportOverlap(props.envelope, rig.bone('left-hand'), `Envelope donor@${t}`)
      if (action === 'bribe' && t >= 4.05 && t <= 5.75) supportOverlap(props.envelope, props.officialHand, `Envelope recipient@${t}`)
      if (action === 'bribe' && t >= 1.1 && t <= 5.75) {
        const elbow = props.officialUpperArm.localToWorld(new Vector3(0, 1, 0))
        assert(elbow.distanceTo(props.officialForearm.getWorldPosition(new Vector3())) < 1e-6, 'Disconnected official elbow')
        const paper = bounds(props.paperwork), cover = bounds(props.coverHand)
        assert(Math.abs(cover.min.y - paper.max.y) < .035, 'Paperwork lost its supporting hand')
      }
      if (props.root.visible) {
        for (const side of ['left', 'right']) {
          const handMeshes = []
          rig.bone(`${side}-hand`).traverse((o) => { if (o.isMesh) handMeshes.push(o) })
          for (const prop of [props.bag, props.parcel, props.envelope, props.officialHand, props.coin, props.donkeyBillboard, props.officialUpperArm, props.officialForearm, props.coverHand, props.coverUpperArm, props.coverForearm, props.paperwork, props.desk, props.urn, props.ballot]) {
            if (!prop.visible) continue
            prop.traverse((o) => {
              if (!o.isMesh) return
              for (const handMesh of handMeshes) {
              const hand = bounds(handMesh)
              const b = bounds(o)
              if (!hand.intersectsBox(b)) continue
              b.intersect(hand)
              const overlap = Math.min(b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z)
              // AABB broad phase is conservative; flag overlap >.015 scene units for visual QA.
              if (overlap > .015 && penetrates(handMesh, o)) {
                const key = `${action}/${entry}/${reduced ? 'reduced' : 'motion'}/${side}/${prop.name}`
                const previous = collisionWarnings[key] ?? { first: t, last: t, max: 0 }
                previous.last = t; previous.max = Math.max(previous.max, overlap); collisionWarnings[key] = previous
              }
              }
            })
          }
        }
      }
      if (action === 'vote' && t >= 3.5 && t <= 4.1) {
        const paper = bounds(props.ballot.getObjectByName('ballot-solid'))
        const urn = props.urn.getWorldPosition(root.position.clone())
        assert(paper.min.x > urn.x - .24 && paper.max.x < urn.x + .24, 'Ballot hits slot side')
        assert(paper.min.z > urn.z - .085 && paper.max.z < urn.z + .085, 'Ballot hits slot edge')
      }
    }
    assert.equal(props.root.visible, false, 'Props must disappear at end')
    const final = Array.from(rig.bones.values(), (bone) => [...bone.position.toArray(), ...bone.quaternion.toArray(), ...bone.scale.toArray()])
    assert.deepEqual(final, initial, `${action}: end must restore idle`)
    // Same frame after an unrelated action and a reverse scrub must be identical.
    for (const t of ACTION_META[action].keyframes) {
      applyScenario(rig, props, preview, t, reduced); const expected = snapshot()
      applyScenario(rig, props, { action: action === 'vote' ? 'mule' : 'vote', entry }, 4.2, reduced)
      applyScenario(rig, props, preview, 6.7, reduced)
      applyScenario(rig, props, preview, t, reduced)
      assert.deepEqual(snapshot(), expected, `${action}@${t}: scrub depends on previous frame`)
    }
    assert.equal(displayTime(action, 2, true, false), 2, 'Paused reduced-motion scrub must remain inspectable')
    assert.equal(displayTime(action, ACTION_SECONDS, true, true), ACTION_SECONDS)
  }
}
console.log('Worst opaque-scene frustum extent (limit .98):', worstFrame)
assert(worstFrame.extent < .98, 'Opaque scene leaves the portrait frustum')
props.dispose()
console.log(`Scenario QA: ${frames} sampled frames; finite transforms, unchanged finger/thumb/limb lengths, palm support, reference-hoof contact, connected official arms, four-aspect frustum, deterministic scrubs, idle recovery and ballot-slot clearance passed.`)
console.log('Conservative hand/prop overlap candidates:', JSON.stringify(collisionWarnings))
assert.equal(Object.keys(collisionWarnings).length, 0, 'Resolve overlap candidates before browser QA')

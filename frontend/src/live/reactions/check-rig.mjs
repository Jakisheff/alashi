// Run from frontend: node --experimental-strip-types src/live/reactions/check-rig.mjs
// Numerical GLB checks only; browser composition/contact QA belongs to the parent.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { Box3, DoubleSide, Matrix4, Mesh, MeshBasicMaterial, PerspectiveCamera, Quaternion, Raycaster, Vector3 } from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { MeshoptDecoder } from 'meshoptimizer'
import { createLivingRig } from './rig.ts'
import { REACTION_KEYFRAMES, REACTION_REDUCED_FRAME, REACTION_SECONDS, LIVING_IDLE_SECONDS } from './definitions.ts'
import { createVictoryFireworks } from './fireworks.ts'

const bytes = await readFile(new URL('../../../public/models/desk-genie.glb', import.meta.url))
const glb = await new GLTFLoader().setMeshoptDecoder(MeshoptDecoder).parseAsync(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), '')
function snapshot(object) {
  const result = []
  object.traverse((o) => result.push(...o.position.toArray(), ...o.quaternion.toArray(), ...o.scale.toArray()))
  return result
}
const sourceBefore = snapshot(glb.scene)
const hero = clone(glb.scene), rig = createLivingRig(hero, glb.animations)
const jointNames = [...rig.bones.keys()]
rig.idleAt(0)
const jointPositions = new Map([...rig.bones].filter(([name]) => /-(arm|forearm|hand|point|middle|curl|thumb)(-mid|-tip)?$/.test(name)).map(([name, node]) => [name, node.position.clone()]))
const core = new Box3(new Vector3(-.48, -.10, -.30), new Vector3(.48, .60, .45))
const casing = []
hero.traverse((o) => { if (o.isMesh && /^(body-shell|screen|left-ear|right-ear)/.test(o.name)) casing.push(o) })
// Actual closed palm surfaces, separate material so cached hero stays untouched.
// Conservative casing AABBs alone cannot catch a fingertip folded into its palm.
const collisionMaterial = new MeshBasicMaterial({ side: DoubleSide })
const palms = ['left', 'right'].flatMap((side) => ['palm', 'palm-button'].map((part) => {
  const source = hero.getObjectByName(`${side}-${part}`)
  assert.ok(source?.isMesh)
  return { source, mesh: new Mesh(source.geometry, collisionMaterial), box: new Box3() }
}))
const tips = []
for (const side of ['left', 'right']) for (const part of ['point', 'middle', 'curl', 'thumb']) rig.bone(`${side}-${part}-tip`).traverse((o) => { if (o.isMesh) tips.push(o) })
const ray = new Raycaster(), direction = new Vector3(1, .173, .081).normalize(), vertex = new Vector3()
let checked = 0
for (const kind of Object.keys(REACTION_KEYFRAMES)) {
  let previous = null
  for (let frame = 0; frame <= REACTION_SECONDS * 30; frame++) {
    const time = frame / 30
    rig.reactionAt(kind, time, time)
    hero.updateMatrixWorld(true)
    const inverseBody = rig.bone('body').matrixWorld.clone().invert()
    const casingBounds = casing.map((o) => ({ name: o.name, bounds: new Box3().setFromBufferAttribute(o.geometry.attributes.position)
      .applyMatrix4(new Matrix4().multiplyMatrices(inverseBody, o.matrixWorld)) }))
    // Exported idle tracks carry ~2e-7 quantization noise in finger positions.
    for (const [name, position] of jointPositions) assert.ok(rig.bone(name).position.distanceTo(position) < 1e-5, `${kind} ${time}: translated socket ${name}`)
    assert.ok(snapshot(hero).every(Number.isFinite), `${kind} ${time}: invalid transform`)
    const joints = [...rig.bones.values()].map((b) => b.quaternion.clone())
    if (previous) joints.forEach((q, i) => assert.ok(q.angleTo(previous[i]) < .35, `${kind} ${time}: abrupt joint rotation at ${jointNames[i]} (${q.angleTo(previous[i]).toFixed(3)} rad)`))
    previous = joints
    for (const side of ['left', 'right']) rig.bone(`${side}-hand`).traverse((o) => {
      if (!o.isMesh) return
      const bounds = new Box3().setFromBufferAttribute(o.geometry.attributes.position)
        .applyMatrix4(new Matrix4().multiplyMatrices(inverseBody, o.matrixWorld))
      assert.ok(!bounds.intersectsBox(core), `${kind} ${time}: ${o.name} enters inner casing`)
      for (const collider of casingBounds) assert.ok(!bounds.intersectsBox(collider.bounds), `${kind} ${time}: ${o.name} overlaps ${collider.name}`)
    })
    for (const palm of palms) { palm.mesh.matrixWorld.copy(palm.source.matrixWorld); palm.box.setFromObject(palm.source) }
    for (const tip of tips) {
      const vertices = tip.geometry.attributes.position
      for (let i = 0; i < vertices.count; i++) {
        vertex.fromBufferAttribute(vertices, i).applyMatrix4(tip.matrixWorld)
        for (const palm of palms) {
          if (!palm.box.containsPoint(vertex)) continue
          ray.set(vertex, direction)
          const hits = ray.intersectObject(palm.mesh, false).map((h) => h.distance)
            .filter((distance, i, all) => i === 0 || distance - all[i - 1] > 1e-6)
          assert.ok(hits.length % 2 === 0 || hits[0] < .002, `${kind} ${time}: fingertip ${tip.name} penetrates ${palm.source.name}`)
        }
      }
    }
    checked++
  }
  for (const { time } of REACTION_KEYFRAMES[kind]) {
    rig.reactionAt(kind, time, time)
    const expected = snapshot(hero)
    // Different action, later frame, then a backwards seek must exactly restore.
    rig.reactionAt(kind === 'facepalm' ? 'thumbsUp' : 'facepalm', 3.1, 3.1)
    rig.reactionAt(kind, time, time)
    assert.deepEqual(snapshot(hero), expected, `${kind} ${time}: seek retained a previous pose`)
  }
  for (const time of [0, REACTION_SECONDS]) {
    rig.idleAt(time)
    const idle = snapshot(hero)
    rig.reactionAt(kind, time, time)
    assert.deepEqual(snapshot(hero), idle, `${kind}: endpoint differs from living idle`)
  }
}
rig.reactionAt('thumbsUp', 2.65, 2.65)
hero.updateMatrixWorld(true)
const thumb = rig.bone('left-thumb'), tip = rig.bone('left-thumb-tip')
const axis = tip.getWorldPosition(new Vector3()).sub(thumb.getWorldPosition(new Vector3())).normalize()
axis.applyQuaternion(rig.bone('body').getWorldQuaternion(new Quaternion()).invert())
assert.ok(axis.y > .8, 'Thumb should point upward, with a natural forearm lean')
const hand = rig.bone('left-hand'), forearm = rig.bone('left-forearm')
const handAxis = new Vector3(0, -1, 0).applyQuaternion(hand.getWorldQuaternion(new Quaternion()))
const forearmAxis = hand.getWorldPosition(new Vector3()).sub(forearm.getWorldPosition(new Vector3())).normalize()
assert.ok(handAxis.dot(forearmAxis) > .999, 'Thumbs up bends the wrist away from the forearm')
assert.ok(hand.quaternion.angleTo(new Quaternion()) < .01, 'Thumbs up twists the wrist at its socket')
rig.reactionAt('shrug', 2.65, 2.65); hero.updateMatrixWorld(true)
for (const side of ['left', 'right']) {
  const hand = rig.bone(`${side}-hand`), elbow = rig.bone(`${side}-forearm`)
  const arm = hand.getWorldPosition(new Vector3()).sub(elbow.getWorldPosition(new Vector3()))
    .applyQuaternion(rig.bone('body').getWorldQuaternion(new Quaternion()).invert())
  const angle = Math.atan2(arm.y, Math.hypot(arm.x, arm.z)) * 180 / Math.PI
  assert.ok(angle > 40 && angle < 52, 'Shrug forearm should rise about 45 degrees')
  const normal = new Vector3(0, 0, 1).applyQuaternion(hand.getWorldQuaternion(new Quaternion()))
  assert.ok(normal.y > .999, 'Shrug palm must stay horizontal while the body tilts')
}
rig.reactionAt('thinking', 2.65, 2.65)
assert.ok(rig.bone('right-hand').quaternion.angleTo(new Quaternion()) < .01, 'Thinking must keep its wrist neutral')
for (const kind of ['thinking', 'lookout']) {
  const time = kind === 'thinking' ? .3 : .1
  rig.idleAt(time)
  const body = rig.bone('body').quaternion.clone(), eye = rig.bone('left-pupil').position.clone()
  rig.reactionAt(kind, time, time)
  assert.ok(rig.bone('left-pupil').position.distanceTo(eye) > .0001, `${kind}: eyes must anticipate the torso`)
  assert.deepEqual(rig.bone('body').quaternion.toArray(), body.toArray(), `${kind}: torso moved before its preparation`)
}
rig.idleAt(.8)
const leftArm = rig.bone('left-arm').quaternion.clone(), rightArm = rig.bone('right-arm').quaternion.clone()
rig.reactionAt('victory', .8, .8)
assert.ok(rig.bone('left-arm').quaternion.angleTo(leftArm) > rig.bone('right-arm').quaternion.angleTo(rightArm), 'Victory must lead with one arm')
function danceHands(time) {
  rig.reactionAt('dance', time, 0); hero.updateMatrixWorld(true)
  const inverseBody = rig.bone('body').matrixWorld.clone().invert()
  return ['left', 'right'].map((side) => {
    const hand = rig.bone(`${side}-hand`), forearm = rig.bone(`${side}-forearm`)
    const handAxis = new Vector3(0, -1, 0).applyQuaternion(hand.getWorldQuaternion(new Quaternion()))
    const armAxis = hand.getWorldPosition(new Vector3()).sub(forearm.getWorldPosition(new Vector3())).normalize()
    assert.ok(handAxis.dot(armAxis) > .999, 'Dance fist must keep a neutral wrist')
    const thumbAxis = rig.bone(`${side}-thumb-tip`).getWorldPosition(new Vector3()).sub(rig.bone(`${side}-thumb`).getWorldPosition(new Vector3())).normalize()
      .applyQuaternion(hand.getWorldQuaternion(new Quaternion()).invert())
    assert.ok(thumbAxis.x * (side === 'left' ? 1 : -1) < -.7, 'Dance thumb must wrap across the fist')
    return hand.getWorldPosition(new Vector3()).applyMatrix4(inverseBody)
  })
}
const firstBeat = danceHands(.9), secondBeat = danceHands(1.37)
assert.ok(firstBeat[0].z - secondBeat[0].z > .15 && secondBeat[1].z - firstBeat[1].z > .15, 'Dance fists must alternate forward and back')
rig.reactionAt('dance', .9, 0)
assert.ok(rig.bone('body').position.x > .04, 'Dance weight shift must follow the first fist')
assert.ok(rig.bone('mouth').scale.y > 0, 'Dance should begin with a smirk')
rig.reactionAt('dance', 1.37, 0)
assert.ok(rig.bone('body').position.x < -.04, 'Dance must transfer weight to the opposite side')
rig.reactionAt('dance', 2.8, 0)
assert.ok(rig.bone('mouth').scale.y < 0 && rig.bone('mouth').scale.x < .7, 'Dance must change to compressed, downturned lips')
rig.reactionAt('dance', REACTION_REDUCED_FRAME.dance, 1, true)
const reducedDance = snapshot(hero)
rig.reactionAt('dance', REACTION_REDUCED_FRAME.dance, 19, true)
assert.deepEqual(snapshot(hero), reducedDance, 'Reduced Dance must hold one still pose')
rig.idleAt(0)
const origin = snapshot(hero)
rig.idleAt(LIVING_IDLE_SECONDS)
const seam = snapshot(hero)
assert.ok(origin.every((n, i) => Math.abs(n - seam[i]) < 1e-6), 'Idle loop seam')
rig.idleAt(5.3)
assert.ok(Math.abs(rig.bone('body').quaternion.y) > .15, 'Idle glance must turn entire body')
rig.idleAt(16.3)
assert.ok(rig.bone('body').quaternion.y > .1, 'Opposite idle turn missing')
rig.idleAt(7, true)
const still = snapshot(hero)
rig.idleAt(19, true)
assert.deepEqual(snapshot(hero), still, 'Reduced idle is not still')
assert.deepEqual(snapshot(glb.scene), sourceBefore, 'Cached hero was mutated')
collisionMaterial.dispose()
const fireworks = createVictoryFireworks()
fireworks.update(2.3, true)
const burst = fireworks.group.children.filter((c) => c.visible).map((c) => [...c.geometry.attributes.position.array])
assert.ok(burst.length > 0 && burst.flat().every(Number.isFinite))
fireworks.update(4.9, true)
assert.ok(fireworks.group.children.every((c) => !c.visible), 'Fireworks must finish before idle')
fireworks.update(2.3, true)
assert.deepEqual(fireworks.group.children.filter((c) => c.visible).map((c) => [...c.geometry.attributes.position.array]), burst, 'Fireworks seek is not deterministic')
fireworks.update(2.3, false)
assert.equal(fireworks.group.visible, false, 'Reduced motion / non-victory must hide fireworks')
// Include the sparks themselves: a valid hero pose can still crop a burst at
// the portrait canvas edge. Match Scene's camera across narrow/wide viewports.
const studioFraming = process.env.ALASHI_STUDIO_FRAMING === '1'
for (const aspect of (studioFraming ? [.8, .9375, 1, 1.15, 2.1] : [.8, .9375, 1, 1.15])) {
  const camera = new PerspectiveCamera(32, aspect, .1, 100)
  const k = Math.max(1, .85 / aspect) * (studioFraming ? 1.14 : 1)
  const targetY = studioFraming ? -.7 : -.4
  camera.position.set(.15 + 1.05 * k, targetY + .3 * k, 5.8 * k)
  camera.lookAt(.15, targetY, 0); camera.updateMatrixWorld(true)
  const placement = new Matrix4().makeRotationY(.35).setPosition(0, .15, 0)
  const cameraClip = camera.projectionMatrix.clone().multiply(camera.matrixWorldInverse).multiply(placement), meshClip = new Matrix4()
  const danceMeshes = []; hero.traverse((o) => {
    if (o.isMesh && o.visible) danceMeshes.push({ mesh: o, bounds: o.isSkinnedMesh || o.morphTargetInfluences?.some(Boolean) ? null : new Box3().setFromBufferAttribute(o.geometry.attributes.position) })
  })
  for (let frame = 0; frame <= REACTION_SECONDS * 30; frame++) {
    rig.reactionAt('dance', frame / 30, frame / 30); hero.updateMatrixWorld(true)
    for (const { mesh, bounds } of danceMeshes) {
      meshClip.multiplyMatrices(cameraClip, mesh.matrixWorld)
      // A projected mesh box proves containment; inspect vertices only near an edge.
      let nearEdge = !bounds
      if (bounds) for (const x of [bounds.min.x, bounds.max.x]) for (const y of [bounds.min.y, bounds.max.y]) for (const z of [bounds.min.z, bounds.max.z]) {
        vertex.set(x, y, z).applyMatrix4(meshClip)
        if (Math.abs(vertex.x) >= .99 || Math.abs(vertex.y) >= .99) nearEdge = true
      }
      if (!nearEdge) continue
      for (let i = 0; i < mesh.geometry.attributes.position.count; i++) {
        mesh.getVertexPosition(i, vertex).applyMatrix4(meshClip)
        if (Math.abs(vertex.x) >= .99 || Math.abs(vertex.y) >= .99) assert.fail(`Dance crops ${mesh.name} at ${aspect}, ${frame / 30}s (${vertex.x.toFixed(3)}, ${vertex.y.toFixed(3)})`)
      }
    }
    fireworks.update(frame / 30, true); fireworks.group.updateMatrixWorld(true)
    for (const child of fireworks.group.children.filter((c) => c.visible)) {
      const positions = child.geometry.attributes.position
      for (let i = 0; i < positions.count; i++) {
        const p = new Vector3().fromBufferAttribute(positions, i).applyMatrix4(child.matrixWorld).project(camera)
        assert.ok(Math.abs(p.x) < .99 && Math.abs(p.y) < .99, `Fireworks crop at ${aspect}, ${frame / 30}s`)
      }
    }
  }
}
fireworks.dispose()
console.log(`PASS: ${checked} GLB frames, sockets, finite transforms, rotation continuity, hand/casing bounds and fingertip/palm surface clearance (2mm), backward seeks, idle endpoints/seam, neutral wrists, alternating Dance fists and weight, facial change, reduced Dance, body turns, reduced idle, cached scene isolation, deterministic bounded fireworks and Dance framing.`)

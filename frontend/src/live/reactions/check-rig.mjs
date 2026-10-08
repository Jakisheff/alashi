// Run from frontend: node --experimental-strip-types src/live/reactions/check-rig.mjs
// Numerical GLB checks only; browser composition/contact QA belongs to the parent.
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { Box3, Matrix4, Quaternion, Vector3 } from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { MeshoptDecoder } from 'meshoptimizer'
import { createLivingRig } from './rig.ts'
import { REACTION_KEYFRAMES, REACTION_SECONDS, LIVING_IDLE_SECONDS } from './definitions.ts'

const bytes = await readFile(new URL('../../../public/models/desk-genie.glb', import.meta.url))
const glb = await new GLTFLoader().setMeshoptDecoder(MeshoptDecoder).parseAsync(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), '')
function snapshot(object) {
  const result = []
  object.traverse((o) => result.push(...o.position.toArray(), ...o.quaternion.toArray(), ...o.scale.toArray()))
  return result
}
const sourceBefore = snapshot(glb.scene)
const hero = clone(glb.scene), rig = createLivingRig(hero, glb.animations)
rig.idleAt(0)
const jointPositions = new Map([...rig.bones].filter(([name]) => /-(arm|forearm|hand|point|middle|curl|thumb)(-mid|-tip)?$/.test(name)).map(([name, node]) => [name, node.position.clone()]))
const core = new Box3(new Vector3(-.48, -.10, -.30), new Vector3(.48, .60, .45))
const casing = []
hero.traverse((o) => { if (o.isMesh && /^(body-shell|screen|left-ear|right-ear)/.test(o.name)) casing.push(o) })
let checked = 0
for (const kind of Object.keys(REACTION_KEYFRAMES)) {
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
    for (const side of ['left', 'right']) rig.bone(`${side}-hand`).traverse((o) => {
      if (!o.isMesh) return
      const bounds = new Box3().setFromBufferAttribute(o.geometry.attributes.position)
        .applyMatrix4(new Matrix4().multiplyMatrices(inverseBody, o.matrixWorld))
      assert.ok(!bounds.intersectsBox(core), `${kind} ${time}: ${o.name} enters inner casing`)
      for (const collider of casingBounds) assert.ok(!bounds.intersectsBox(collider.bounds), `${kind} ${time}: ${o.name} overlaps ${collider.name}`)
    })
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
assert.ok(axis.y > .999, 'Thumb does not point up relative to the body')
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
console.log(`PASS: ${checked} GLB frames, joint lengths/sockets, finite transforms, hand/casing-screen-ear bounds clearance, backward seeks, idle endpoints/seam, thumb axis, both body turns, reduced idle, cached scene isolation.`)

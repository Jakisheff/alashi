import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { Box3, Euler, Group, Matrix4, Mesh, Quaternion, Texture, Vector3 } from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from 'meshoptimizer'
import { createRig } from '../src/live/actions/rig.ts'
import { BUY_COUNTER, poseTradeHands, purchaseCrateOpacity, purchaseCratePosition } from '../src/live/market/hands.ts'
import { purchaseMotionAt } from '../src/live/market/purchase.ts'

async function load(name: string) {
  const bytes = await readFile(new URL('../public/models/' + name, import.meta.url))
  const loader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder)
  // Texture pixels do not affect surface positions in this geometry check.
  loader.register(() => ({ name: 'geometry-only', loadTexture: async () => new Texture() }))
  return loader.parseAsync(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), '')
}
const hero = await load('desk-genie.glb'), props = await load('experiments/market-sale-props.glb')
const rig = createRig(hero.scene, hero.animations), root = new Group()
const crate = props.scene.getObjectByName('market-crate')!, stand = props.scene.getObjectByName('market-stand')!
root.add(hero.scene, crate, stand)
stand.rotation.set(0, -Math.PI / 2, 0); stand.scale.setScalar(.95)
const palmsUp = new Quaternion().setFromAxisAngle(new Vector3(1, 0, 0), -Math.PI / 2)
const smooth = (x: number) => { const v = Math.max(0, Math.min(1, x)); return v * v * (3 - 2 * v) }
const arms: Mesh[] = [], planks: Mesh[] = []
for (const side of ['left', 'right']) rig.bone(side + '-arm').traverse(o => { if ((o as Mesh).isMesh) arms.push(o as Mesh) })
stand.traverse(o => { if ((o as Mesh).isMesh) planks.push(o as Mesh) })
crate.position.set(0, 0, 0); root.updateMatrixWorld(true)
const crateVolume = new Box3().setFromObject(crate).expandByScalar(-.008)
for (const mesh of [...arms, ...planks]) mesh.geometry.computeBoundingBox()
const transform = new Matrix4(), vertex = new Vector3()
function inside(mesh: Mesh, box: Box3, target: Matrix4) {
  transform.copy(target).invert().multiply(mesh.matrixWorld)
  const positions = mesh.geometry.attributes.position
  for (let i = 0; i < positions.count; i++) {
    vertex.fromBufferAttribute(positions, i).applyMatrix4(transform)
    if (box.containsPoint(vertex)) return true
  }
  return false
}
function sample(t: number, reduced: boolean, sideEntry: boolean) {
  rig.idleAt(t, reduced)
  const body = rig.bone('body'), motion = purchaseMotionAt(t, reduced)
  body.quaternion.multiply(new Quaternion().setFromEuler(new Euler(motion.pitch, motion.yaw, motion.roll, 'YXZ')))
  body.position.add(new Vector3(motion.x, motion.y, motion.z))
  body.position.y -= reduced ? 0 : .07 * Math.sin(t * Math.PI) * (1 - motion.idleWeight)
  poseTradeHands(rig.bones, true, t, body.quaternion.clone().invert().multiply(palmsUp))
  root.updateMatrixWorld(true)
  const left = rig.bone('left-hand').localToWorld(new Vector3(0, -.23, .165))
  const right = rig.bone('right-hand').localToWorld(new Vector3(0, -.23, .165))
  const opacity = smooth(t / .6) * (1 - smooth((t - 5.5) / .9))
  const offset = sideEntry ? new Vector3(3.1 * (1 - opacity), 0, 0) : new Vector3(0, -1.8 * (1 - opacity), 0)
  crate.position.copy(purchaseCratePosition(t, left, right).add(offset))
  const receive = smooth((t - 2.7) / .95)
  crate.rotation.set(0, -.18 * (1 - receive), .045 * Math.sin(receive * Math.PI))
  stand.position.copy(BUY_COUNTER).add(offset)
  root.updateMatrixWorld(true)
  return opacity
}
// Reproduce the reported bug before checking the corrected paths.
sample(1.35, false, false)
crate.position.set(1, -.555, .44); root.updateMatrixWorld(true)
assert(arms.some(mesh => inside(mesh, crateVolume, crate.matrixWorld)), 'Original payment/crate collision was not reproduced')
let frames = 0
for (const reduced of [false, true]) for (const sideEntry of [false, true]) {
  for (let frame = 0; frame <= 6.4 * 60; frame++) {
    const t = frame / 60, opacity = sample(t, reduced, sideEntry)
    if (opacity < .001) continue
    const crateBox = new Box3().setFromObject(crate)
    const counter = planks.map(mesh => ({ mesh, box: mesh.geometry.boundingBox!.clone().expandByScalar(-.008), world: new Box3().setFromObject(mesh) }))
    for (const mesh of arms) {
      const armBox = new Box3().setFromObject(mesh)
      if (purchaseCrateOpacity(t) > .001 && armBox.intersectsBox(crateBox)) assert(!inside(mesh, crateVolume, crate.matrixWorld), `BUY ${t.toFixed(3)} reduced=${reduced} side=${sideEntry}: ${mesh.name} penetrates crate`)
      for (const plank of counter) if (armBox.intersectsBox(plank.world)) assert(!inside(mesh, plank.box, plank.mesh.matrixWorld), `BUY ${t.toFixed(3)}: ${mesh.name} penetrates counter ${plank.mesh.name}`)
    }
    if (t >= 3.65 && t <= 5.45) {
      for (const side of ['left', 'right']) {
        const hand = new Box3().setFromObject(rig.bone(side + '-hand'))
        assert(hand.intersectsBox(crateBox.clone().expandByScalar(.09)), `BUY ${t}: ${side} palm lost support before crate faded`)
      }
    }
    frames++
  }
}
assert.equal(purchaseCrateOpacity(5.5), 0)
console.log(`PASS: original collision reproduced; ${frames} BUY frames, both entrances, reduced-motion poses, crate/counter clearance and support until fade`)

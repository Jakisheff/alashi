import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { AnimationMixer, Group, LoopOnce, Mesh, PerspectiveCamera, Vector3 } from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { MeshoptDecoder } from 'meshoptimizer'
import { cameraPlacement } from '../src/genie/cameraFraming.ts'
import { CLIP_REDUCED_FRAME, CLIP_SECONDS, type GenieClip } from '../src/genie/pose.ts'

const bytes = await readFile(new URL('../public/models/desk-genie.glb', import.meta.url))
const glb = await new GLTFLoader().setMeshoptDecoder(MeshoptDecoder).parseAsync(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), '')
const snapshot = (object: typeof glb.scene) => {
  const values: number[] = []
  object.traverse((o) => values.push(...o.position.toArray(), ...o.quaternion.toArray(), ...o.scale.toArray()))
  return values
}
const original = snapshot(glb.scene), hero = clone(glb.scene), other = clone(glb.scene)
const placement = new Group()
placement.position.y = .15
placement.rotation.y = .35
placement.add(hero)
const meshes: { mesh: Mesh; corners: Vector3[]; face: boolean }[] = []
hero.traverse((object) => {
  if (!(object instanceof Mesh)) return
  object.geometry.computeBoundingBox()
  const box = object.geometry.boundingBox!
  const corners = [box.min.x, box.max.x].flatMap((x) => [box.min.y, box.max.y].flatMap((y) => [box.min.z, box.max.z].map((z) => new Vector3(x, y, z))))
  meshes.push({ mesh: object, corners, face: /screen|brow|pupil|eye|mouth/.test(object.name) })
})
// Match desktop columns and the mobile clamp(14rem, 40svh, 22rem) hero.
const views = [[800, 900], [544, 900], [544, 1366], [390, 337.6], [320, 296], [320, 224], [1000, 800]].map(([width, height]) => {
  const camera = new PerspectiveCamera(32, width / height, .1, 100)
  const { position, target, maxOrbitDistance } = cameraPlacement(width / height)
  // OrbitControls also constrains the fitted default camera on tall screens.
  const lookAt = new Vector3(...target)
  camera.position.set(...position).sub(lookAt).clampLength(3.5, maxOrbitDistance).add(lookAt)
  camera.lookAt(...target)
  camera.updateMatrixWorld(true)
  return { camera, width, height }
})
// Moving the legacy camera must not change live/replay studio framing.
for (const aspect of [.8, .9375, 1, 1.15, 2.1]) {
  const distance = Math.max(1, .85 / aspect) * 1.14
  assert.deepEqual(cameraPlacement(aspect, true), {
    position: [.15 + 1.05 * distance, -.7 + .3 * distance, 5.8 * distance],
    target: [.15, -.7, 0],
    maxOrbitDistance: Math.max(14, Math.hypot(1.05, .3, 5.8) * distance),
  })
}
const projected = new Vector3()
function checkFraming(name: GenieClip, time: number) {
  placement.updateMatrixWorld(true)
  for (const { camera, width, height } of views) for (const { mesh, corners, face } of meshes) {
    const badgeClearance = width < 640 && height <= 352 ? 38 : 42
    const top = face ? Math.min(.96, 1 - 2 * badgeClearance / height) : .96
    const inside = (point: Vector3) => Math.abs(point.x) <= .96 && point.y >= -.96 && point.y <= top && Math.abs(point.z) <= 1
    // A passing transformed box proves all its vertices fit. Only boxes crossing
    // an edge need the exact mesh vertices, avoiding conservative corner failures.
    if (corners.every((point) => inside(projected.copy(point).applyMatrix4(mesh.matrixWorld).project(camera)))) continue
    const points = mesh.geometry.attributes.position
    for (let i = 0; i < points.count; i++) {
      projected.fromBufferAttribute(points, i).applyMatrix4(mesh.matrixWorld).project(camera)
      assert.ok(inside(projected), `${name} ${time}s, ${width}x${height}: ${mesh.name} crosses the framing margin or badge clearance (${projected.x.toFixed(3)}, ${projected.y.toFixed(3)})`)
    }
  }
}
const mixer = new AnimationMixer(hero)
function sample(name: GenieClip, time: number) {
  const clip = glb.animations.find((c) => c.name === name)
  assert.ok(clip, `Missing clip ${name}`)
  mixer.stopAllAction()
  const action = mixer.clipAction(clip).reset().setLoop(LoopOnce, 1).play()
  action.paused = true
  action.time = time
  mixer.update(0)
  return snapshot(hero)
}
sample('act', .45)
checkFraming('act', .45)
let frames = 0
for (const name of Object.keys(CLIP_SECONDS) as GenieClip[]) {
  const duration = CLIP_SECONDS[name]
  assert.ok(CLIP_REDUCED_FRAME[name] >= 0 && CLIP_REDUCED_FRAME[name] <= duration)
  for (let frame = 0; frame <= Math.round(duration * 60); frame++) {
    const pose = sample(name, frame / 60)
    assert.ok(pose.every(Number.isFinite), `${name}: nonfinite frozen pose`)
    checkFraming(name, frame / 60)
    frames++
  }
  const held = sample(name, CLIP_REDUCED_FRAME[name])
  mixer.update(.8)
  assert.deepEqual(snapshot(hero), held, `${name}: reduced-motion pose must stay still`)
  sample(name === 'rejected' ? 'accepted' : 'rejected', .95)
  assert.deepEqual(sample(name, CLIP_REDUCED_FRAME[name]), held, `${name}: reverse seek retained another clip`)
}
assert.deepEqual(snapshot(glb.scene), original, 'Cached model was changed')
assert.deepEqual(snapshot(other), original, 'Independent hero was changed')
mixer.stopAllAction()
console.log(`PASS: ${frames} legacy GLB frames, seven responsive camera bounds with face/badge clearance, frozen reverse seeks, held reduced-motion poses and model isolation`)

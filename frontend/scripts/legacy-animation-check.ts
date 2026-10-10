import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { AnimationMixer, LoopOnce } from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { MeshoptDecoder } from 'meshoptimizer'
import { CLIP_REDUCED_FRAME, CLIP_SECONDS, type GenieClip } from '../src/genie/pose.ts'

const bytes = await readFile(new URL('../public/models/desk-genie.glb', import.meta.url))
const glb = await new GLTFLoader().setMeshoptDecoder(MeshoptDecoder).parseAsync(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), '')
const snapshot = (object: typeof glb.scene) => {
  const values: number[] = []
  object.traverse((o) => values.push(...o.position.toArray(), ...o.quaternion.toArray(), ...o.scale.toArray()))
  return values
}
const original = snapshot(glb.scene), hero = clone(glb.scene), other = clone(glb.scene)
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
let frames = 0
for (const name of Object.keys(CLIP_SECONDS) as GenieClip[]) {
  const duration = CLIP_SECONDS[name]
  assert.ok(CLIP_REDUCED_FRAME[name] >= 0 && CLIP_REDUCED_FRAME[name] <= duration)
  for (let frame = 0; frame <= Math.round(duration * 60); frame++) {
    const pose = sample(name, frame / 60)
    assert.ok(pose.every(Number.isFinite), `${name}: nonfinite frozen pose`)
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
console.log(`PASS: ${frames} legacy GLB frames, frozen reverse seeks, held reduced-motion poses and model isolation`)

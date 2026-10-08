// No browser/build/game calls. Run from frontend: node --experimental-strip-types src/live/actions/validate.mjs
import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { Box3, Group } from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { MeshoptDecoder } from 'meshoptimizer'
import { ACTION_SECONDS, ACTION_META } from './definitions.ts'
import { createRig } from './rig.ts'
import { createScenarioProps } from './props.ts'
import { applyScenario, displayTime } from './motion.ts'

const bytes = await readFile(new URL('../../../public/models/desk-genie.glb', import.meta.url))
const glb = await new GLTFLoader().setMeshoptDecoder(MeshoptDecoder).parseAsync(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength), '')
const rig = createRig(glb.scene, glb.animations)
const props = createScenarioProps(), root = new Group()
root.add(glb.scene, props.root)
const rest = new Map(Array.from(rig.bones, ([name, bone]) => [name, bone.position.toArray()]))
const snapshot = () => {
  const values = []
  root.traverse((o) => values.push([o.name, o.visible, ...o.position.toArray(), ...o.quaternion.toArray(), ...o.scale.toArray()]))
  return values
}
const bounds = (o) => new Box3().setFromObject(o)
const collisionWarnings = {}
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
      for (const [name, bone] of rig.bones) if (/^(left|right)-(arm|forearm|hand|point|middle|curl)/.test(name)) {
        assert(bone.position.toArray().every((value, i) => Math.abs(value - rest.get(name)[i]) < 1e-6), `${action}: joint length changed: ${name}`)
      }
      assert(Math.abs(rig.bone('body').position.y) <= .071, 'Body drifted outside the original hover range')
      if (props.root.visible) {
        for (const side of ['left', 'right']) {
          const hand = bounds(rig.bone(`${side}-hand`))
          for (const prop of [props.bag, props.parcel, props.envelope, props.officialHand, props.desk, props.urn, props.ballot]) {
            if (!prop.visible) continue
            prop.traverse((o) => {
              if (!o.isMesh) return
              const b = bounds(o)
              if (!hand.intersectsBox(b)) return
              b.intersect(hand)
              const overlap = Math.min(b.max.x - b.min.x, b.max.y - b.min.y, b.max.z - b.min.z)
              // AABB broad phase is conservative; flag overlap >.015 scene units for visual QA.
              if (overlap > .015) {
                const key = `${action}/${entry}/${reduced ? 'reduced' : 'motion'}/${side}/${prop.name}`
                const previous = collisionWarnings[key] ?? { first: t, last: t, max: 0 }
                previous.last = t; previous.max = Math.max(previous.max, overlap); collisionWarnings[key] = previous
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
props.dispose()
console.log(`Scenario QA: ${frames} sampled frames; finite transforms, unchanged bone lengths, deterministic scrubs, idle recovery and ballot-slot clearance passed.`)
console.log('Conservative hand/prop overlap candidates:', JSON.stringify(collisionWarnings))
assert.equal(Object.keys(collisionWarnings).length, 0, 'Resolve overlap candidates before browser QA')

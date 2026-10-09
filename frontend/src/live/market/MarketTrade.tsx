import { useGLTF } from '@react-three/drei'
import { createPortal, useFrame } from '@react-three/fiber'
import { useEffect, useMemo, useRef } from 'react'
import { AnimationMixer, Bone, Color, Group, Mesh, PropertyBinding, Quaternion, Vector3, type Material } from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { createBrandCoin } from '../../brand/coin'
import { ScreenText } from '../../genie/ScreenText'
import { createSmokeMaterial } from '../../genie/smoke'

export const TRADE_SECONDS = 6.4
export type TradeEntry = 'bottom' | 'side'
export type TradeAction = 'sell' | 'buy'
export type TradePreview = { action: TradeAction; take: number; playing: boolean; speed: number; entry: TradeEntry; seek: number | null }
const HERO = `${import.meta.env.BASE_URL}models/desk-genie.glb?v=20261008-articulated`
const PROPS = `${import.meta.env.BASE_URL}models/experiments/market-sale-props.glb?v=sale-local-bitcoin-2`
const GOLD_SPARK = new Color(2.4, 1.65, .45)
const MINT_SPARK = new Color(.6, 1.8, 1.25)
const smooth = (v: number) => { const x = Math.min(1, Math.max(0, v)); return x * x * (3 - 2 * x) }
const ramp = (t: number, a: number, b: number) => smooth((t - a) / (b - a))
const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
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

function fade(materials: Material[], amount: number) {
  for (const m of materials) { m.opacity = amount; m.depthWrite = amount > .98 }
}

export function MarketTrade({ preview, onTime, onFinished }: { preview: TradePreview; onTime: (t: number) => void; onFinished: () => void }) {
  const heroGLB = useGLTF(HERO), propsGLB = useGLTF(PROPS)
  const root = useRef<Group>(null)
  const sparkle = useRef<Group>(null)
  const hero = useMemo(() => clone(heroGLB.scene), [heroGLB.scene])
  const mixer = useMemo(() => new AnimationMixer(hero), [hero])
  const smoke = useMemo(() => createSmokeMaterial(), [])
  const bones = useMemo(() => {
    const map = new Map<string, Bone>()
    hero.traverse((o) => { if ((o as Bone).isBone) map.set(o.name.replace(/_\d+$/, ''), o as Bone) })
    return map
  }, [hero])
  const idleLayer = useMemo(() => {
    const idle = heroGLB.animations.find((a) => a.name === 'idle')!
    return idle.tracks.flatMap((track) => {
      const binding = PropertyBinding.parseTrackName(track.name)
      const bone = bones.get(binding.nodeName.replace(/_\d+$/, ''))
      const property = binding.propertyName
      const affected = /^(left|right)-(arm|forearm|hand|point|middle|curl)(-mid|-tip)?$/.test(binding.nodeName)
      if (!bone || !((affected && property === 'quaternion') || (/^(left|right)-lid$/.test(binding.nodeName) && property === 'scale'))) return []
      return [{ bone, property, sample: track.InterpolantFactoryMethodLinear() }]
    })
  }, [heroGLB.animations, bones])
  const props = useMemo(() => {
    const scene = propsGLB.scene.clone(true)
    // Discard the decorative Bitcoin coin from the cached prop clone.
    scene.getObjectByName('market-coin')?.removeFromParent()
    const brandCoin = createBrandCoin(.145, .032)
    const materials: Material[] = []
    scene.traverse((o) => {
      const mesh = o as Mesh
      if (!mesh.isMesh) return
      const copies = (Array.isArray(mesh.material) ? mesh.material : [mesh.material]).map((m) => { const c = m.clone(); c.transparent = true; materials.push(c); return c })
      mesh.material = Array.isArray(mesh.material) ? copies : copies[0]
    })
    const get = (name: string) => { const o = scene.getObjectByName(name); if (!o) throw new Error(`Missing sale prop: ${name}`); return o }
    return { stand: get('market-stand'), crate: get('market-crate'), coin: brandCoin.root, materials: [...materials, ...brandCoin.materials], disposeProps: () => { materials.forEach((m) => m.dispose()); brandCoin.dispose() } }
  }, [propsGLB.scene])
  const anchor = useMemo(() => hero.getObjectByName('text-anchor'), [hero])
  const owned = useRef(props)
  const smokeRef = useRef(smoke)
  useEffect(() => { owned.current = props; smokeRef.current = smoke }, [props, smoke])
  const time = useRef(0), reported = useRef(-1), finished = useRef(false)
  const palmUp = useMemo(() => new Quaternion().setFromAxisAngle(new Vector3(1, 0, 0), -Math.PI / 2), [])

  useEffect(() => {
    const idle = heroGLB.animations.find((a) => a.name === 'idle')
    if (!idle) throw new Error('Missing idle clip')
    mixer.clipAction(idle).reset().play()
    hero.traverse((o) => {
      const mesh = o as Mesh
      if (mesh.isMesh && !Array.isArray(mesh.material) && mesh.material.name === 'tail-smoke') { mesh.material = smoke.material; mesh.renderOrder = 1 }
    })
    return () => { mixer.stopAllAction(); mixer.uncacheRoot(hero) }
  }, [heroGLB.animations, hero, mixer, smoke])
  useEffect(() => () => { smoke.material.dispose(); props.disposeProps() }, [smoke, props])
  useEffect(() => { time.current = 0; reported.current = -1; finished.current = false }, [preview.take, preview.action])
  useEffect(() => { if (preview.seek !== null) { time.current = preview.seek; finished.current = false } }, [preview.seek])

  useFrame((state, delta) => {
    if (!root.current) return
    const props = owned.current
    // The action follows elapsed time, including a slow frame or a suspended tab.
    const elapsed = Number.isFinite(delta) ? Math.max(0, delta) : 0
    const speed = Number.isFinite(preview.speed) ? Math.max(0, preview.speed) : 1
    if (preview.playing && !finished.current) time.current = Math.min(TRADE_SECONDS, time.current + elapsed * speed)
    const actual = time.current
    const t = reduced && preview.playing ? (actual < 5.4 ? 4.25 : 6.4) : actual
    const opacity = ramp(t, 0, .6) * (1 - ramp(t, 5.5, 6.4))
    // The cached hero's clips and geometry are untouched. This clone starts from a stable idle pose each frame.
    mixer.setTime(reduced ? 0 : state.clock.elapsedTime % 4)
    // Mixer bindings can skip unchanged values; restore joints we overwrite explicitly.
    // Otherwise constant idle tracks may leave an arm raised or a shutter closed.
    for (const { bone, property, sample } of idleLayer) {
      const values = sample.evaluate(reduced ? 0 : state.clock.elapsedTime % 4)
      if (property === 'quaternion') bone.quaternion.fromArray(values)
      else bone.scale.fromArray(values)
    }
    // Preserve the existing idle bob and sly face; trades only move arms and props.
    // Assign the height absolutely: repeated additive offsets would make the hero drift upward.
    bones.get('body')!.position.y = reduced ? 0 : .07 * Math.sin(state.clock.elapsedTime * Math.PI)
    const weight = ramp(t, .55, 1.1) * (1 - ramp(t, 4.9, 5.6))
    const buying = preview.action === 'buy'
    reach(bones, 'left', path(t, buying ? [[0,.82,-.8,.42],[.9,.88,-.53,.36],[1.35,1.05,-.41,.34],[1.85,1.22,-.43,.32],[2.3,.66,-.65,.44],[2.85,.34,-.82,.42],[3.65,.28,-.80,.43],[4.25,.28,-.76,.43],[4.7,.28,-.80,.43],[5.5,.82,-.8,.42]] : [[0,.82,-.8,.42],[1.45,.82,-.8,.42],[1.85,.82,-.50,.34],[2.15,.83,-.36,.32],[2.8,1.23,-.32,.29],[3.2,1.16,-.48,.32],[3.65,1.02,-.48,.34],[4.15,.94,-.27,.33],[4.65,.94,-.27,.33],[5.5,.82,-.8,.42]]), palmUp, weight)
    reach(bones, 'right', buying ? path(t, [[0,-.79,-.72,.4],[2,-.79,-.72,.4],[2.8,-.36,-.82,.42],[3.65,-.28,-.80,.43],[4.25,-.28,-.76,.43],[4.7,-.28,-.80,.43],[5.5,-.79,-.72,.4]]) : new Vector3(-.79, -.72, .4), palmUp, weight)
    // Smile and wink once after payment, then return to the normal sly expression.
    bones.get('mouth')!.scale.y = .6 + .22 * ramp(t, 3.65, 4.15) * (1 - ramp(t, 4.65, 5.3))
    const wink = buying ? 0 : ramp(t, 3.85, 4.0) * (1 - ramp(t, 4.18, 4.4))
    // Sample the idle shutter each frame first, so the wink never accumulates.
    const lid = bones.get('right-lid')!
    if (buying && t >= 3.65 && t < 4.8) {
      bones.get('left-lid')!.scale.y = .3
      lid.scale.y = .3
    }
    if (wink > 0) {
      // Keep the other eye open even if the background idle blink overlaps.
      bones.get('left-lid')!.scale.y = .3
      lid.scale.y = .3 + .7 * wink
    }
    root.current.updateMatrixWorld(true)
    const hand = bones.get('left-hand')!
    // Palm/button thickness is ~.14 above the wrist plane. The crate rests above it,
    // with straight supporting fingers, rather than intersecting the hand volume.
    const palmPoint = root.current.worldToLocal(hand.localToWorld(new Vector3(0, -.23, .165)))
    const offset = preview.entry === 'bottom' ? new Vector3(0, -1.8 * (1 - opacity), 0) : new Vector3(3.1 * (1 - opacity), 0, 0)
    props.stand.position.copy((buying ? new Vector3(1.0, -.61, .22) : new Vector3(0, -.8, .62)).add(offset))
    props.stand.rotation.set(0, buying ? -Math.PI / 2 : 0, 0)
    props.stand.scale.setScalar(buying ? .95 : 1)
    props.stand.visible = opacity > .001
    if (buying) {
      const rightHand = bones.get('right-hand')!
      const rightPalm = root.current.worldToLocal(rightHand.localToWorld(new Vector3(0, -.23, .165)))
      const receive = ramp(t, 2.7, 3.65)
      // Both palms sit under the crate; the bottom stays above the palm buttons/fingers.
      const supported = palmPoint.clone().add(rightPalm).multiplyScalar(.5)
      supported.y = Math.max(palmPoint.y, rightPalm.y)
      const settling = t > 3.65 ? .026 * Math.sin((t - 3.65) * 13) * Math.exp(-(t - 3.65) * 4) : 0
      // The buyer stays beside the stall: goods wait on the right-hand counter
      // until payment has left, then travel into his palms. Seller stays off-screen.
      props.crate.visible = opacity > .001
      props.crate.position.copy(new Vector3(1.0, -.555, .44).lerp(supported, receive).add(offset))
      props.crate.position.y += .13 * Math.sin(receive * Math.PI) + settling
      props.crate.rotation.set(0, -.18 * (1 - receive), .045 * Math.sin(receive * Math.PI))
      const pay = ramp(t, 1.3, 2.35)
      props.coin.visible = opacity > .001 && t >= .65 && t < 2.4
      props.coin.position.copy(palmPoint.clone().add(new Vector3(0, .14, .1)).lerp(new Vector3(1.9, -.58, .3), pay).add(offset))
      props.coin.position.y += .16 * Math.sin(pay * Math.PI)
      props.coin.rotation.set(0, .16 + Math.sin(pay * Math.PI) * Math.PI * 2, .05)
      props.coin.scale.setScalar(Math.max(.001, ramp(t, .65, .9)))
    } else {
      props.crate.visible = opacity > .001 && t < 3.25
      const stock = new Vector3(.82, -.745, .59)
      const held = stock.clone().lerp(palmPoint, ramp(t, 1.45, 1.75))
      if (t > 2.8) held.lerp(new Vector3(2.9, -.24, .20), ramp(t, 2.8, 3.25))
      props.crate.position.copy(held.add(offset)); props.crate.rotation.set(0, -.12 * ramp(t, 1.5, 2.1), 0)
      const receive = ramp(t, 3.0, 3.65)
      props.coin.visible = opacity > .001 && t >= 3 && t < 5.45
      props.coin.position.copy(new Vector3(2.3, -.3, .65).lerp(palmPoint.clone().add(new Vector3(0, .14, .10)), receive).add(offset))
      props.coin.rotation.set(0, .16 + Math.sin(receive * Math.PI) * Math.PI * 2, .05)
      const shrink = 1 - ramp(t, 4.9, 5.4); props.coin.scale.setScalar(Math.max(.001, shrink))
    }
    // A brief bloom of sparks sells the catch; no effect in reduced-motion mode.
    if (sparkle.current) {
      const burst = (t - 3.65) / .65
      sparkle.current.visible = buying && !reduced && burst > 0 && burst < 1
      sparkle.current.position.copy(props.crate.position).add(new Vector3(0, .32, .09))
      for (let i = 0; i < sparkle.current.children.length; i++) {
        const particle = sparkle.current.children[i], angle = i * Math.PI / 4
        particle.position.set(Math.cos(angle) * (.22 + .25 * burst), Math.sin(angle) * (.2 + .18 * burst), .24)
        particle.scale.setScalar(Math.max(.001, Math.sin(Math.max(0, Math.min(1, burst)) * Math.PI)))
      }
    }
    fade(props.materials, opacity)
    if (!reduced) smokeRef.current.uniforms.uTime.value = state.clock.elapsedTime
    if (Math.abs(actual - reported.current) > .09) { reported.current = actual; onTime(actual) }
    if (preview.playing && actual >= TRADE_SECONDS && !finished.current) { finished.current = true; onTime(TRADE_SECONDS); onFinished() }
  })

  return <group ref={root} position={[0, .15, 0]} rotation-y={.35}>
    <primitive object={hero} />
    <primitive object={props.stand} />
    <primitive object={props.crate} />
    <primitive object={props.coin} />
    <group ref={sparkle} visible={false}>
      {Array.from({ length: 8 }, (_, i) => <mesh key={i}>
        <octahedronGeometry args={[.018, 0]} />
        <meshBasicMaterial color={i % 2 ? GOLD_SPARK : MINT_SPARK} toneMapped={false} />
      </mesh>)}
    </group>
    {anchor && createPortal(<ScreenText />, anchor)}
  </group>
}

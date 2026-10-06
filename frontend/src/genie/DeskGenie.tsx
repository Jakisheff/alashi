import { RoundedBox } from '@react-three/drei'
import { useFrame } from '@react-three/fiber'
import { useEffect, useMemo, useRef } from 'react'
import {
  CatmullRomCurve3,
  DoubleSide,
  MathUtils,
  TubeGeometry,
  Vector3,
  type Group,
  type Mesh,
} from 'three'
import { useScene } from '../store'
import { CLIP_SECONDS, poseAt, type GenieClip, type Pose } from './pose'

// Palette from character/desk-genie-concept-v1.png
const IVORY = '#ebe1cc'
const TEAL = '#1f4a43'
const ORANGE = '#e06a2c'
const JOINT = '#30332f'
const SCREEN = '#1c1e1a'
const GLOW = '#ffe6a0'
const TAIL = '#2fa392'

const BASE_Y = 0.15
const EYE_HALF = 0.138 // eye radius 0.12 * 1.15
const LID_H = 0.18
const BROW_ARC = 1.3 // radians of a torus arc, centred on top
const BROW_R = 0.14
const TAIL_SEGMENTS = 48
const TAIL_RADIAL = 12
const TAIL_SHAPE = [
  [0, 0.1, 0],
  [0, -0.25, 0.02],
  [-0.08, -0.6, 0.05],
  [0.05, -0.92, 0.05],
  [0.35, -1.05, 0.02],
  [0.55, -0.9, 0],
  [0.5, -0.7, 0],
] as const
const TAIL_LIGHTS = [0.35, 0.55, 0.72, 0.88]

// ?pose=<clip>&t=<seconds> freezes a pose: deterministic frames for review and screenshots.
const frozen = (() => {
  const q = new URLSearchParams(typeof location === 'undefined' ? '' : location.search)
  const clip = q.get('pose')
  return clip && clip in CLIP_SECONDS ? { clip: clip as GenieClip, t: Number(q.get('t') ?? 0.5) } : null
})()

const reducedMotion = () =>
  typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches

function Arm({ side, armRef, elbowRef }: {
  side: 1 | -1
  armRef: React.RefObject<Group | null>
  elbowRef: React.RefObject<Group | null>
}) {
  return (
    <group ref={armRef} position={[0.58 * side, -0.38, 0.15]}>
      <mesh>
        <sphereGeometry args={[0.1, 20, 16]} />
        <meshStandardMaterial color={JOINT} roughness={0.6} />
      </mesh>
      <mesh position={[0, -0.16, 0]}>
        <capsuleGeometry args={[0.075, 0.2, 6, 16]} />
        <meshStandardMaterial color={IVORY} roughness={0.5} />
      </mesh>
      <group ref={elbowRef} position={[0, -0.32, 0]}>
        <mesh>
          <sphereGeometry args={[0.072, 16, 12]} />
          <meshStandardMaterial color={JOINT} roughness={0.6} />
        </mesh>
        <mesh position={[0, -0.15, 0]}>
          <capsuleGeometry args={[0.07, 0.18, 6, 16]} />
          <meshStandardMaterial color={IVORY} roughness={0.5} />
        </mesh>
        {/* Mitten hand: dark palm, orange pad, ivory fingers and thumb. */}
        <group position={[0, -0.36, 0]}>
          <mesh scale={[1, 1.1, 0.7]}>
            <sphereGeometry args={[0.1, 20, 16]} />
            <meshStandardMaterial color={TEAL} roughness={0.55} />
          </mesh>
          <mesh position={[0, 0, 0.068]}>
            <circleGeometry args={[0.045, 20]} />
            <meshStandardMaterial color={ORANGE} roughness={0.7} />
          </mesh>
          {[-0.05, 0, 0.05].map((x) => (
            <mesh key={x} position={[x, -0.11, 0.01]}>
              <capsuleGeometry args={[0.026, 0.06, 4, 10]} />
              <meshStandardMaterial color={IVORY} roughness={0.5} />
            </mesh>
          ))}
          <mesh position={[0.09 * side, -0.02, 0.03]} rotation={[0, 0, 0.9 * side]}>
            <capsuleGeometry args={[0.026, 0.05, 4, 10]} />
            <meshStandardMaterial color={IVORY} roughness={0.5} />
          </mesh>
        </group>
      </group>
    </group>
  )
}

function Eye({ x, pupilRef, lidRef }: {
  x: number
  pupilRef: React.RefObject<Mesh | null>
  lidRef: React.RefObject<Mesh | null>
}) {
  return (
    <group position={[x, 0.07, 0.516]}>
      <mesh scale={[1, 1.15, 1]}>
        <circleGeometry args={[0.12, 32]} />
        <meshBasicMaterial color={GLOW} toneMapped={false} />
      </mesh>
      <mesh ref={pupilRef} position={[0, 0, 0.002]}>
        <circleGeometry args={[0.055, 24]} />
        <meshBasicMaterial color="#2a2820" />
      </mesh>
      {/* Upper lid: a patch of screen that slides down over the eye. */}
      <mesh ref={lidRef} position={[0, 0.2, 0.004]}>
        <planeGeometry args={[0.3, LID_H]} />
        <meshStandardMaterial color={SCREEN} roughness={0.25} metalness={0.1} emissive="#1f2a22" />
      </mesh>
    </group>
  )
}

export function DeskGenie() {
  const root = useRef<Group>(null)
  const rArm = useRef<Group>(null)
  const rElbow = useRef<Group>(null)
  const lArm = useRef<Group>(null)
  const lElbow = useRef<Group>(null)
  const lLid = useRef<Mesh>(null)
  const rLid = useRef<Mesh>(null)
  const lPupil = useRef<Mesh>(null)
  const rPupil = useRef<Mesh>(null)
  const lBrow = useRef<Mesh>(null)
  const rBrow = useRef<Mesh>(null)
  const mouth = useRef<Mesh>(null)
  const lights = useRef<(Mesh | null)[]>([])
  const tailMesh = useRef<Mesh>(null)

  const motion = useMemo(() => (reducedMotion() ? 0.3 : 1), [])
  const pose = useRef<Pose>(poseAt('idle', 0, 0, motion))
  const clipStart = useRef(0)
  const restart = useRef(false)

  const take = useScene((s) => s.take)
  useEffect(() => {
    restart.current = true
  }, [take])

  const curve = useMemo(
    () => new CatmullRomCurve3(TAIL_SHAPE.map(([x, y, z]) => new Vector3(x, y, z))),
    [],
  )
  const tail = useMemo(
    () => new TubeGeometry(curve, TAIL_SEGMENTS, 0.2, TAIL_RADIAL, false),
    [curve],
  )
  useEffect(() => () => tail.dispose(), [tail])

  useFrame((state, delta) => {
    const time = frozen ? 1 : state.clock.elapsedTime
    if (restart.current) {
      clipStart.current = time
      restart.current = false
    }
    const clip = frozen?.clip ?? useScene.getState().clip
    const t = frozen?.t ?? time - clipStart.current
    // One-shot clips hand back to idle; idle is a seamless loop.
    if (!frozen && clip !== 'idle' && t > CLIP_SECONDS[clip]) useScene.setState({ clip: 'idle' })

    const target = poseAt(clip, t, time, motion)
    const p = pose.current
    for (const k of Object.keys(target) as (keyof Pose)[]) {
      p[k] = frozen ? target[k] : MathUtils.damp(p[k], target[k], k === 'eyeOpen' ? 30 : 12, delta)
    }

    root.current!.position.y = BASE_Y + p.y
    root.current!.rotation.set(p.tiltX, p.yaw, p.tiltZ)
    rArm.current!.rotation.set(-p.rArmFwd, 0, p.rArmOut)
    lArm.current!.rotation.set(-p.lArmFwd, 0, -p.lArmOut)
    rElbow.current!.rotation.x = -p.rElbow
    lElbow.current!.rotation.x = -p.lElbow

    // Blink closes whatever the lid leaves open; lids follow the brow slant.
    const cover = 1 - Math.max(0, Math.min(1, p.eyeOpen)) * (1 - MathUtils.clamp(p.lid, 0, 1))
    const lidY = EYE_HALF - cover * 2 * EYE_HALF + LID_H / 2
    lLid.current!.position.y = lidY
    rLid.current!.position.y = lidY
    // Outer lid corners droop a little: sly, not angry.
    lLid.current!.rotation.z = 0.12 + p.browTilt * 0.45
    rLid.current!.rotation.z = -0.12 - p.browTilt * 0.45
    lPupil.current!.position.set(p.lookX * 0.045, p.lookY * 0.05, 0.002)
    rPupil.current!.position.set(p.lookX * 0.045, p.lookY * 0.05, 0.002)
    // Arched brows; inner ends rise when worried, drop when scheming.
    const browZ = Math.PI / 2 - BROW_ARC / 2
    lBrow.current!.position.y = 0.27 - BROW_R + p.browL * 0.06
    rBrow.current!.position.y = 0.27 - BROW_R + p.browR * 0.06
    lBrow.current!.rotation.z = browZ - 0.05 + p.browTilt * 0.45
    rBrow.current!.rotation.z = browZ + 0.1 - p.browTilt * 0.45
    const s = Math.abs(p.smile) < 0.12 ? (p.smile < 0 ? -0.12 : 0.12) : p.smile
    mouth.current!.scale.set(1, s, 1)

    // Tail: sway the control points, then rebuild the tapered tube in place
    // using the same vertex layout as THREE.TubeGeometry.
    curve.points.forEach((pt, i) => {
      const k = i / (curve.points.length - 1)
      const [x, y, z] = TAIL_SHAPE[i]
      pt.set(
        x + p.tailSway * 0.12 * k * Math.sin(time * 1.8 - i * 0.7) * motion,
        y,
        z + p.tailSway * 0.08 * k * Math.cos(time * 1.4 - i * 0.6) * motion,
      )
    })
    const frames = curve.computeFrenetFrames(TAIL_SEGMENTS, false)
    const tailGeo = tailMesh.current!.geometry
    const pos = tailGeo.attributes.position
    const c = new Vector3()
    let n = 0
    for (let i = 0; i <= TAIL_SEGMENTS; i++) {
      const u = i / TAIL_SEGMENTS
      curve.getPointAt(u, c)
      const r = 0.23 * Math.pow(1 - u, 1.1) + 0.015
      const N = frames.normals[i]
      const B = frames.binormals[i]
      for (let j = 0; j <= TAIL_RADIAL; j++) {
        const v = (j / TAIL_RADIAL) * Math.PI * 2
        const sin = Math.sin(v)
        const cos = -Math.cos(v)
        pos.setXYZ(
          n++,
          c.x + r * (cos * N.x + sin * B.x),
          c.y + r * (cos * N.y + sin * B.y),
          c.z + r * (cos * N.z + sin * B.z),
        )
      }
    }
    pos.needsUpdate = true
    tailGeo.computeVertexNormals()
    TAIL_LIGHTS.forEach((u, i) => {
      const light = lights.current[i]
      if (light) curve.getPointAt(u, light.position)
    })
  })

  return (
    <group ref={root} rotation-order="YXZ">
      {/* Head-body: a rounded CRT housing. */}
      <RoundedBox args={[1.3, 1.15, 1]} radius={0.24} smoothness={6}>
        <meshStandardMaterial color={IVORY} roughness={0.55} />
      </RoundedBox>
      <RoundedBox args={[0.9, 0.66, 0.08]} radius={0.07} smoothness={4} position={[0.03, 0.06, 0.475]}>
        <meshStandardMaterial color={SCREEN} roughness={0.25} metalness={0.1} emissive="#1f2a22" />
      </RoundedBox>

      <Eye x={-0.18} pupilRef={lPupil} lidRef={lLid} />
      <Eye x={0.22} pupilRef={rPupil} lidRef={rLid} />
      <mesh ref={lBrow} position={[-0.18, 0.13, 0.53]}>
        <torusGeometry args={[BROW_R, 0.022, 8, 20, BROW_ARC]} />
        <meshStandardMaterial color="#c2b291" roughness={0.6} />
      </mesh>
      <mesh ref={rBrow} position={[0.22, 0.13, 0.53]}>
        <torusGeometry args={[BROW_R, 0.022, 8, 20, BROW_ARC]} />
        <meshStandardMaterial color="#c2b291" roughness={0.6} />
      </mesh>
      <mesh ref={mouth} position={[0.07, -0.13, 0.518]} rotation={[0, 0, Math.PI + 0.12]}>
        <torusGeometry args={[0.1, 0.018, 8, 24, Math.PI]} />
        <meshBasicMaterial color={GLOW} toneMapped={false} side={DoubleSide} />
      </mesh>

      {/* Forest-teal side panels with orange slots. */}
      {[1, -1].map((side) => (
        <group key={side} position={[0.64 * side, 0.05, 0]}>
          <mesh rotation={[0, 0, Math.PI / 2]}>
            <cylinderGeometry args={[0.34, 0.34, 0.12, 40]} />
            <meshStandardMaterial color={TEAL} roughness={0.5} />
          </mesh>
          <RoundedBox args={[0.04, 0.22, 0.08]} radius={0.015} position={[0.07 * side, 0, 0]}>
            <meshStandardMaterial color={ORANGE} roughness={0.6} />
          </RoundedBox>
        </group>
      ))}

      {/* Orange details, vents and screws. */}
      <RoundedBox args={[0.26, 0.06, 0.14]} radius={0.02} position={[0.12, 0.585, 0]}>
        <meshStandardMaterial color={ORANGE} roughness={0.6} />
      </RoundedBox>
      {[-0.33, -0.39].map((y) => (
        <RoundedBox key={y} args={[0.2, 0.032, 0.03]} radius={0.01} position={[0.1, y, 0.5]}>
          <meshStandardMaterial color={ORANGE} roughness={0.6} />
        </RoundedBox>
      ))}
      {[-0.32, -0.25, -0.18].map((x) => (
        <mesh key={x} position={[x, -0.36, 0.5]}>
          <sphereGeometry args={[0.018, 10, 8]} />
          <meshStandardMaterial color={JOINT} />
        </mesh>
      ))}
      <mesh position={[0.42, 0.46, 0.5]}>
        <sphereGeometry args={[0.025, 12, 10]} />
        <meshStandardMaterial color="#8d8a80" metalness={0.6} roughness={0.35} />
      </mesh>

      {/* Lower housing narrows into the tail. */}
      <mesh position={[0, -0.6, 0]} scale={[1, 0.55, 0.8]}>
        <sphereGeometry args={[0.5, 40, 24, 0, Math.PI * 2, Math.PI / 2, Math.PI / 2]} />
        <meshStandardMaterial color={IVORY} roughness={0.55} />
      </mesh>

      <Arm side={1} armRef={rArm} elbowRef={rElbow} />
      <Arm side={-1} armRef={lArm} elbowRef={lElbow} />

      <group position={[0, -0.72, 0]}>
        <mesh ref={tailMesh} geometry={tail} renderOrder={1}>
          <meshPhysicalMaterial
            color={TAIL}
            emissive="#14786b"
            emissiveIntensity={0.6}
            roughness={0.1}
            clearcoat={1}
            transparent
            opacity={0.5}
            depthWrite={false}
          />
        </mesh>
        {TAIL_LIGHTS.map((u, i) => (
          <mesh key={u} ref={(m) => { lights.current[i] = m }} renderOrder={2}>
            <sphereGeometry args={[0.035, 10, 8]} />
            <meshBasicMaterial color="#fff6c8" toneMapped={false} />
          </mesh>
        ))}
      </group>
    </group>
  )
}

import { useGLTF } from '@react-three/drei'
import { createPortal, useFrame } from '@react-three/fiber'
import { useEffect, useMemo, useRef, useState } from 'react'
import type { Mesh } from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { ScreenText } from '../../genie/ScreenText'
import { createSmokeMaterial } from '../../genie/smoke'
import { REACTION_REDUCED_FRAME, REACTION_SECONDS, type ReactionPreview } from './definitions.ts'
import { createLivingRig } from './rig.ts'
import { createVictoryFireworks } from './fireworks.ts'

export { IDLE_KEYFRAMES, LIVING_IDLE_SECONDS, REACTION_KEYFRAMES, REACTION_REDUCED_FRAME, REACTION_SECONDS } from './definitions.ts'
export type { ReactionKind, ReactionPreview } from './definitions.ts'
export type LivingGenieProps = {
  reaction?: ReactionPreview | null
  onTime?: (seconds: number) => void
  onFinished?: () => void
}

const HERO = `${import.meta.env.BASE_URL}models/desk-genie.glb?v=20261008-articulated`
const clampTime = (value: number) => Number.isFinite(value) ? Math.max(0, Math.min(REACTION_SECONDS, value)) : 0
function useReducedMotion() {
  const [reduced, setReduced] = useState(() => typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches)
  useEffect(() => {
    const media = window.matchMedia('(prefers-reduced-motion: reduce)')
    const update = () => setReduced(media.matches)
    media.addEventListener('change', update)
    update()
    return () => media.removeEventListener('change', update)
  }, [])
  return reduced
}

/** Canvas child, sharing the normal hero placement. The parent chooses reactions;
 * silence and ordinary idle never infer an error, facepalm, or thumbs-up.
 * A new take (or kind) resets the timeline; seek is edge-triggered, like MarketTrade.
 */
export function LivingGenie({ reaction = null, onTime, onFinished }: LivingGenieProps) {
  const glb = useGLTF(HERO)
  const hero = useMemo(() => clone(glb.scene), [glb.scene])
  const rig = useMemo(() => createLivingRig(hero, glb.animations), [hero, glb.animations])
  const smoke = useMemo(() => createSmokeMaterial(), [])
  const fireworks = useMemo(() => createVictoryFireworks(), [])
  useEffect(() => () => fireworks.dispose(), [fireworks])
  const smokeRef = useRef(smoke)
  useEffect(() => { smokeRef.current = smoke }, [smoke])
  const anchor = useMemo(() => hero.getObjectByName('text-anchor'), [hero])
  const reduced = useReducedMotion()
  const playback = useRef({
    key: '', seek: null as number | null, time: 0, idleTime: 0, base: 0,
    reported: -1, finished: false,
  })

  useEffect(() => {
    // SkeletonUtils owns transforms/skeleton; only replace the smoke material on
    // cloned meshes. All cached hero materials and geometry remain untouched.
    hero.traverse((o) => {
      const mesh = o as Mesh
      if (!mesh.isMesh) return
      // Preserve any multi-material slots, including non-smoke materials.
      if (Array.isArray(mesh.material)) {
        mesh.material = mesh.material.map((m) => m.name === 'tail-smoke' ? smoke.material : m)
      } else if (mesh.material.name === 'tail-smoke') mesh.material = smoke.material
      if (Array.isArray(mesh.material) ? mesh.material.includes(smoke.material) : mesh.material === smoke.material) mesh.renderOrder = 1
    })
    return () => smoke.material.dispose()
  }, [hero, smoke])

  useFrame((_state, delta) => {
    const p = playback.current
    const elapsed = Number.isFinite(delta) ? Math.max(0, delta) : 0
    const step = Math.min(elapsed, .05)
    if (!reaction) {
      fireworks.update(0, false)
      p.key = ''; p.seek = null; p.finished = false
      if (!reduced) p.idleTime += step
      rig.idleAt(p.idleTime, reduced)
      smokeRef.current.uniforms.uTime.value = reduced ? 0 : p.idleTime
      return
    }
    const key = `${reaction.kind}:${reaction.take}`
    const seek = reaction.seek === null ? null : clampTime(reaction.seek)
    const restarted = p.key !== key
    const sought = restarted || p.seek !== seek
    if (restarted) {
      p.key = key; p.base = p.idleTime; p.time = 0; p.reported = -1; p.finished = false
    }
    if (sought) {
      p.seek = seek
      if (seek !== null) { p.time = seek; p.finished = false; p.reported = -1 }
    }
    // First frame after a seek is exact, even while playing. Pausing freezes the
    // *whole* pose (blink, tail and smoke too), so frame QA is repeatable.
    const speed = Number.isFinite(reaction.speed) ? Math.max(0, reaction.speed) : 1
    if (reaction.playing && !p.finished && !sought) p.time = clampTime(p.time + elapsed * speed)
    p.idleTime = p.base + p.time
    const poseTime = reduced && reaction.playing && p.time < REACTION_SECONDS
      ? REACTION_REDUCED_FRAME[reaction.kind] : p.time
    rig.reactionAt(reaction.kind, poseTime, p.idleTime, reduced)
    fireworks.update(p.time, reaction.kind === 'victory' && !reduced)
    smokeRef.current.uniforms.uTime.value = reduced ? 0 : p.idleTime
    if (p.reported < 0 || Math.abs(p.time - p.reported) >= .09) { p.reported = p.time; onTime?.(p.time) }
    if (reaction.playing && p.time >= REACTION_SECONDS && !p.finished) {
      p.finished = true
      if (p.reported !== REACTION_SECONDS) { p.reported = REACTION_SECONDS; onTime?.(REACTION_SECONDS) }
      onFinished?.()
    }
  })

  return <><group position={[0, .15, 0]} rotation-y={.35} dispose={null}>
    <primitive object={hero} />
    {anchor && createPortal(<ScreenText />, anchor)}
  </group><primitive object={fireworks.group} /></>
}

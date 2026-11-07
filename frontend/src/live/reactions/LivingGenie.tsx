import { useGLTF } from '@react-three/drei'
import { createPortal, useFrame } from '@react-three/fiber'
import { useEffect, useMemo, useRef, useState } from 'react'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { ScreenText } from '../../genie/ScreenText'
import { createSmokeMaterial } from '../../genie/smoke'
import { REACTION_SECONDS, type ReactionPreview } from './definitions.ts'
import { createLivingRig } from './rig.ts'
import { createVictoryFireworks } from './fireworks.ts'
import { assignSmokeMaterial } from './smokeMaterial.ts'
import { advanceReactionFrame, completeReaction, prepareReactionFrame, shouldReportReactionTime, type ReactionClock } from './reactionClock.ts'

export { IDLE_KEYFRAMES, LIVING_IDLE_SECONDS, REACTION_KEYFRAMES, REACTION_REDUCED_FRAME, REACTION_SECONDS } from './definitions.ts'
export type { ReactionKind, ReactionPreview } from './definitions.ts'
export type LivingGenieProps = {
  reaction?: ReactionPreview | null
  idlePlaying?: boolean
  onTime?: (seconds: number) => void
  onFinished?: () => void
  syncSpeech?: boolean
}

const HERO = `${import.meta.env.BASE_URL}models/desk-genie.glb?v=20261008-articulated`
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
export function LivingGenie({ reaction = null, idlePlaying = true, onTime, onFinished, syncSpeech = true }: LivingGenieProps) {
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
  const playback = useRef<ReactionClock>({
    key: '', seek: null, time: 0, idleTime: 0, base: 0,
    reported: -1, finished: false, shown: null,
  })

  useEffect(() => {
    assignSmokeMaterial(hero, smoke.material)
    return () => smoke.material.dispose()
  }, [hero, smoke])

  useFrame((_state, delta) => {
    const p = playback.current
    const elapsed = Number.isFinite(delta) ? Math.max(0, delta) : 0
    const step = Math.min(elapsed, .05)
    if (!reaction) {
      fireworks.update(0, false)
      p.key = ''; p.seek = null; p.finished = false
      if (idlePlaying && !reduced) p.idleTime += step
      rig.idleAt(p.idleTime, reduced)
      smokeRef.current.uniforms.uTime.value = reduced ? 0 : p.idleTime
      return
    }
    const sought = prepareReactionFrame(p, reaction)
    // First frame after a seek is exact; pausing holds the whole displayed pose.
    const poseTime = advanceReactionFrame(p, reaction, elapsed, reduced, sought)
    rig.reactionAt(reaction.kind, poseTime, p.idleTime, reduced)
    fireworks.update(p.time, reaction.kind === 'victory' && !reduced)
    smokeRef.current.uniforms.uTime.value = reduced ? 0 : p.idleTime
    if (shouldReportReactionTime(p)) onTime?.(p.time)
    if (reaction.playing && p.time >= REACTION_SECONDS && !p.finished) {
      if (completeReaction(p)) onTime?.(REACTION_SECONDS)
      onFinished?.()
    }
  }, -2)

  return <><group position={[0, .15, 0]} rotation-y={.35} dispose={null}>
    <primitive object={hero} />
    {anchor && createPortal(<ScreenText clock={syncSpeech && reaction ? () => playback.current.time : undefined} reducedMotion={reduced} />, anchor)}
  </group><primitive object={fireworks.group} /></>
}

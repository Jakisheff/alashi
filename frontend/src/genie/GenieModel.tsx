import { useAnimations, useGLTF } from '@react-three/drei'
import { createPortal, useFrame, type ThreeEvent } from '@react-three/fiber'
import { useEffect, useMemo, useRef, useState } from 'react'
import { LoopOnce, LoopRepeat, type AnimationAction, type Group, type Mesh } from 'three'
import { useScene } from '../store'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { CLIP_REDUCED_FRAME, CLIP_SECONDS, type GenieClip } from './pose'
import { ScreenText } from './ScreenText'
import { createSmokeMaterial } from './smoke'

// Built by art/desk_genie.py in Blender; clips are sampled from pose.ts.
// Refresh the model cache when shipping the new rig and animation set.
const MODEL = `${import.meta.env.BASE_URL}models/desk-genie.glb?v=20261008-articulated`
const FADE = 0.2


/** frozen (?pose=<clip>&t=<seconds>) holds one pose: deterministic frames for review and screenshots. */
export function GenieModel({ frozen = null, interactive = false }: { frozen?: { clip: GenieClip; t: number } | null; interactive?: boolean }) {
  const group = useRef<Group>(null)
  const glb = useGLTF(MODEL)
  const scene = useMemo(() => clone(glb.scene), [glb.scene])
  const animations = glb.animations
  const { actions, mixer } = useAnimations(animations, group)
  const actionsRef = useRef(actions)
  useEffect(() => { actionsRef.current = actions }, [actions])
  const clip = useScene((s) => s.clip)
  const take = useScene((s) => s.take)
  const taps = useRef<number[]>([])
  const current = useRef<AnimationAction | null>(null)
  const reducedClock = useRef(0)
  const firstFrame = useRef(true)
  const [reducedMotion, setReducedMotion] = useState(() => window.matchMedia('(prefers-reduced-motion: reduce)').matches)
  useEffect(() => {
    const media = window.matchMedia('(prefers-reduced-motion: reduce)')
    const update = () => setReducedMotion(media.matches)
    media.addEventListener('change', update)
    return () => media.removeEventListener('change', update)
  }, [])
  const smoke = useMemo(() => createSmokeMaterial(), [])
  const smokeRef = useRef(smoke)
  const textAnchor = useMemo(() => scene.getObjectByName('text-anchor'), [scene])

  // The GLB tail carries a placeholder glass material; swap in the animated smoke shader.
  useEffect(() => {
    scene.traverse((o) => {
      const mesh = o as Mesh
      if (mesh.isMesh && !Array.isArray(mesh.material) && mesh.material.name === 'tail-smoke') {
        mesh.material = smoke.material
        mesh.renderOrder = 1
      }
    })
    return () => smoke.material.dispose()
  }, [scene, smoke])
  useFrame((_state, delta) => {
    const action = current.current
    const started = firstFrame.current
    firstFrame.current = false
    smokeRef.current.uniforms.uTime.value = reducedMotion ? 0 : frozen ? Math.max(0, Math.min(CLIP_SECONDS[frozen.clip], frozen.t)) : action?.time ?? 0
    if (reducedMotion && !frozen && clip !== 'idle' && action && !started) {
      reducedClock.current += Number.isFinite(delta) ? Math.max(0, delta) : 0
      if (reducedClock.current >= CLIP_SECONDS[clip]) useScene.getState().play('idle')
    }
  })

  useEffect(() => {
    const name = frozen?.clip ?? clip
    const next = actionsRef.current[name]
    if (!next) return
    const prev = current.current
    reducedClock.current = 0
    firstFrame.current = true
    if (frozen || reducedMotion) mixer.stopAllAction()
    next.reset()
    next.setLoop(name === 'idle' ? LoopRepeat : LoopOnce, Infinity)
    next.clampWhenFinished = true
    if (frozen) {
      next.play()
      next.paused = true
      next.time = Math.max(0, Math.min(CLIP_SECONDS[name], frozen.t))
      mixer.update(0)
    } else if (reducedMotion) {
      next.play()
      next.paused = true
      next.time = CLIP_REDUCED_FRAME[name]
      mixer.update(0)
    } else if (prev && prev !== next) {
      next.fadeIn(FADE).play()
      prev.fadeOut(FADE)
    } else {
      next.play() // first clip, or the same clip retriggered: restart without fading through the bind pose
    }
    current.current = next
  }, [clip, take, actions, frozen, reducedMotion, mixer])

  // One-shot clips hand back to idle when they finish.
  useEffect(() => {
    const onFinished = (e: { action: AnimationAction }) => {
      if (!frozen && e.action === current.current && e.action !== actions.idle) useScene.getState().play('idle')
    }
    mixer.addEventListener('finished', onFinished)
    return () => mixer.removeEventListener('finished', onFinished)
  }, [mixer, actions, frozen])

  const onClick = (event: ThreeEvent<MouseEvent>) => {
    if (!interactive || frozen || event.button !== 0 || event.delta > 5) return
    event.stopPropagation()
    const now = performance.now()
    taps.current = [...taps.current.filter((t) => now - t < 900), now]
    if (taps.current.length < 3) return
    taps.current = []
    if (useScene.getState().clip === 'fuckOff') return
    useScene.getState().play('fuckOff')
    useScene.getState().say('Иди пососи')
  }

  return (
    <group ref={group} position={[0, 0.15, 0]} rotation-y={0.35} onClick={onClick} dispose={null}>
      <primitive object={scene} />
      {textAnchor && createPortal(<ScreenText />, textAnchor)}
    </group>
  )
}

useGLTF.preload(MODEL)

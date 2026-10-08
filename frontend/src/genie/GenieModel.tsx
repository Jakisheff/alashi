import { useAnimations, useGLTF } from '@react-three/drei'
import { createPortal, useFrame, type ThreeEvent } from '@react-three/fiber'
import { useEffect, useMemo, useRef } from 'react'
import { LoopOnce, LoopRepeat, type AnimationAction, type Group, type Mesh } from 'three'
import { useScene } from '../store'
import type { GenieClip } from './pose'
import { ScreenText } from './ScreenText'
import { createSmokeMaterial } from './smoke'

// Built by art/desk_genie.py in Blender; clips are sampled from pose.ts.
// Refresh the model cache when shipping the new rig and animation set.
const MODEL = `${import.meta.env.BASE_URL}models/desk-genie.glb?v=20261008-articulated`
const FADE = 0.2


const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

/** frozen (?pose=<clip>&t=<seconds>) holds one pose: deterministic frames for review and screenshots. */
export function GenieModel({ frozen = null, interactive = false }: { frozen?: { clip: GenieClip; t: number } | null; interactive?: boolean }) {
  const group = useRef<Group>(null)
  const { scene, animations } = useGLTF(MODEL)
  const { actions, mixer } = useAnimations(animations, group)
  const clip = useScene((s) => s.clip)
  const take = useScene((s) => s.take)
  const taps = useRef<number[]>([])
  const current = useRef<AnimationAction | null>(null)
  const smoke = useMemo(() => createSmokeMaterial(), [])
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
    // The GLB scene is cached across visits; the next mount finds this material by the same name and swaps it again
    return () => smoke.material.dispose()
  }, [scene, smoke])
  useFrame((state) => {
    if (!reducedMotion) smoke.uniforms.uTime.value = state.clock.elapsedTime
  })

  useEffect(() => {
    const name = frozen?.clip ?? clip
    const next = actions[name]
    if (!next) return
    const prev = current.current
    next.reset()
    next.setLoop(name === 'idle' ? LoopRepeat : LoopOnce, Infinity)
    next.clampWhenFinished = true
    if (frozen) {
      next.play()
      next.paused = true
      next.time = frozen.t
    } else if (prev && prev !== next) {
      next.fadeIn(FADE).play()
      prev.fadeOut(FADE)
    } else {
      next.play() // first clip, or the same clip retriggered: restart without fading through the bind pose
    }
    // Reduced motion: no hovering loop, reactions still play.
    if (reducedMotion && name === 'idle') next.paused = true
    current.current = next
  }, [clip, take, actions, frozen])

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
    <group ref={group} position={[0, 0.15, 0]} rotation-y={0.35} onClick={onClick}>
      <primitive object={scene} />
      {textAnchor && createPortal(<ScreenText />, textAnchor)}
    </group>
  )
}

useGLTF.preload(MODEL)

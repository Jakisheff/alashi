import { useAnimations, useGLTF } from '@react-three/drei'
import { createPortal, useFrame } from '@react-three/fiber'
import { useEffect, useMemo, useRef } from 'react'
import { LoopOnce, LoopRepeat, type AnimationAction, type Group, type Mesh } from 'three'
import { useScene } from '../store'
import { CLIP_SECONDS, type GenieClip } from './pose'
import { ScreenText } from './ScreenText'
import { createSmokeMaterial } from './smoke'

// Built by art/desk_genie.py in Blender; clips are sampled from pose.ts.
const MODEL = '/models/desk-genie.glb'
const FADE = 0.2

// ?pose=<clip>&t=<seconds> freezes a pose: deterministic frames for review and screenshots.
const frozen = (() => {
  const q = new URLSearchParams(location.search)
  const clip = q.get('pose')
  return clip && clip in CLIP_SECONDS ? { clip: clip as GenieClip, t: Number(q.get('t') ?? 0.5) } : null
})()

const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

export function GenieModel() {
  const group = useRef<Group>(null)
  const { scene, animations } = useGLTF(MODEL)
  const { actions, mixer } = useAnimations(animations, group)
  const clip = useScene((s) => s.clip)
  const take = useScene((s) => s.take)
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
  }, [clip, take, actions])

  // One-shot clips hand back to idle when they finish.
  useEffect(() => {
    const onFinished = (e: { action: AnimationAction }) => {
      if (!frozen && e.action !== actions.idle) useScene.getState().play('idle')
    }
    mixer.addEventListener('finished', onFinished)
    return () => mixer.removeEventListener('finished', onFinished)
  }, [mixer, actions])

  return (
    <group ref={group} position={[0, 0.15, 0]} rotation-y={0.35}>
      <primitive object={scene} />
      {textAnchor && createPortal(<ScreenText />, textAnchor)}
    </group>
  )
}

useGLTF.preload(MODEL)

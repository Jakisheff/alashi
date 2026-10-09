import { useGLTF } from '@react-three/drei'
import { createPortal, useFrame } from '@react-three/fiber'
import { Suspense, useEffect, useMemo, useRef, useState } from 'react'
import { Mesh } from 'three'
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js'
import { ScreenText } from '../../genie/ScreenText'
import { createSmokeMaterial } from '../../genie/smoke'
import { ACTION_SECONDS, clampTime, scenarioFraming, type ScenarioPreview } from './definitions.ts'
import { applyScenario, displayTime } from './motion.ts'
import { createScenarioProps } from './props.ts'
import { MuleReference } from './MuleReference'
import { createRig } from './rig.ts'

const HERO = `${import.meta.env.BASE_URL}models/desk-genie.glb?v=20261008-articulated`

function useReducedMotion() {
  const [reduced, setReduced] = useState(() => typeof window !== 'undefined' && window.matchMedia('(prefers-reduced-motion: reduce)').matches)
  useEffect(() => {
    const query = window.matchMedia('(prefers-reduced-motion: reduce)')
    const change = () => setReduced(query.matches)
    query.addEventListener('change', change)
    return () => query.removeEventListener('change', change)
  }, [])
  return reduced
}

/** Pure visual preview. Parent owns action confirmation, English copy and controls. */
export function ScenarioAction({ preview, onTime, onFinished }: {
  preview: ScenarioPreview
  onTime: (time: number) => void
  onFinished: () => void
}) {
  const glb = useGLTF(HERO)
  const hero = useMemo(() => clone(glb.scene), [glb.scene])
  const rig = useMemo(() => createRig(hero, glb.animations), [hero, glb.animations])
  const props = useMemo(() => createScenarioProps(), [])
  const smoke = useMemo(() => createSmokeMaterial(), [])
  const smokeRef = useRef(smoke)
  const anchor = useMemo(() => hero.getObjectByName('text-anchor'), [hero])
  const reduced = useReducedMotion()
  const time = useRef(0), lastReport = useRef(-1), finished = useRef(false)

  useEffect(() => {
    smokeRef.current = smoke
    hero.traverse((o) => {
      const mesh = o as Mesh
      if (mesh.isMesh && !Array.isArray(mesh.material) && mesh.material.name === 'tail-smoke') {
        mesh.material = smoke.material; mesh.renderOrder = 1
      }
    })
    return () => { smoke.material.dispose(); props.dispose() }
  }, [hero, smoke, props])
  useEffect(() => {
    time.current = 0; lastReport.current = -1; finished.current = false
  }, [preview.take, preview.action])
  useEffect(() => {
    if (preview.seek !== null) {
      time.current = clampTime(preview.seek); lastReport.current = -1; finished.current = false
    }
  }, [preview.seek, preview.take, preview.action])

  useFrame((_, delta) => {
    const speed = Number.isFinite(preview.speed) ? Math.max(0, preview.speed) : 1
    const elapsed = Number.isFinite(delta) ? Math.max(0, delta) : 0
    if (preview.playing && !finished.current) time.current = Math.min(ACTION_SECONDS, time.current + elapsed * speed)
    const actual = time.current
    const shown = displayTime(preview.action, actual, reduced, preview.playing)
    applyScenario(rig, props, preview, shown, reduced)
    smokeRef.current.uniforms.uTime.value = reduced ? 0 : shown
    if (Math.abs(actual - lastReport.current) >= .09) { lastReport.current = actual; onTime(actual) }
    if (preview.playing && actual >= ACTION_SECONDS && !finished.current) {
      finished.current = true; onTime(ACTION_SECONDS); onFinished()
    }
  })

  const framing = scenarioFraming(preview.action)
  // Keep side characters and the ballot box inside the portrait camera.
  return <group position={[framing.x, .15, 0]} scale={framing.scale} rotation-y={.35} dispose={null}>
    <primitive object={hero} />
    <primitive object={props.root} />
    {preview.action === 'mule' && <Suspense fallback={null}><MuleReference props={props} /></Suspense>}
    {anchor && createPortal(<ScreenText />, anchor)}
  </group>
}

import { OrbitControls, PerformanceMonitor } from '@react-three/drei'
import { Canvas, useThree } from '@react-three/fiber'
import { Bloom, EffectComposer } from '@react-three/postprocessing'
import { memo, Suspense, useEffect, useState } from 'react'
import { Color, DoubleSide, Mesh, MeshBasicMaterial, PlaneGeometry, PMREMGenerator, Scene as ThreeScene } from 'three'
import { GenieModel } from './GenieModel'
import type { GenieClip } from './pose'

// The 3D part of the home page, loaded lazily (React.lazy in App) so the onboarding text paints before three.js.
// memo: App re-renders on every feed event; nothing here may rebuild unless `frozen` changes.

const TARGET: [number, number, number] = [0.15, -0.4, 0]
const VIEW_DIR = [1.05, 0.3, 5.8] // camera offset from TARGET on a wide screen
const FPS = 60 // the idle bob is slow: rendering above 60 on 120 Hz screens buys nothing
const maxDpr = () => Math.min(window.devicePixelRatio || 1, 2)
// Orbiting only with a mouse/trackpad: on touch screens OrbitControls sets touch-action:none on the canvas, so a swipe
// over the top of the page would spin the genie instead of scrolling
const ORBIT = window.matchMedia('(pointer: fine)').matches

// Keep the whole genie in frame on narrow screens: back the camera off as the aspect drops.
function FitCamera() {
  const camera = useThree((s) => s.camera)
  const aspect = useThree((s) => s.size.width / s.size.height)
  useEffect(() => {
    const k = Math.max(1, 0.85 / aspect)
    camera.position.set(TARGET[0] + VIEW_DIR[0] * k, TARGET[1] + VIEW_DIR[1] * k, TARGET[2] + VIEW_DIR[2] * k)
    camera.lookAt(...TARGET) // OrbitControls aims too when present; without them (touch) this is the only aim
  }, [camera, aspect])
  return null
}

// Studio lighting baked once into a PMREM environment: the same three light panels the scene used with drei's
// <Environment><Lightformer/></Environment>, without its HDR/EXR loaders in the bundle and without re-baking on renders.
const PANELS: { intensity: number; position: [number, number, number]; scale: [number, number]; color?: string }[] = [
  { intensity: 2, position: [-3, 3, 4], scale: [5, 3] },
  { intensity: 1.2, position: [4, 1, -3], scale: [3, 5], color: '#cfe9e3' },
  { intensity: 0.6, position: [0, -4, 2], scale: [6, 1] },
]
function StudioEnvironment() {
  const get = useThree((s) => s.get)
  useEffect(() => {
    const { gl, scene } = get()
    const room = new ThreeScene()
    room.background = new Color('#000000')
    const plane = new PlaneGeometry(1, 1)
    const materials: MeshBasicMaterial[] = []
    for (const p of PANELS) {
      const mat = new MeshBasicMaterial({ color: new Color(p.color ?? '#ffffff').multiplyScalar(p.intensity), side: DoubleSide, toneMapped: false })
      materials.push(mat)
      const m = new Mesh(plane, mat)
      m.position.set(...p.position)
      m.scale.set(p.scale[0], p.scale[1], 1)
      m.lookAt(0, 0, 0) // drei Lightformer's default target
      room.add(m)
    }
    const pmrem = new PMREMGenerator(gl)
    const env = pmrem.fromScene(room, 0, 0.1, 100)
    scene.environment = env.texture
    return () => {
      scene.environment = null
      env.dispose()
      pmrem.dispose()
      plane.dispose()
      materials.forEach((m) => m.dispose())
    }
  }, [get])
  return null
}

// frameloop="demand" + this driver: at most FPS frames per second, none while the canvas is scrolled out of view
// (on phones the scene sits above the onboarding text) or the tab is hidden (rAF stops there by itself).
function FrameDriver() {
  const invalidate = useThree((s) => s.invalidate)
  const el = useThree((s) => s.gl.domElement)
  useEffect(() => {
    let visible = true
    const io = new IntersectionObserver(([e]) => (visible = e.isIntersecting))
    io.observe(el)
    let last = 0
    let raf = 0
    const tick = (now: number) => {
      raf = requestAnimationFrame(tick)
      if (!visible || now - last < 1000 / FPS - 2) return
      last = now
      invalidate()
    }
    raf = requestAnimationFrame(tick)
    return () => {
      cancelAnimationFrame(raf)
      io.disconnect()
    }
  }, [invalidate, el])
  return null
}

function SceneImpl({ frozen }: { frozen: { clip: GenieClip; t: number } | null }) {
  // Adaptive quality: step the pixel ratio down on devices that cannot hold the frame rate, back up when they can
  const [dpr, setDpr] = useState(maxDpr)
  return (
    <Canvas
      dpr={dpr}
      frameloop="demand"
      camera={{ fov: 32 }}
      // The composer below does the multisampling; canvas MSAA on top of it was paid twice
      gl={{ antialias: false }}
      fallback={<p className="p-6">WebGL is unavailable.</p>}
    >
      <PerformanceMonitor
        flipflops={3}
        onDecline={() => setDpr((d) => Math.max(1, d - 0.5))}
        onIncline={() => setDpr((d) => Math.min(maxDpr(), d + 0.5))}
        onFallback={() => setDpr(1)}
      />
      <FrameDriver />
      <color attach="background" args={['#F4F4F5']} />
      <ambientLight intensity={0.35} />
      <directionalLight position={[3, 4, 5]} intensity={1.6} />
      <StudioEnvironment />
      <FitCamera />
      <Suspense fallback={null}>
        <GenieModel frozen={frozen} interactive />
      </Suspense>
      {ORBIT && <OrbitControls target={TARGET} enablePan={false} minDistance={3.5} maxDistance={14} />}
      {/* Bloom picks up only emissive parts: screen face, rim light, tail smoke and sparks. mipmapBlur off: a single
          NaN pixel spread into a black block for one frame (A/B in DIN-UI-INTEGRATION-20261007-01); the
          smoke shader clamp in smoke.ts fixes the NaN source, this keeps any other one local. */}
      <EffectComposer multisampling={4}>
        <Bloom mipmapBlur={false} intensity={0.9} luminanceThreshold={1} luminanceSmoothing={0.25} />
      </EffectComposer>
    </Canvas>
  )
}

export default memo(SceneImpl)

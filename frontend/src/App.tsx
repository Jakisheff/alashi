import { ContactShadows, Environment, Lightformer, OrbitControls } from '@react-three/drei'
import { Canvas, useThree } from '@react-three/fiber'
import { Bloom, EffectComposer } from '@react-three/postprocessing'
import { Suspense, useEffect } from 'react'
import { GenieModel } from './genie/GenieModel'
import type { GenieClip } from './genie/pose'
import { useScene } from './store'

const TARGET: [number, number, number] = [0.15, -0.4, 0]
const VIEW_DIR = [1.05, 0.3, 5.8] // camera offset from TARGET on a wide screen

// Keep the whole genie in frame on narrow screens: back the camera off as the aspect drops.
function FitCamera() {
  const camera = useThree((s) => s.camera)
  const aspect = useThree((s) => s.size.width / s.size.height)
  useEffect(() => {
    const k = Math.max(1, 0.85 / aspect)
    camera.position.set(TARGET[0] + VIEW_DIR[0] * k, TARGET[1] + VIEW_DIR[1] * k, TARGET[2] + VIEW_DIR[2] * k)
  }, [camera, aspect])
  return null
}

const CLIPS: GenieClip[] = ['idle', 'act', 'accepted', 'rejected']

// Lines from CHARACTER_BRIEF_2026-10-07.md, in English for the demo.
const LINES: Partial<Record<GenieClip, string[]>> = {
  act: ["Don't rub the lamp. Clear the context.", 'You said "small fix". I heard that.'],
  accepted: ['Wish granted. Estimate: two sprints.'],
  rejected: ["Didn't work. At least now it's reproducible."],
}

// Picks a line for each triggered clip; Degenie types it on its own screen (ScreenText).
function useLines() {
  const { clip, take, captions, say } = useScene()
  useEffect(() => {
    const options = LINES[clip]
    if (captions && options) say(options[take % options.length])
    // Only a new trigger speaks; returning to idle keeps the current line on screen.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [take])
}

export default function App() {
  const { clip, captions, speech, play, toggleCaptions } = useScene()
  useLines()

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const i = Number(e.key) - 1
      if (CLIPS[i]) play(CLIPS[i])
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [play])

  return (
    <div className="relative h-full bg-[#ece6da] text-stone-800">
      <Canvas
        dpr={[1, 2]}
        camera={{ fov: 32 }}
        fallback={<p className="p-6">WebGL is unavailable. Degenie is {clip}.</p>}
      >
        <color attach="background" args={['#ece6da']} />
        <ambientLight intensity={0.35} />
        <directionalLight position={[3, 4, 5]} intensity={1.6} />
        <Environment resolution={256}>
          <Lightformer intensity={2} position={[-3, 3, 4]} scale={[5, 3, 1]} />
          <Lightformer intensity={1.2} position={[4, 1, -3]} scale={[3, 5, 1]} color="#cfe9e3" />
          <Lightformer intensity={0.6} position={[0, -4, 2]} scale={[6, 1, 1]} rotation-x={Math.PI / 2} />
        </Environment>
        <FitCamera />
        <Suspense fallback={null}>
          <GenieModel />
        </Suspense>
        <ContactShadows position={[0, -2.05, 0]} scale={4} blur={2.6} opacity={0.35} far={3} />
        <OrbitControls target={TARGET} enablePan={false} minDistance={3.5} maxDistance={14} />
        {/* Bloom picks up only emissive parts: screen face, rim light, tail smoke and sparks. */}
        <EffectComposer>
          <Bloom mipmapBlur intensity={0.9} luminanceThreshold={0.9} luminanceSmoothing={0.25} />
        </EffectComposer>
      </Canvas>

      <div className="absolute top-3 left-3 flex items-center gap-2 rounded-full bg-white/80 px-3 py-1 text-sm">
        <span className="font-semibold">Degenie</span>
        <span className="rounded-full bg-stone-800 px-2 text-xs tracking-wide text-white uppercase">preview</span>
        <span className="hidden text-stone-500 sm:inline">no live game data</span>
      </div>

      <div className="absolute bottom-3 left-1/2 flex -translate-x-1/2 gap-1 rounded-full bg-white/80 p-1 font-mono text-xs whitespace-nowrap sm:text-sm">
        {CLIPS.map((c, i) => (
          <button
            key={c}
            onClick={() => play(c)}
            className={`rounded-full px-2.5 py-0.5 ${c === clip ? 'bg-[#1f4a43] text-white' : 'hover:bg-stone-200'}`}
          >
            <span className="hidden opacity-50 sm:inline">{i + 1} </span>{c}
          </button>
        ))}
        <button onClick={toggleCaptions} className="rounded-full px-2.5 py-0.5 hover:bg-stone-200" aria-pressed={captions}>
          {captions ? 'text on' : 'text off'}
        </button>
      </div>
      <p className="sr-only" aria-live="polite">
        Degenie: {clip}. {speech}
      </p>
    </div>
  )
}

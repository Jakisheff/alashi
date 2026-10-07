import { Canvas, useThree } from '@react-three/fiber'
import { Bloom, EffectComposer } from '@react-three/postprocessing'
import { Outlet, useNavigate, useRouterState, useSearch } from '@tanstack/react-router'
import { Suspense, useEffect, useRef, useState } from 'react'
import { Color, DoubleSide, Mesh, MeshBasicMaterial, PlaneGeometry, PMREMGenerator, Scene as ThreeScene } from 'three'
import { useArenaFeed } from '@fe/feed'
import { GenieModel } from '@fe/genie/GenieModel'
import { LogPage } from '@fe/log/LogPage'
import { useScene } from '@fe/store'

// Desktop companion spike: Degenie in a screen corner over every window, chatting about the watched game,
// with the neon log board in a 9:16 panel under it. The Tauri window is transparent and click-through except
// over elements marked data-hit (their rects are sent to Rust, which toggles cursor-event passthrough).

type Pos = { x: number; y: number }
type Win = {
  startDragging: () => Promise<void>
  scaleFactor: () => Promise<number>
  outerPosition: () => Promise<{ toLogical: (s: number) => Pos }>
  setSize: (s: unknown) => Promise<void>
  setPosition: (p: unknown) => Promise<void>
}
type Tauri = {
  core: { invoke: (cmd: string, args?: object) => Promise<unknown> }
  window: {
    getCurrentWindow: () => Win
    currentMonitor: () => Promise<{ workArea: { position: Pos; size: { width: number; height: number } } } | null>
    LogicalSize: new (w: number, h: number) => unknown
    LogicalPosition: new (x: number, y: number) => unknown
  }
}
const tauri = (window as unknown as { __TAURI__?: Tauri }).__TAURI__

// Window = exactly the content, so it can be dragged anywhere. Logs make it taller downwards (moved up if the
// screen ends first). Keep WIDTH/CLOSED_H in sync with src-tauri/src/main.rs.
const WIDTH = 400
const CLOSED_H = 400 // padding + bubble row + genie + buttons
const panelHeight = () => Math.min(640, Math.max(360, screen.availHeight - CLOSED_H - 40))

const BLOOM = new URLSearchParams(location.search).get('bloom') !== '0'

// Lines from CHARACTER_BRIEF_2026-10-07 (the user's chosen humour direction), said when the game is quiet
const JOKES = [
  'You have three quota resets. Not enough for a second wish.',
  "Don't rub the lamp. Clear the context.",
  'Wish accepted. Estimate: two sprints.',
  'You said "a small change". I heard that.',
  'Did not work. At least now we have a repro.',
  'I am a genie, not a linter. But that diff is long.',
]

/** Same studio lighting as frontend/src/genie/Scene.tsx (not exported there; copied for the spike) */
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
    const mats = PANELS.map((p) => {
      const mat = new MeshBasicMaterial({ color: new Color(p.color ?? '#ffffff').multiplyScalar(p.intensity), side: DoubleSide, toneMapped: false })
      const m = new Mesh(plane, mat)
      m.position.set(...p.position)
      m.scale.set(p.scale[0], p.scale[1], 1)
      m.lookAt(0, 0, 0)
      room.add(m)
      return mat
    })
    const pmrem = new PMREMGenerator(gl)
    const env = pmrem.fromScene(room, 0, 0.1, 100)
    scene.environment = env.texture
    return () => {
      scene.environment = null
      env.dispose()
      pmrem.dispose()
      plane.dispose()
      mats.forEach((m) => m.dispose())
    }
  }, [get])
  return null
}

/** Same framing as the site: camera at TARGET + VIEW_DIR looking at TARGET (OrbitControls did the aiming there) */
const TARGET: [number, number, number] = [0.15, -0.4, 0]
function Aim() {
  const camera = useThree((s) => s.camera)
  useEffect(() => {
    camera.position.set(TARGET[0] + 1.05, TARGET[1] + 0.3, TARGET[2] + 5.8)
    camera.lookAt(...TARGET)
  }, [camera])
  return null
}

function useFitWindow(open: boolean) {
  useEffect(() => {
    if (!tauri) return
    void (async () => {
      const { getCurrentWindow, currentMonitor, LogicalSize, LogicalPosition } = tauri.window
      const w = getCurrentWindow()
      const [s, m, at] = await Promise.all([w.scaleFactor(), currentMonitor(), w.outerPosition()])
      const pos = at.toLogical(s)
      const h = open ? CLOSED_H + 8 + panelHeight() : CLOSED_H
      let y = pos.y
      if (m) {
        const top = m.workArea.position.y / s
        const bottom = top + m.workArea.size.height / s
        if (y + h > bottom) y = Math.max(top, bottom - h)
      }
      await w.setSize(new LogicalSize(WIDTH, h))
      if (y !== pos.y) await w.setPosition(new LogicalPosition(pos.x, y))
    })()
  }, [open])
}

/** Report the rects of every [data-hit] element to Rust whenever they change */
function useHitRegions() {
  useEffect(() => {
    if (!tauri) return
    let last = ''
    const id = setInterval(() => {
      const rects = [...document.querySelectorAll('[data-hit]')].map((el) => {
        const r = el.getBoundingClientRect()
        return { x: r.x, y: r.y, w: r.width, h: r.height }
      })
      const key = JSON.stringify(rects)
      if (key === last) return
      last = key
      void tauri.core.invoke('set_hit_regions', { rects })
    }, 250)
    return () => clearInterval(id)
  }, [])
}

function useJokes(quietMs: number) {
  const speechId = useScene((s) => s.speechId)
  useEffect(() => {
    const id = setTimeout(() => {
      const { say, play } = useScene.getState()
      say(JOKES[Math.floor(Math.random() * JOKES.length)])
      play('accepted')
    }, quietMs)
    return () => clearTimeout(id)
  }, [speechId, quietMs])
}

function useVoice(on: boolean) {
  const speech = useScene((s) => s.speech)
  const speechId = useScene((s) => s.speechId)
  useEffect(() => {
    if (!on || !speech || !('speechSynthesis' in window)) return
    speechSynthesis.cancel()
    const u = new SpeechSynthesisUtterance(speech.replace(/[^\p{L}\p{N}\p{P}\s]/gu, ''))
    u.rate = 1.15
    speechSynthesis.speak(u)
  }, [on, speech, speechId])
}

export function Companion() {
  const open = useRouterState({ select: (s) => s.location.pathname === '/log' })
  const navigate = useNavigate()
  const [game, setGame] = useState<string | null>(null)
  const [voice, setVoice] = useState(false)
  const speech = useScene((s) => s.speech)
  const speechId = useScene((s) => s.speechId)
  const [shownUntil, setShownUntil] = useState(-1) // speechId whose bubble timed out

  useArenaFeed(game, true, '') // live game when chosen in the log panel, labelled sample chatter otherwise
  useJokes(game ? 45_000 : 20_000)
  useVoice(voice)
  useHitRegions()
  useFitWindow(open)

  // Follow the game picked in the log panel
  const panelGame = useRouterState({ select: (s) => (s.location.pathname === '/log' ? (s.location.search as { game?: string }).game : undefined) })
  if (panelGame && panelGame !== game) setGame(panelGame) // adjust state during render: no extra effect pass

  // Each line stays up for 7 s
  const bubble = shownUntil !== speechId
  useEffect(() => {
    const id = setTimeout(() => setShownUntil(speechId), 7000)
    return () => clearTimeout(id)
  }, [speechId])

  // Press and move = drag the whole window; press and release in place = a joke
  const down = useRef<{ x: number; y: number } | null>(null)
  const onPointerDown = (e: React.PointerEvent) => (down.current = { x: e.screenX, y: e.screenY })
  const onPointerMove = (e: React.PointerEvent) => {
    const d = down.current
    if (d && tauri && Math.hypot(e.screenX - d.x, e.screenY - d.y) > 4) {
      down.current = null
      void tauri.window.getCurrentWindow().startDragging()
    }
  }
  const onPointerUp = () => {
    if (!down.current) return
    down.current = null
    const { say, play } = useScene.getState()
    say(JOKES[Math.floor(Math.random() * JOKES.length)])
    play('accepted')
  }

  return (
    <div className="flex h-full flex-col items-end justify-start gap-2 p-3 font-sans">
      <div className="flex h-[70px] items-end">
        {bubble && speech && (
          <div data-hit className="line-clamp-3 max-w-[300px] rounded-2xl rounded-br-sm bg-white/95 px-4 py-2 text-[13px] leading-snug text-zinc-900 shadow-lg ring-1 ring-black/5">
            {speech}
          </div>
        )}
      </div>
      <div
        data-hit
        className="h-[260px] w-[260px] cursor-grab"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        title="Drag to move · click for a joke"
      >
        <Canvas dpr={[1, 2]} camera={{ fov: 32 }} gl={{ alpha: true, antialias: !BLOOM, premultipliedAlpha: false }}>
          <ambientLight intensity={0.35} />
          <directionalLight position={[3, 4, 5]} intensity={1.6} />
          <Aim />
          <StudioEnvironment />
          <Suspense fallback={null}>
            <GenieModel />
          </Suspense>
          {BLOOM && (
            <EffectComposer multisampling={4}>
              <Bloom mipmapBlur={false} intensity={0.9} luminanceThreshold={1} luminanceSmoothing={0.25} />
            </EffectComposer>
          )}
        </Canvas>
      </div>
      <div data-hit className="flex gap-1.5">
        <button
          onClick={() => setVoice((v) => !v)}
          className="grid h-8 w-10 place-items-center rounded-full bg-[#fcee0a] text-black shadow"
          title={voice ? 'Voice on' : 'Voice off'}
          aria-pressed={voice}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <path d="M4 9h4l5-4v14l-5-4H4z" />
            {voice ? <path d="M16.5 8.5a5 5 0 0 1 0 7M19 6a8.5 8.5 0 0 1 0 12" /> : <path d="m17 9 5 6m0-6-5 6" />}
          </svg>
        </button>
        <button
          onClick={() => void navigate(open ? { to: '/' } : { to: '/log', search: { game: game ?? '1' } })}
          className="h-8 rounded-full bg-[#fcee0a] px-4 text-xs font-bold tracking-[.2em] text-black uppercase shadow"
        >
          {open ? 'Hide logs' : 'Logs'}
        </button>
      </div>
      <Outlet />
    </div>
  )
}

export function LogPanel() {
  const { game = '1' } = useSearch({ from: '/log' })
  return (
    <div
      data-hit
      className="panel relative shrink-0 overflow-y-auto rounded-xl shadow-2xl ring-1 ring-[#fcee0a]/40"
      style={{ height: panelHeight(), aspectRatio: '9 / 16' }}
    >
      <LogPage key={game} game={game} />
    </div>
  )
}

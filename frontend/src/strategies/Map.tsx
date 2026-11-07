import { useEffect, useRef, useState, type RefObject } from 'react'
import { timeline, point, type Stage } from './timeline'
import { chiefSprite, REPLAY_TAIL_SECONDS, workerMotion, type AtlasSprite } from './atlasMotion'
import type { Run } from './client'
import './map-player.css'
type Layout = { width: number; height: number; chiefs: number[][]; routes: number[][][]; labels: [string, number, number][] }
const asset = (file: string) => `${import.meta.env.BASE_URL}strategies/${file}`
export type MapTeam = { id: string; stages: Stage[]; completed: boolean }
export function StrategyMap({ runs, online }: { runs: Run[]; online: boolean }) {
  const starts = runs.flatMap(r => Object.values(r.calls).map(c => c.started_at)), origin = starts.length ? Math.min(...starts) : 0
  return <DegenieMap teams={runs.map(r => ({ id: r.id, stages: timeline(r, origin), completed: !!r.result }))} trading={runs.some(r => !!r.brief.trading)} online={online} caption={runs.length ? runs.some(r => r.status === 'running') ? 'Live team tasks · staged movement' : 'Recorded team tasks and receipts · replay' : 'Waiting for two owners'} />
}
function mapLabel(label: string, trading: boolean) {
  if (trading) return label === 'ANALYTICS' ? 'PRODUCTION' : label.startsWith('VERIFY') ? 'EXCHANGE' : label
  return label === 'PARLIAMENT' ? 'COPY CHECK' : label === 'MARKET' ? 'CREATIVE' : label.startsWith('VERIFY') ? 'REVIEW' : 'RESEARCH'
}
function workerRoute(route: number[][], side: number, index: number, trading: boolean) {
  if (trading && index === 0) return side ? [route[0], route[1], [1050, 540], [945, 570]] : [route[0], route[1], [740, 620], [795, 635]]
  if (trading && index === 2) return side ? [route[0], route[1], [1040, 825], [925, 820]] : [route[0], route[1], [700, 830], [800, 835]]
  return route
}
function drawRobot(stage: CanvasRenderingContext2D, atlas: HTMLImageElement, { x, y, direction, sprite, label, carrying, accent }: {
  x: number; y: number; direction: number; sprite: AtlasSprite; label: string; carrying: boolean; accent: string
}) {
  stage.strokeStyle = accent; stage.lineWidth = 3; stage.beginPath(); stage.ellipse(x, y, 28, 12, 0, 0, Math.PI * 2); stage.stroke()
  stage.drawImage(atlas, sprite.frame * 160, (sprite.clip * 8 + direction) * 192, 160, 192, x - 48, y - 100, 96, 115)
  stage.font = '700 18px system-ui'; stage.fillStyle = '#061b2be8'; stage.fillRect(x - 45, y + 17, 90, 25); stage.fillStyle = accent; stage.fillText(label, x, y + 36)
  if (carrying) { stage.fillStyle = '#ffe183'; stage.fillRect(x + 23, y - 49, 13, 17) }
}
function workerDirection(stage: Stage, route: number[][], time: number, progress: number) {
  const a = point(route, Math.max(0, progress - .01)), b = point(route, Math.min(1, progress + .01))
  return ((Math.round(Math.atan2(b[0] - a[0], b[1] - a[1]) / (Math.PI / 4)) + (stage.end !== null && time >= stage.end - .8 ? 4 : 0)) % 8 + 8) % 8
}
function workerLabel(stage: Stage, time: number, side: number, index: number) {
  return stage.end === null || time < stage.end + .6 ? stage.task.endsWith(' confirmed') ? stage.task.split(' ')[0].toUpperCase() : stage.tool ? 'TOOL' : stage.task.toUpperCase().slice(0, 9) : `${side ? 'B' : 'A'}${index + 1}`
}
type MapScene = { stage: CanvasRenderingContext2D; atlas: HTMLImageElement; layout: Layout; t: number; trading: boolean; reduced: boolean }
function drawWorker({ stage, atlas, layout, t, trading, reduced }: MapScene, team: Stage[], side: number, index: number) {
  const route = workerRoute(layout.routes[index + side * 4], side, index, trading)
  const current = team.filter(s => s.route === index && t >= s.start).at(-1)
  const motion = workerMotion(current, t, index * .1, reduced), progress = motion.progress
  let direction = side ? 7 : 1, label = `${side ? 'B' : 'A'}${index + 1}`
  if (current) {
    direction = workerDirection(current, route, t, progress)
    label = workerLabel(current, t, side, index)
    stage.strokeStyle = side ? '#efc77bab' : '#51dfffa0'; stage.lineWidth = 3; stage.beginPath(); stage.moveTo(...route[0] as [number, number]); stage.bezierCurveTo(...route.slice(1).flat() as [number, number, number, number, number, number]); stage.stroke()
  }
  const [x, y] = point(route, progress); drawRobot(stage, atlas, { x, y, direction, sprite: motion, label, carrying: motion.carrying, accent: side ? '#f4c778' : '#63eaff' })
}
function drawTeam(scene: MapScene, teamRuns: MapTeam[], sides: Stage[][], side: number) {
  const { stage, atlas, layout, t, reduced } = scene
  const team = sides[side], r = teamRuns[side], completedAt = r?.completed ? Math.max(0, ...team.map(s => s.end ?? s.work + 1)) : null
  const [chiefX, chiefY] = layout.chiefs[side]
  drawRobot(stage, atlas, { x: chiefX, y: chiefY, direction: side ? 7 : 1, sprite: chiefSprite(completedAt, t, reduced), label: side ? 'B' : 'A', carrying: false, accent: side ? '#f4c778' : '#63eaff' })
  for (let i = 0; i < 4; i++) drawWorker(scene, team, side, i)
}
function drawMapFrame(stage: CanvasRenderingContext2D, terrain: HTMLImageElement, atlas: HTMLImageElement, layout: Layout, teamRuns: MapTeam[], sides: Stage[][], t: number, trading: boolean, reduced: boolean) {
  stage.clearRect(0, 0, 1672, 941); stage.drawImage(terrain, 0, 0, 1672, 941)
  stage.textAlign = 'center'; stage.font = '600 19px system-ui'
  for (const [label, x, y] of layout.labels) { stage.fillStyle = '#061b2be8'; stage.fillRect(x - 80, y - 18, 160, 30); stage.fillStyle = '#d8f7ff'; stage.fillText(mapLabel(label, trading), x, y + 4) }
  const scene = { stage, atlas, layout, t, trading, reduced }
  for (const side of [0, 1]) drawTeam(scene, teamRuns, sides, side)
}
function mapTime(seconds: number) {
  const whole = Math.floor(Math.max(0, seconds))
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`
}
function loadMapAssets(terrain: HTMLImageElement, atlas: HTMLImageElement, signal: AbortSignal) {
  const load = (image: HTMLImageElement, url: string) => new Promise<void>((yes, no) => { image.onload = () => yes(); image.onerror = () => no(new Error('asset')); image.src = url })
  return Promise.all([fetch(asset('map-layout.json'), { signal }).then(r => { if (!r.ok) throw new Error('layout'); return r.json() as Promise<Layout> }), load(terrain, asset('terrain.png')), load(atlas, asset('degenie.png'))])
}
function updateMapProgress(slider: RefObject<HTMLInputElement | null>, timeLabel: RefObject<HTMLOutputElement | null>, duration: number, ongoing: boolean, t: number, playerControls: boolean) {
  if (slider.current) {
    slider.current.max = String(ongoing && playerControls ? Math.max(duration, t) : duration)
    slider.current.value = String(t)
    if (playerControls) slider.current.setAttribute('aria-valuetext', ongoing ? `${mapTime(t)}; duration unknown` : `${mapTime(t)} of ${mapTime(duration)}`)
  }
  if (timeLabel.current) timeLabel.current.textContent = `${mapTime(t)} / ${ongoing ? '—' : mapTime(duration)}`
}
function mapExtent(stages: Stage[]) {
  const duration = stages.length ? Math.max(0, ...stages.map(s => s.end ?? s.work + 1)) + REPLAY_TAIL_SECONDS : 0
  const ongoing = stages.some(s => s.end === null)
  return { duration, ongoing }
}
function advanceClock({ clock, last, now, seek, connected, paused, duration, ongoing, hasStages }: {
  clock: number; last: number; now: number; seek: number | null; duration: number
  connected: boolean; paused: boolean; ongoing: boolean; hasStages: boolean
}) {
  if (seek !== null) clock = seek
  if (connected && !paused && last && hasStages) clock = Math.min(ongoing ? Infinity : duration, clock + Math.min(.1, (now - last) / 1000))
  return clock
}
function mapClock({ stages, clock, last, now, seek, connected, paused, reduced, playerControls }: {
  stages: Stage[]; clock: number; last: number; now: number; seek: number | null
  connected: boolean; paused: boolean; reduced: boolean; playerControls: boolean
}) {
  const { duration, ongoing } = mapExtent(stages)
  clock = advanceClock({ clock, last, now, seek, connected, paused, duration, ongoing, hasStages: stages.length > 0 })
  const time = reduced && !playerControls ? duration : clock
  const ended = playerControls && stages.length > 0 && !ongoing && clock >= duration
  return { clock, duration, ongoing, time, ended }
}
type MapRuntime = { frame: number; disposed: boolean; clock: number; last: number }
type MapCurrent = { teams: MapTeam[]; online: boolean; paused: boolean; onTime?: (time: number) => void }
function activeMapTasks(stages: Stage[], time: number) {
  return stages.filter(s => time >= s.start && (s.end === null || time < s.end)).map(s => s.task).join(',')
}
function createMapFrame({ runtime, current, seek, ended, slider, timeLabel, stage, terrain, atlas, layout, reduced, playerControls, trading, setPaused }: {
  runtime: MapRuntime; current: RefObject<MapCurrent>; seek: RefObject<number | null>; ended: RefObject<boolean>
  slider: RefObject<HTMLInputElement | null>; timeLabel: RefObject<HTMLOutputElement | null>
  stage: CanvasRenderingContext2D; terrain: HTMLImageElement; atlas: HTMLImageElement; layout: Layout; reduced: MediaQueryList
  playerControls: boolean; trading: boolean; setPaused: (paused: boolean) => void
}) {
  function draw(ms: number) {
    if (runtime.disposed) return
    const { teams: teamRuns, online: connected, paused: stopped } = current.current
    const sides = [0, 1].map(side => teamRuns[side]?.stages ?? [])
    const stages = sides.flat()
    const next = mapClock({ stages, clock: runtime.clock, last: runtime.last, now: ms, seek: seek.current, connected, paused: stopped, reduced: reduced.matches, playerControls })
    runtime.clock = next.clock; seek.current = null; runtime.last = ms
    const { duration, ongoing, time: t } = next
    ended.current = next.ended
    if (ended.current && !stopped) setPaused(true)
    updateMapProgress(slider, timeLabel, duration, ongoing, t, playerControls)
    current.current.onTime?.(t)
    stage.canvas.dataset.motionTime = t.toFixed(3)
    stage.canvas.dataset.activeTasks = activeMapTasks(stages, t)
    drawMapFrame(stage, terrain, atlas, layout, teamRuns, sides, t, trading, reduced.matches)
    runtime.frame = requestAnimationFrame(draw)
  }
  return draw
}
function MapCaption({ caption, playerControls, paused, disabled, assetsFailed, onReplay, onToggle }: {
  caption: string; playerControls: boolean; paused: boolean; disabled: boolean; assetsFailed: boolean
  onReplay: () => void; onToggle: () => void
}) {
  return <div className="map-caption"><span>{caption}</span>{!playerControls && <div><button onClick={onReplay} disabled={disabled}>Replay events</button><button onClick={onToggle} disabled={disabled || assetsFailed}>{paused ? "Play replay" : "Pause replay"}</button></div>}</div>
}
function MapPlaybackControls({ playerControls, slider, timeLabel, playing, disabled, assetsFailed, online, onToggle, onReplay, onSeek }: {
  playerControls: boolean; slider: RefObject<HTMLInputElement | null>; timeLabel: RefObject<HTMLOutputElement | null>
  playing: boolean; disabled: boolean; assetsFailed: boolean; online: boolean
  onToggle: () => void; onReplay: () => void; onSeek: (value: string) => void
}) {
  return playerControls ? <div className="map-player" role="group" aria-label="Map replay controls">
    <input ref={slider} className="replay-position" aria-label="Map replay position" type="range" min="0" max="0" step="0.1" defaultValue="0" disabled={disabled || assetsFailed} onChange={e => onSeek(e.target.value)} />
    <div className="map-player-row">
      <button type="button" className="map-player-button" aria-label={playing ? 'Pause map replay' : 'Play map replay'} title={playing ? 'Pause' : 'Play'} disabled={disabled || assetsFailed || !online} onClick={onToggle}>
        {playing ? <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 4h4v16H6zm8 0h4v16h-4z" /></svg> : <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 4v16l12-8z" /></svg>}
      </button>
      <button type="button" className="map-player-button" aria-label="Replay map from the beginning" title="Replay" disabled={disabled || !online} onClick={onReplay}>
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5a7 7 0 1 1-6.2 10.3l1.8-1A5 5 0 1 0 12 7H8v3L4 6l4-4v3z" /></svg>
      </button>
      <output ref={timeLabel} className="map-player-time">0:00 / 0:00</output>
      <span className="map-player-label">Map replay</span>
    </div>
  </div> : <input ref={slider} className="replay-position" aria-label="Replay position in seconds" type="range" min="0" max="20" step="0.1" defaultValue="0" disabled={disabled || assetsFailed} onChange={e => onSeek(e.target.value)} />
}
function useMapPlayback(slider: RefObject<HTMLInputElement | null>, playerControls: boolean) {
  const seek = useRef<number | null>(null), ended = useRef(false)
  const [paused, setPaused] = useState(false), [replay, setReplay] = useState(0)
  const restart = () => { ended.current = false; seek.current = 0; setPaused(false); setReplay(n => n + 1) }
  const togglePlayback = () => {
    if (paused && ended.current) restart()
    else setPaused(p => !p)
  }
  const seekTo = (value: string) => {
    const position = Number(value)
    seek.current = position
    if (playerControls && position < Number(slider.current?.max ?? Infinity)) ended.current = false
    setPaused(true)
  }
  return { seek, ended, paused, setPaused, replay, restart, togglePlayback, seekTo }
}
export function DegenieMap({ teams, online, caption, trading = false, onTime, playerControls = false }: { teams: MapTeam[]; online: boolean; caption: string; trading?: boolean; onTime?: (time: number) => void; playerControls?: boolean }) {
  const canvas = useRef<HTMLCanvasElement>(null), slider = useRef<HTMLInputElement>(null), timeLabel = useRef<HTMLOutputElement>(null)
  const { seek, ended, paused, setPaused, replay, restart, togglePlayback, seekTo } = useMapPlayback(slider, playerControls)
  const [error, setError] = useState(''), [canvasUnavailable, setCanvasUnavailable] = useState(false)
  const current = useRef({ teams, online, paused, onTime }); current.current = { teams, online, paused, onTime }
  useEffect(() => {
    const ctx = canvas.current?.getContext('2d'); if (!ctx) { setCanvasUnavailable(true); return }
    setCanvasUnavailable(false)
    const stage = ctx, terrain = new Image(), atlas = new Image(), controller = new AbortController()
    const runtime: MapRuntime = { frame: 0, disposed: false, clock: 0, last: 0 }
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)')
    async function start() {
      try {
        const [layout] = await loadMapAssets(terrain, atlas, controller.signal)
        if (runtime.disposed) return
        setError('')
        const draw = createMapFrame({ runtime, current, seek, ended, slider, timeLabel, stage, terrain, atlas, layout, reduced, playerControls, trading, setPaused })
        runtime.frame = requestAnimationFrame(draw)
      } catch { if (!runtime.disposed) setError('Map assets could not load.') }
    }
    void start()
    return () => { runtime.disposed = true; controller.abort(); cancelAnimationFrame(runtime.frame) }
  }, [teams.map(r => r.id).join(':'), replay, trading, playerControls])
  const hasStages = teams.some(t => t.stages.length)
  const playing = hasStages && online && !paused && !canvasUnavailable && !error
  return <section className="strategy-map" aria-label="Whole map of Degenie tasks">
    <MapCaption caption={caption} playerControls={playerControls} paused={paused} disabled={!hasStages || canvasUnavailable} assetsFailed={Boolean(error)} onReplay={restart} onToggle={togglePlayback} />
    <canvas ref={canvas} width={1672} height={941} role="img" aria-label={trading ? "Two owner teams of Degenie visualize confirmed trading receipts on the whole map." : "Two owner teams send their Degenie to tasks in parallel, use tools, and return reports."} />
    <MapPlaybackControls playerControls={playerControls} slider={slider} timeLabel={timeLabel} playing={playing} disabled={playerControls ? !hasStages || canvasUnavailable : canvasUnavailable} assetsFailed={Boolean(error)} online={online} onToggle={togglePlayback} onReplay={restart} onSeek={seekTo} />
    {!online && <p role="status">Connection paused. Movement is frozen at the last confirmed data.</p>}{canvasUnavailable ? <p role="alert">Map unavailable in this browser.</p> : error && <p role="alert">{error}</p>}
  </section>
}

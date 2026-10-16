import { useEffect, useRef, useState } from 'react'
import { timeline, point, smooth, type Stage } from './timeline'
import type { Run } from './client'
type Layout = { width: number; height: number; chiefs: number[][]; routes: number[][][]; labels: [string, number, number][] }
const asset = (file: string) => `${import.meta.env.BASE_URL}strategies/${file}`
export type MapTeam = { id: string; stages: Stage[]; completed: boolean }
export function StrategyMap({ runs, online }: { runs: Run[]; online: boolean }) {
  const starts = runs.flatMap(r => Object.values(r.calls).map(c => c.started_at)), origin = starts.length ? Math.min(...starts) : 0
  return <DegenieMap teams={runs.map(r => ({ id: r.id, stages: timeline(r, origin), completed: !!r.result }))} trading={runs.some(r => !!r.brief.trading)} online={online} caption={runs.length ? runs.some(r => r.status === 'running') ? 'Live team tasks · staged movement' : 'Recorded team tasks and receipts · replay' : 'Waiting for two owners'} />
}
export function DegenieMap({ teams, online, caption, trading = false, onTime }: { teams: MapTeam[]; online: boolean; caption: string; trading?: boolean; onTime?: (time: number) => void }) {
  const canvas = useRef<HTMLCanvasElement>(null), slider = useRef<HTMLInputElement>(null), seek = useRef<number | null>(null), [paused, setPaused] = useState(false), [replay, setReplay] = useState(0), [error, setError] = useState('')
  const current = useRef({ teams, online, paused, onTime }); current.current = { teams, online, paused, onTime }
  useEffect(() => {
    const ctx = canvas.current?.getContext('2d'); if (!ctx) return
    const stage = ctx, terrain = new Image(), atlas = new Image(), controller = new AbortController()
    let frame = 0, disposed = false, clock = 0, last = 0
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)')
    const load = (image: HTMLImageElement, url: string) => new Promise<void>((yes, no) => { image.onload = () => yes(); image.onerror = () => no(new Error('asset')); image.src = url })
    async function start() {
      try {
        const [layout] = await Promise.all([fetch(asset('map-layout.json'), { signal: controller.signal }).then(r => { if (!r.ok) throw new Error('layout'); return r.json() as Promise<Layout> }), load(terrain, asset('terrain.png')), load(atlas, asset('degenie.png'))])
        if (disposed) return
        function draw(ms: number) {
          if (disposed) return
          const { teams: teamRuns, online: connected, paused: stopped } = current.current
          const sides = [0, 1].map(side => teamRuns[side]?.stages ?? [])
          const stages = sides.flat(), duration = stages.length ? Math.max(0, ...stages.map(s => s.end ?? s.work + 1)) + .8 : 0
          if (seek.current !== null) { clock = seek.current; seek.current = null }
          if (connected && !stopped && last && stages.length) clock = Math.min(stages.some(s => s.end === null) ? Infinity : duration, clock + Math.min(.1, (ms - last) / 1000))
          last = ms
          const t = reduced.matches ? duration : clock
          if (slider.current) { slider.current.max = String(duration); slider.current.value = String(t) }
          current.current.onTime?.(t)
          stage.canvas.dataset.motionTime = t.toFixed(3)
          stage.canvas.dataset.activeTasks = stages.filter(s => t >= s.start && (s.end === null || t < s.end)).map(s => s.task).join(',')
          stage.clearRect(0, 0, 1672, 941); stage.drawImage(terrain, 0, 0, 1672, 941)
          stage.textAlign = 'center'; stage.font = '600 19px system-ui'
          for (const [label, x, y] of layout.labels) { stage.fillStyle = '#061b2be8'; stage.fillRect(x - 80, y - 18, 160, 30); stage.fillStyle = '#d8f7ff'; stage.fillText(trading ? label === 'ANALYTICS' ? 'PRODUCTION' : label.startsWith('VERIFY') ? 'EXCHANGE' : label : label === 'PARLIAMENT' ? 'COPY CHECK' : label === 'MARKET' ? 'CREATIVE' : label.startsWith('VERIFY') ? 'REVIEW' : 'RESEARCH', x, y + 4) }
          function robot(x: number, y: number, direction: number, clip: number, time: number, label: string, carrying = false, accent = '#63eaff') {
            stage.strokeStyle = accent; stage.lineWidth = 3; stage.beginPath(); stage.ellipse(x, y, 28, 12, 0, 0, Math.PI * 2); stage.stroke()
            const durations = [4.033, 1.133, 1.233], f = reduced.matches ? clip === 2 ? 14 : 0 : Math.floor((time % durations[clip]) / durations[clip] * 24)
            stage.drawImage(atlas, f * 160, (clip * 8 + direction) * 192, 160, 192, x - 48, y - 100, 96, 115)
            stage.font = '700 18px system-ui'; stage.fillStyle = '#061b2be8'; stage.fillRect(x - 45, y + 17, 90, 25); stage.fillStyle = accent; stage.fillText(label, x, y + 36)
            if (carrying) { stage.fillStyle = '#ffe183'; stage.fillRect(x + 23, y - 49, 13, 17) }
          }
          for (const side of [0, 1]) {
            const team = sides[side], r = teamRuns[side], done = !!r?.completed && t >= Math.max(0, ...team.map(s => s.end ?? s.work + 1))
            robot(...layout.chiefs[side] as [number, number], side ? 7 : 1, done ? 2 : 0, t, side ? 'B' : 'A', false, side ? '#f4c778' : '#63eaff')
            for (let i = 0; i < 4; i++) {
              let route = layout.routes[i + side * 4]
              if (trading && i === 0) route = side ? [route[0], route[1], [1050, 540], [945, 570]] : [route[0], route[1], [740, 620], [795, 635]]
              if (trading && i === 2) route = side ? [route[0], route[1], [1040, 825], [925, 820]] : [route[0], route[1], [700, 830], [800, 835]]
              const s = team.filter(s => s.route === i && t >= s.start).at(-1)
              let p = 0, clip = 0, direction = side ? 7 : 1, label = `${side ? 'B' : 'A'}${i + 1}`, carrying = false
              if (s) {
                if (t < s.work) p = smooth((t - s.start - .16) / .64)
                else if (s.end === null || t < s.end - .8) { p = 1; clip = 1 }
                else if (t < s.end - .16) { p = 1 - smooth((t - s.end + .8) / .64); carrying = s.ok }
                else { clip = s.ok && t < s.end + .6 ? 2 : 0 }
                if (reduced.matches) p = s.end === null ? 1 : 0
                const a = point(route, Math.max(0, p - .01)), b = point(route, Math.min(1, p + .01))
                direction = ((Math.round(Math.atan2(b[0] - a[0], b[1] - a[1]) / (Math.PI / 4)) + (s.end !== null && t >= s.end - .8 ? 4 : 0)) % 8 + 8) % 8
                label = s.end === null || t < s.end + .6 ? s.task.endsWith(' confirmed') ? s.task.split(' ')[0].toUpperCase() : s.tool ? 'TOOL' : s.task.toUpperCase().slice(0, 9) : `${side ? 'B' : 'A'}${i + 1}`
                stage.strokeStyle = side ? '#efc77bab' : '#51dfffa0'; stage.lineWidth = 3; stage.beginPath(); stage.moveTo(...route[0] as [number, number]); stage.bezierCurveTo(...route.slice(1).flat() as [number, number, number, number, number, number]); stage.stroke()
              }
              const [x, y] = point(route, p); robot(x, y, direction, clip, t + i * .1, label, carrying, side ? '#f4c778' : '#63eaff')
            }
          }
          frame = requestAnimationFrame(draw)
        }
        frame = requestAnimationFrame(draw)
      } catch { if (!disposed) setError('Map assets could not load. Run receipts remain available below.') }
    }
    void start()
    return () => { disposed = true; controller.abort(); cancelAnimationFrame(frame) }
  }, [teams.map(r => r.id).join(':'), replay, trading])
  return <section className="strategy-map" aria-label="Whole map of Degenie tasks">
    <div className="map-caption"><span>{caption}</span><div><button onClick={() => { setPaused(false); seek.current = 0; setReplay(n => n + 1) }} disabled={!teams.some(t => t.stages.length)}>Replay events</button><button onClick={() => setPaused(p => !p)} disabled={!teams.some(t => t.stages.length)}>{paused ? "Play replay" : "Pause replay"}</button></div></div>
    <canvas ref={canvas} width={1672} height={941} role="img" aria-label={trading ? "Two owner teams of Degenie visualize confirmed trading receipts on the whole map." : "Two owner teams send their Degenie to tasks in parallel, use tools, and return reports."} />
    <input ref={slider} className="replay-position" aria-label="Replay position in seconds" type="range" min="0" max="20" step="0.1" defaultValue="0" onChange={e => { seek.current = Number(e.target.value); setPaused(true) }} />
    {!online && <p role="status">Connection paused. Movement is frozen at the last confirmed data.</p>}{error && <p role="alert">{error}</p>}
  </section>
}

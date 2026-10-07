// Force layout off the main thread. Same forces as the HackAlem engine (charge, link, collide, pull to the party
// centre); the page only applies the positions it receives and paints. Protocol: see layout.ts.
import { forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, type Simulation, type SimulationNodeDatum } from 'd3-force'

type Node = SimulationNodeDatum & { cx: number; cy: number; r: number }

export type StartMsg = {
  type: 'start'
  gen: number
  /** x, y, fx, fy (NaN = free), cx, cy (pull target), r (collide radius) per node */
  data: Float64Array
  links: Uint32Array
  charge: number
  linkDist: number
  pull: number
  alpha: number
  /** Ticks run before the first message while alpha is above this (the chaotic start stays off screen) */
  warmUntil: number
  /** Background run: post a snapshot every SNAPSHOT_MS and run as fast as possible */
  silent: boolean
}
export type InMsg =
  | StartMsg
  | { type: 'stop' }
  | { type: 'fix'; i: number; x: number; y: number }
  | { type: 'free'; i: number }
  | { type: 'alphaTarget'; v: number }
export type OutMsg = { type: 'tick' | 'end'; gen: number; xy: Float64Array; alpha: number }

const ALPHA_MIN = 0.02
const STRIDE = 7
const SNAPSHOT_MS = 300
const FRAME_MS = 16

let sim: Simulation<Node, undefined> | null = null
let nodes: Node[] = []
let gen = 0
let silent = false
let timer: ReturnType<typeof setTimeout> | 0 = 0
let lastPost = 0

function post(type: OutMsg['type']) {
  const xy = new Float64Array(nodes.length * 2)
  nodes.forEach((n, i) => {
    xy[i * 2] = n.x!
    xy[i * 2 + 1] = n.y!
  })
  lastPost = performance.now()
  const msg: OutMsg = { type, gen, xy, alpha: sim?.alpha() ?? 0 }
  ;(self as unknown as Worker).postMessage(msg, [xy.buffer])
}

function stop() {
  if (timer) clearTimeout(timer)
  timer = 0
}

function step() {
  timer = 0
  if (!sim) return
  const t0 = performance.now()
  // On screen: up to 3 ticks per frame on big graphs (the old main-thread pace); background: as much as fits
  const budget = silent ? 40 : 12
  const perBatch = silent ? Infinity : 1 + Math.min(2, Math.floor(nodes.length / 1000))
  let n = 0
  do {
    sim.tick()
    n++
  } while (n < perBatch && performance.now() - t0 < budget && sim.alpha() >= ALPHA_MIN)
  // d3 semantics: the run ends when alpha falls below alphaMin (an alphaTarget above it keeps it going)
  if (sim.alpha() < ALPHA_MIN) {
    post('end')
    return
  }
  if (!silent || performance.now() - lastPost > SNAPSHOT_MS) post('tick')
  timer = setTimeout(step, silent ? 0 : Math.max(0, FRAME_MS - (performance.now() - t0)))
}

function start(m: StartMsg) {
  stop()
  gen = m.gen
  silent = m.silent
  const d = m.data
  nodes = []
  for (let i = 0; i < d.length / STRIDE; i++) {
    const o = i * STRIDE
    nodes.push({
      x: d[o],
      y: d[o + 1],
      fx: isNaN(d[o + 2]) ? null : d[o + 2],
      fy: isNaN(d[o + 3]) ? null : d[o + 3],
      cx: d[o + 4],
      cy: d[o + 5],
      r: d[o + 6],
    })
  }
  const links: { source: number; target: number }[] = []
  for (let i = 0; i < m.links.length; i += 2) links.push({ source: m.links[i], target: m.links[i + 1] })
  sim = forceSimulation(nodes)
    .alphaMin(ALPHA_MIN)
    .alpha(m.alpha)
    .stop()
    .force('charge', forceManyBody<Node>().strength(-m.charge).distanceMax(400))
    .force('link', forceLink<Node, { source: number; target: number }>(links).distance(m.linkDist).strength(0.6))
    .force('collide', forceCollide<Node>((n) => n.r + 2))
    .force('cx', forceX<Node>((n) => n.cx).strength(m.pull))
    .force('cy', forceY<Node>((n) => n.cy).strength(m.pull))
  for (let i = 0; i < 300 && sim.alpha() > m.warmUntil; i++) sim.tick()
  if (sim.alpha() < ALPHA_MIN) {
    post('end')
    return
  }
  post('tick')
  timer = setTimeout(step, 0)
}

self.onmessage = (e: MessageEvent<InMsg>) => {
  const m = e.data
  if (m.type === 'start') start(m)
  else if (m.type === 'stop') stop()
  else if (m.type === 'fix') {
    const n = nodes[m.i]
    if (n) [n.fx, n.fy] = [m.x, m.y]
  } else if (m.type === 'free') {
    const n = nodes[m.i]
    if (n) [n.fx, n.fy] = [null, null]
  } else if (m.type === 'alphaTarget' && sim) {
    sim.alphaTarget(m.v)
    // like simulation.restart(): resume a finished run while something is being dragged
    if (!timer && m.v > 0) timer = setTimeout(step, 0)
  }
}

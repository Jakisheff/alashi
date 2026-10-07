import type { InMsg, OutMsg } from './layout.worker'

// Main-thread handle of the force layout worker. The page hands over node positions, pull targets and radii,
// receives positions back (transferred buffers) and writes them into its own node objects, then repaints.

export type PhysNode = { x: number; y: number; fx?: number | null; fy?: number | null }

export type StartOpts = {
  center: (i: number) => { x: number; y: number }
  radius: (i: number) => number
  charge: number
  linkDist: number
  pull: number
  alpha: number
  /** Ticks run inside the worker before the first update while alpha is above this */
  warmUntil?: number
  /** Background layout: snapshots every ~300 ms instead of every frame */
  silent?: boolean
}

export const ALPHA_MIN = 0.02

export function createPhysics<N extends PhysNode>(onTick: (alpha: number) => void, onEnd: () => void) {
  const worker = new Worker(new URL('./layout.worker.ts', import.meta.url), { type: 'module' })
  let nodes: N[] = []
  let index = new Map<N, number>()
  let gen = 0
  let alpha = 0
  let running = false
  const send = (m: InMsg, transfer: Transferable[] = []) => worker.postMessage(m, transfer)

  worker.onmessage = (e: MessageEvent<OutMsg>) => {
    const m = e.data
    if (m.gen !== gen) return // a newer start replaced this run
    for (let i = 0; i < nodes.length; i++) {
      nodes[i].x = m.xy[i * 2]
      nodes[i].y = m.xy[i * 2 + 1]
    }
    alpha = m.alpha
    if (m.type === 'end') running = false
    onTick(alpha)
    if (m.type === 'end') onEnd()
  }

  return {
    start(list: N[], links: [N, N][], o: StartOpts) {
      nodes = list
      index = new Map(list.map((n, i) => [n, i]))
      const data = new Float64Array(list.length * 7)
      list.forEach((n, i) => {
        const c = o.center(i)
        data.set([n.x, n.y, n.fx ?? NaN, n.fy ?? NaN, c.x, c.y, o.radius(i)], i * 7)
      })
      const pairs = new Uint32Array(links.length * 2)
      links.forEach(([s, t], i) => {
        pairs[i * 2] = index.get(s)!
        pairs[i * 2 + 1] = index.get(t)!
      })
      alpha = o.alpha
      running = true
      send(
        {
          type: 'start',
          gen: ++gen,
          data,
          links: pairs,
          charge: o.charge,
          linkDist: o.linkDist,
          pull: o.pull,
          alpha: o.alpha,
          warmUntil: o.warmUntil ?? 1,
          silent: !!o.silent,
        },
        [data.buffer, pairs.buffer],
      )
    },
    stop() {
      gen++
      running = false
      send({ type: 'stop' })
    },
    /** Pin a node (drag, focused agent); x/y in graph units */
    fix(n: N, x: number, y: number) {
      const i = index.get(n)
      if (i !== undefined) send({ type: 'fix', i, x, y })
    },
    free(n: N) {
      const i = index.get(n)
      if (i !== undefined) send({ type: 'free', i })
    },
    /** Keep the layout warm while dragging (d3 alphaTarget + restart) */
    alphaTarget(v: number) {
      if (v > 0) running = true
      send({ type: 'alphaTarget', v })
    },
    alpha: () => alpha,
    running: () => running,
    nodes: () => nodes,
    terminate: () => worker.terminate(),
  }
}

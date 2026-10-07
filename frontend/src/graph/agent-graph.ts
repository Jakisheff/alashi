import * as d3 from 'd3'
import { ALPHA_MIN, createPhysics } from './layout'
import { KIND_TITLE, ROLE_COLOR, ROLE_TITLE, formatCash, roleSymbolPath } from './roles'
import type { GraphAgent, GraphData, GraphEdge, GraphParty } from './types'
import { DEFAULT_VIEW, type GraphView } from './view'

// Agent network: D3 (force, zoom, drag) + SVG. Port of HackAlem dai-front money-graph.ts (67d9d60), remapped:
// node = agent, cluster = game party, edge = aggregated agent -> agent interactions, timelapse = rounds.
// Contract between AgentGraph (imperative D3) and GraphPage (React).

export type AgentGraphViewState = GraphView & {
  focus: string | null
  /** The owner's agent (?agent=): white ring, like HackAlem seeds */
  mine: string | null
  /** Timelapse round, null: off */
  play: number | null
  range: [number, number] | null
}

export type AgentGraphCallbacks = {
  onSelect?: (gid: string) => void
  onBackground?: () => void
}

export type AgentGraph = {
  /** Calling again keeps positions of known ids; layout reruns only if the set of nodes/edges changed */
  setData: (data: GraphData) => void
  setView: (view: AgentGraphViewState, opts?: { fit?: boolean }) => void
  flyTo: (gid: string) => void
  hover: (gid: string | null) => void
  fit: () => void
  /** Zoom in (factor > 1) or out (< 1) around the centre */
  zoomBy: (factor: number) => void
  /** How many nodes and links are visible now (timelapse ghosts excluded) */
  counts: () => { n: number; e: number }
  destroy: () => void
}

// Own palette of the drawing (dark canvas in the spirit of a graph view). Role colours come from roles.ts.
const MONO = 'ui-monospace, SFMono-Regular, Menlo, monospace'
const SANS = 'ui-sans-serif, system-ui, sans-serif'
const BG = '#17171c'
const CL = ['#8ec5ff', '#ffb86b', '#b8e986', '#f5a3c7', '#c9b6ff', '#7fe0d4', '#ffd86b', '#ff9e9e']
const STUB = '#5a5a60'
const DIM = '#3c3c44'
const EDGE = '#777786'
const ORIGIN = { x: 0, y: 0 }
const NO_ROLE = 'role unknown'

const LOG_MIN = Math.log(5e5)
const LOG_SPAN = Math.log(5e7) - LOG_MIN
/** Width 1-8 by log of an edge's weight between 0.5M and 50M simulated cash; a vote counts as 0.5M. */
const edgeWidth = (e: GraphEdge) => {
  const w = Math.max(e.sum, e.count * 5e5)
  return 1 + 7 * Math.min(1, Math.max(0, (Math.log(w) - LOG_MIN) / LOG_SPAN))
}
const esc = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`)
const push = <K, V>(m: Map<K, V[]>, key: K, v: V) => {
  const list = m.get(key)
  if (list) list.push(v)
  else m.set(key, [v])
}

type GNode = d3.SimulationNodeDatum & {
  id: string
  /** Stub nodes (only referenced by edges) carry just the id */
  n: Partial<GraphAgent>
  stub: boolean
  x: number
  y: number
  deg: number
  layer: number
  ghost: boolean
  /** Target and start of the layered-layout animation */
  tx: number
  ty: number
  sx: number
  sy: number
}

type GLink = {
  id: string
  source: GNode
  target: GNode
  e: GraphEdge
  first: number | null
  days: number[] | null
  recip: boolean
  ghost: boolean
  /** Link is on screen now: neighbours on hover */
  vis: boolean
  index?: number
}

type Hull = [number, GNode[]]
type Box = { x: number; y: number; w: number; h: number }

// Rendering: nodes, links, party hulls and labels are painted on one <canvas> per frame (batched by style), so a
// 2.5K-agent / 8.5K-link network stays interactive. The transparent SVG on top only carries zoom/drag/hover events
// and the pulse ring. HackAlem drew every element as SVG with per-element transitions, which stalled at this size.
export function createAgentGraph(el: HTMLElement, cb: AgentGraphCallbacks = {}): AgentGraph {
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
  const duration = (ms: number) => (reduced ? 0 : ms)
  let destroyed = false

  const root = d3
    .select(el)
    .style('position', 'absolute')
    .style('inset', '0')
    .style('background', BG)
    .style('background-image', 'radial-gradient(ellipse at 50% 45%, #252334 0%, transparent 65%)')
    .style('overflow', 'hidden')
  const canvas = root.append('canvas').attr('data-graph', '').style('position', 'absolute').style('inset', '0').node()!
  const ctx = canvas.getContext('2d')!
  const svg = root
    .append('svg')
    .attr('width', '100%')
    .attr('height', '100%')
    .style('position', 'absolute')
    .style('inset', '0')
    .style('display', 'block')
    .style('cursor', 'grab')
    .attr('aria-label', 'Interactive agent interaction graph')
    .attr('role', 'group')
  const g = svg.append('g')
  const gR = g.append('g')
  // Pinch and wheel over the graph always belong to the graph. A Mac trackpad pinch arrives as ctrl+wheel: d3-zoom
  // lets it through once the zoom limit is reached, and the browser then zooms the whole page (controls vanish).
  // Safari also sends its own gesture events; touch screens get touch-action: none.
  const keepGesture = (e: Event) => e.preventDefault()
  el.addEventListener('wheel', keepGesture, { passive: false })
  el.addEventListener('gesturestart', keepGesture)
  el.addEventListener('gesturechange', keepGesture)
  svg.style('touch-action', 'none')
  const tip = root
    .append('div')
    .style('position', 'absolute')
    .style('pointer-events', 'none')
    .style('display', 'none')
    .style('z-index', '5')
    .style('background', 'rgba(24,24,27,.96)')
    .style('border', '1px solid #3a3a40')
    .style('border-radius', '8px')
    .style('padding', '8px 10px')
    .style('color', '#ececf0')
    .style('font', `13px/1.4 ${SANS}`)
    .style('min-width', '220px')
    .style('box-shadow', '0 8px 28px rgba(0,0,0,.45)')
  // Canvas has no focusable elements: a screen-reader/keyboard list of the visible agents stands in for them
  const a11y = root.append('ul').attr('class', 'sr-only').attr('aria-label', 'Agents on the graph')

  let N = new Map<string, GNode>()
  let E: GLink[] = []
  // Adjacency over all edges: neighbourhood walk and hover neighbours without scanning every edge
  let outAdj = new Map<string, GLink[]>()
  let inAdj = new Map<string, GLink[]>()
  let passports = new Map<number, GraphParty>()
  let top = new Set<string>()
  let V: AgentGraphViewState = { ...DEFAULT_VIEW, focus: null, mine: null, play: null, range: null }
  let vis: { nodes: GNode[]; edges: GLink[] } = { nodes: [], edges: [] }
  let hoverId: string | null = null
  let first = true
  /** Nodes/edges changed after the first draw: the next setView relays the graph */
  let dirty = false
  let pendingFit = false
  let k = 1
  let camT: d3.Timer | null = null
  let layoutT: d3.Timer | null = null
  // Overview: the whole network is laid out in the background ahead of time (while a neighbourhood is open) and remembered.
  // Switching modes does not relay the network: each mode restores its positions and camera.
  type ONode = { id: string; x: number; y: number }
  const ovPos = new Map<string, { x: number; y: number }>()
  let ovAlpha = 0
  /** Which mode the main simulation holds: its alpha is unfinished physics of that mode only */
  let simMode: GraphView['mode'] | null = null
  // Background overview layout in its own worker; every snapshot lands in ovPos
  const ovPhys = createPhysics<ONode>(
    () => {
      for (const d of ovPhys.nodes()) ovPos.set(d.id, { x: d.x, y: d.y })
    },
    () => undefined,
  )
  const cams = new Map<string, { t: d3.ZoomTransform; focus: string | null }>()

  let T = d3.zoomIdentity
  let hullGroups: Hull[] = []
  let shownLabels: GNode[] = []
  let leaving: { nodes: GNode[]; edges: GLink[] } = { nodes: [], edges: [] }

  const zoom = d3
    .zoom<SVGSVGElement, unknown>()
    .scaleExtent([0.15, 6])
    .on('zoom', (e: d3.D3ZoomEvent<SVGSVGElement, unknown>) => {
      g.attr('transform', e.transform.toString())
      T = e.transform
      k = e.transform.k
      // The user moves the camera (wheel, drag): the pending 'fit after settling' is dropped,
      // otherwise a few seconds later the camera would jump away from what they were looking at
      if (e.sourceEvent) pendingFit = false
      labels(0)
      requestDraw()
    })

  // Force layout runs in a Web Worker (layout.worker.ts): the main thread only applies positions and paints,
  // so a 2.5K-agent layout no longer blocks hover, zoom or the rest of the page.
  const sim = createPhysics<GNode>(
    () => draw(),
    () => {
      if (pendingFit) {
        pendingFit = false
        fit(500)
      }
    },
  )

  function setData(d: GraphData) {
    // Node and edge objects are reused by id/key: positions, velocities and fx/fy survive,
    // the simulation and layout timers keep working with the same objects.
    const prevN = N
    const prevE = new Map(E.map((l) => [l.id, l]))
    let reused = 0
    const node = (gid: string, n: Partial<GraphAgent>, stub: boolean) => {
      const o = prevN.get(gid)
      if (o) {
        reused++
        o.n = n
        o.stub = stub
        o.deg = 0
        N.set(gid, o)
      } else N.set(gid, { id: gid, n, stub, x: NaN, y: NaN, deg: 0, layer: 0, ghost: false, tx: NaN, ty: NaN, sx: NaN, sy: NaN })
    }
    N = new Map()
    E = []
    outAdj = new Map()
    inAdj = new Map()
    top = new Set(d.top)
    passports = new Map(d.parties.map((c) => [c.id, c]))
    for (const n of d.agents) node(n.id, n, false)
    const firstDay = new Map<string, number>()
    const days = new Map<string, number[]>()
    for (const t of d.interactions) {
      if (!t.ok) continue
      const key = t.src + '|' + t.dst
      const dd = t.round
      firstDay.set(key, Math.min(firstDay.get(key) ?? 99, dd))
      push(days, key, dd)
    }
    const keys = new Set<string>()
    for (const e of d.edges) {
      for (const gid of [e.src, e.dst]) if (!N.has(gid)) node(gid, { id: gid }, true)
      const key = e.src + '|' + e.dst
      keys.add(key)
      const old = prevE.get(key)
      const l: GLink = old ?? { id: key, source: N.get(e.src)!, target: N.get(e.dst)!, e, first: null, days: null, recip: false, ghost: false, vis: false }
      if (old) reused++
      l.e = e
      l.first = firstDay.get(key) ?? null
      l.days = days.get(key) ?? null
      E.push(l)
      push(outAdj, e.src, l)
      push(inAdj, e.dst, l)
      l.source.deg++
      l.target.deg++
    }
    for (const l of E) l.recip = keys.has(l.e.dst + '|' + l.e.src)
    if (prevN.size === 0) first = true
    else if (reused !== prevN.size + prevE.size || N.size !== prevN.size || E.length !== prevE.size) dirty = true
    if (first || dirty) precomputeOverview()
  }

  const R = (o: GNode) => (o.n.score == null ? 3 : 3 + 9 * o.n.score) * V.nodeScale
  const lit = (o: GNode) => {
    const h = V.highlight
    if (!h) return true
    if (h === 'mine') return o.id === V.mine
    return o.n.role === h
  }
  const color = (o: GNode) => {
    // Legend highlight: other nodes lose colour but keep their place
    if (!lit(o)) return DIM
    if (o.stub || !o.n.role) return STUB
    if (V.colorBy === 'party') return CL[(o.n.party || 0) % CL.length]
    return ROLE_COLOR[o.n.role]
  }
  const ew = (l: GLink) => (0.5 + (2.2 * (edgeWidth(l.e) - 1)) / 7) * V.edgeScale
  const usesSim = () => !(V.mode === 'local' && V.layout === 'layers')
  const roleTitle = (o: GNode) => (o.n.role ? ROLE_TITLE[o.n.role] : NO_ROLE)
  const label = (o: GNode) => o.n.name ?? o.id.slice(0, 8)
  const isMine = (o: GNode) => o.id === V.mine

  // ---------- visibility ----------
  function compute() {
    let ids: Map<string, number>
    const focus = V.focus
    if (V.mode === 'local' && focus && N.has(focus)) {
      ids = new Map([[focus, 0]])
      const walk = (dir: 1 | -1) => {
        const adj = dir < 0 ? inAdj : outAdj
        let front = [focus]
        for (let d = 1; d <= V.depth; d++) {
          const nx: string[] = []
          for (const a of front) {
            for (const l of adj.get(a) ?? []) {
              const b = dir < 0 ? l.source.id : l.target.id
              if (!ids.has(b)) {
                ids.set(b, dir * d)
                nx.push(b)
              }
            }
          }
          front = nx
        }
      }
      if (V.dirIn) walk(-1)
      if (V.dirOut) walk(1)
    } else if (V.mode === 'local') {
      ids = new Map()
    } else {
      ids = new Map()
      for (const id of N.keys()) ids.set(id, 0)
    }
    const pass = (o: GNode) => {
      if (V.mode === 'local' && o.id === V.focus) return true
      if (V.roles && o.n.role && !V.roles.includes(o.n.role)) return false
      if (V.parties && o.n.party != null && !V.parties.includes(o.n.party)) return false
      if (V.hideDead && o.n.alive === false) return false
      if (!V.showIsolated && o.deg === 0 && !isMine(o)) return false
      return true
    }
    const nodes = [...ids.keys()].map((id) => N.get(id)!).filter(pass)
    for (const o of nodes) o.layer = ids.get(o.id)!
    const S = new Set(nodes.map((o) => o.id))
    let edges = E.filter((l) => S.has(l.source.id) && S.has(l.target.id))
    if (V.mode === 'local' && !V.between) {
      edges = edges.filter((l) => {
        const a = l.source.layer
        const b = l.target.layer
        return b === a + 1 && (a >= 0 || b <= 0)
      })
    }
    // rounds: range and timelapse
    const timed = V.play != null || !!V.range
    for (const l of edges) {
      let on = true
      if (V.range && l.days) {
        const [from, to] = V.range
        on = l.days.some((d) => d >= from && d <= to)
      }
      if (V.play != null && l.first) on = on && l.first <= V.play
      l.ghost = !on || (timed && !l.days)
    }
    const grown = new Set<string>()
    for (const l of edges) {
      if (!l.ghost) {
        grown.add(l.source.id)
        grown.add(l.target.id)
      }
    }
    for (const o of nodes) o.ghost = timed && !grown.has(o.id) && !isMine(o) && o.id !== V.focus
    for (const l of E) l.vis = false
    for (const l of edges) l.vis = true
    vis = { nodes, edges }
  }

  // ---------- layouts ----------
  function clusterCenters(nodes: GNode[]) {
    const by = d3.rollup(
      nodes,
      (v) => v.length,
      (o) => o.n.party ?? -1,
    )
    const ids = [...by.keys()].sort((a, b) => by.get(b)! - by.get(a)!)
    const C = new Map<number, { x: number; y: number }>()
    ids.forEach((id, i) => {
      if (i === 0) {
        C.set(id, ORIGIN)
        return
      }
      // Alashi parties are small (2-12 agents): a tighter spiral than HackAlem's 2.2K-node clusters
      const a = i * 2.4
      const r = 70 + 55 * Math.sqrt(i)
      C.set(id, { x: r * Math.cos(a), y: r * Math.sin(a) })
    })
    return C
  }

  function layers(nodes: GNode[], edges: GLink[]) {
    const T = new Map<string, { x: number; y: number }>()
    const sumTo = new Map<string, number>()
    for (const l of edges) {
      sumTo.set(l.source.id, (sumTo.get(l.source.id) || 0) + l.e.sum + l.e.count)
      sumTo.set(l.target.id, (sumTo.get(l.target.id) || 0) + l.e.sum + l.e.count)
    }
    d3.group(nodes, (o) => o.layer).forEach((list, L) => {
      list.sort((a, b) => (sumTo.get(b.id) || 0) - (sumTo.get(a.id) || 0))
      const per = list.length > 30 ? 22 : 12
      list.forEach((o, i) => {
        const c = Math.floor(i / per)
        const row = i % per
        const inCol = Math.min(per, list.length - c * per)
        const dir = L < 0 ? -1 : 1
        T.set(o.id, { x: L * 210 + dir * c * 58 * (L === 0 ? 0 : 1), y: (row - (inCol - 1) / 2) * 30 })
      })
    })
    return T
  }

  /** Party centre of a node in the overview; in a neighbourhood: the origin. Unknown parties fall back to the origin. */
  function centers(nodes: GNode[]) {
    const C = V.mode === 'overview' ? clusterCenters(nodes) : null
    return (o: GNode) => (C && C.get(o.n.party ?? -1)) || ORIGIN
  }

  /** Background overview layout in the worker; snapshots fill ovPos. */
  function precomputeOverview() {
    const all = [...N.values()]
    const C = clusterCenters(all)
    const center = (o: GNode) => C.get(o.n.party ?? -1) ?? ORIGIN
    const nodes: ONode[] = all.map((o) => {
      const c = center(o)
      const p = ovPos.get(o.id) ?? { x: c.x + (Math.random() - 0.5) * 60, y: c.y + (Math.random() - 0.5) * 60 }
      return { id: o.id, x: p.x, y: p.y }
    })
    const byId = new Map(nodes.map((d) => [d.id, d]))
    ovPhys.start(
      nodes,
      E.map((l) => [byId.get(l.source.id)!, byId.get(l.target.id)!]),
      { center: (i) => center(all[i]), radius: (i) => R(all[i]), charge: V.charge, linkDist: V.linkDist, pull: V.clusterPull, alpha: 1, silent: true },
    )
  }
  function stopPrecompute() {
    if (ovPhys.running()) ovPhys.stop()
  }
  /** Take the background layout (even unfinished: ovPos holds its latest snapshot). Returns the alpha to continue from. */
  function takeOverview() {
    let alpha = ovAlpha
    ovAlpha = 0
    if (ovPhys.running()) {
      alpha = Math.max(alpha, ovPhys.alpha())
      ovPhys.stop()
    }
    return alpha
  }

  /** Move nodes smoothly to target positions (layered layout, back to overview). */
  function morph(nodes: GNode[], target: (o: GNode) => { x: number; y: number }, animate: boolean, done?: () => void) {
    for (const o of nodes) {
      const t = target(o)
      o.tx = t.x
      o.ty = t.y
      // a node shown for the first time appears in place instead of flying out of the centre
      if (isNaN(o.x)) {
        o.x = t.x
        o.y = t.y
      }
      o.sx = o.x
      o.sy = o.y
    }
    if (!animate) {
      for (const o of nodes) {
        o.x = o.tx
        o.y = o.ty
      }
      draw()
      done?.()
      return
    }
    const t0 = performance.now()
    const tm = d3.timer(() => {
      const p = Math.min(1, (performance.now() - t0) / 700)
      const e = d3.easeCubicInOut(p)
      for (const o of nodes) {
        o.x = o.sx + (o.tx - o.sx) * e
        o.y = o.sy + (o.ty - o.sy) * e
      }
      draw()
      if (p >= 1) {
        tm.stop()
        layoutT = null
        done?.()
      }
    })
    layoutT = tm
  }

  /** Lay out visible nodes. Returns true if visible physics started (camera should refit at the end). */
  function place(animate: boolean, entering: boolean): boolean {
    const { nodes, edges } = vis
    sim.stop()
    layoutT?.stop()
    layoutT = null
    animate = animate && !reduced
    if (!usesSim()) {
      const T = layers(nodes, edges)
      for (const o of nodes) {
        o.fx = null
        o.fy = null
      }
      morph(nodes, (o) => T.get(o.id)!, animate)
      return false
    }
    const center = centers(nodes)
    const pull = V.mode === 'overview' ? V.clusterPull : 0.04
    // warmUntil: ticks run inside the worker before the first frame while alpha is above it
    const run = (alpha: number, warmUntil = 1) => {
      for (const o of nodes) {
        o.fx = V.mode === 'local' && o.id === V.focus ? 0 : null
        o.fy = o.fx
      }
      sim.start(
        nodes,
        edges.map((l) => [l.source, l.target]),
        { center: (i) => center(nodes[i]), radius: (i) => R(nodes[i]), charge: V.charge, linkDist: V.linkDist, pull, alpha, warmUntil },
      )
      simMode = V.mode
    }

    if (V.mode === 'overview' && entering) {
      let alpha = takeOverview()
      let fresh = 0
      for (const o of nodes) {
        if (ovPos.has(o.id)) continue
        const c = center(o)
        ovPos.set(o.id, { x: c.x + (Math.random() - 0.5) * 60, y: c.y + (Math.random() - 0.5) * 60 })
        fresh++
      }
      if (fresh) alpha = Math.max(alpha, fresh > nodes.length * 0.5 ? 1 : 0.3)
      if (alpha > 0.5) {
        // Background did not finish: the worker runs the chaotic start before the first frame, the rest settles on screen
        for (const o of nodes) Object.assign(o, ovPos.get(o.id))
        run(alpha, reduced ? ALPHA_MIN : 0.45)
        draw()
        return !reduced
      }
      morph(nodes, (o) => ovPos.get(o.id)!, animate, () => {
        if (alpha >= ALPHA_MIN && !reduced) run(alpha)
      })
      return false
    }

    // Neighbourhood in force mode or overview relayout after filters/forces change: warm start from current positions
    const fresh = nodes.filter((o) => isNaN(o.x))
    for (const o of fresh) {
      const c = center(o)
      o.x = c.x + (Math.random() - 0.5) * 60
      o.y = c.y + (Math.random() - 0.5) * 60
    }
    if (reduced) {
      // reduced motion: the worker settles the layout completely before the first frame
      run(0.5, ALPHA_MIN)
      return false
    }
    run(animate ? 0.5 : 0.3)
    return true
  }

  // ---------- drawing (canvas) ----------
  // Alpha of every drawn item animates toward a target (enter/exit fades, hover dimming): one timer, not 11K transitions.
  // Keys: node id, 'L'+link id, 'T'+node id (label), 'H'+party id (hull).
  const alpha = new Map<string, number>()
  const goal = new Map<string, number>()
  let from = new Map<string, number>()
  let fadeStart = 0
  let fadeMs = 0
  let fadeT: d3.Timer | null = null
  const A = (key: string) => alpha.get(key) ?? 0
  function setGoal(key: string, v: number) {
    goal.set(key, v)
  }
  function fade(ms: number) {
    const dur = duration(ms)
    fadeT?.stop()
    fadeT = null
    if (!dur) {
      for (const [key, v] of goal) alpha.set(key, v)
      settleGoals()
      requestDraw()
      return
    }
    from = new Map(alpha)
    fadeStart = performance.now()
    fadeMs = dur
    fadeT = d3.timer(() => {
      const p = Math.min(1, (performance.now() - fadeStart) / fadeMs)
      const e = d3.easeCubicOut(p)
      for (const [key, v] of goal) {
        const a0 = from.get(key) ?? 0
        alpha.set(key, a0 + (v - a0) * e)
      }
      requestDraw()
      if (p >= 1) {
        fadeT?.stop()
        fadeT = null
        settleGoals()
      }
    })
  }
  /** After a fade: forget fully transparent items and drop the exiting ones. */
  function settleGoals() {
    for (const [key, v] of goal) {
      if (v === 0) {
        goal.delete(key)
        alpha.delete(key)
      }
    }
    leaving = { nodes: [], edges: [] }
    // hulls and labels that faded out leave the draw lists, so a later highlight cannot bring them back
    hullGroups = hullGroups.filter((d) => goal.has('H' + d[0]))
    shownLabels = shownLabels.filter((o) => goal.has('T' + o.id))
  }

  // Role shapes as Path2D, cached by role and radius (score sizes repeat)
  const shapes = new Map<string, Path2D>()
  function shape(o: GNode) {
    const role = o.stub ? null : o.n.role
    const r = Math.round(R(o) * 4) / 4
    const key = `${role}|${r}`
    let p = shapes.get(key)
    if (!p) {
      const sym = roleSymbolPath(role, r)
      p = new Path2D()
      p.addPath(new Path2D(sym.d), sym.rotate ? new DOMMatrix().rotate(sym.rotate) : undefined)
      shapes.set(key, p)
    }
    return p
  }

  function render(animate: boolean) {
    const dur = animate ? 350 : 0
    const nodeIds = new Set(vis.nodes.map((o) => o.id))
    const edgeIds = new Set(vis.edges.map((l) => l.id))
    // Items that left the view fade out from where they are
    leaving = {
      nodes: [...new Set([...leaving.nodes, ...[...N.values()].filter((o) => !nodeIds.has(o.id) && A(o.id) > 0)])],
      edges: [...new Set([...leaving.edges, ...E.filter((l) => !edgeIds.has(l.id) && A('L' + l.id) > 0)])],
    }
    for (const o of leaving.nodes) setGoal(o.id, 0)
    for (const l of leaving.edges) setGoal('L' + l.id, 0)
    hulls()
    qt = null
    a11yList()
    opac(dur)
    labels(animate ? 300 : 0)
    flows()
    requestDraw()
  }

  function hulls() {
    const groups: Hull[] =
      V.mode === 'overview'
        ? [
            ...d3.group(
              vis.nodes.filter((o) => !o.ghost && o.n.party != null),
              (o) => o.n.party as number,
            ),
          ]
        : []
    const ids = new Set(groups.map((d) => 'H' + d[0]))
    for (const d of hullGroups) if (!ids.has('H' + d[0])) setGoal('H' + d[0], 0)
    // keep exiting hulls drawable while they fade
    hullGroups = [...groups, ...hullGroups.filter((d) => !ids.has('H' + d[0]) && A('H' + d[0]) > 0)]
  }

  const hullTitle = (d: Hull) => {
    const p = passports.get(d[0])
    return `${p ? p.label : `party ${d[0]}`} · ${d[1].length} agents${p ? ` · ${p.finished ? 'finished' : `round ${p.round} · ${p.phase}`}` : ''}`
  }
  const hullLine = d3.line().curve(d3.curveCatmullRomClosed.alpha(0.6))

  /** Link geometry: straight, or a quadratic curve when both directions exist. End point stops at the target's edge. */
  function geom(l: GLink) {
    const s = l.source
    const t = l.target
    const dx = t.x - s.x
    const dy = t.y - s.y
    const len = Math.hypot(dx, dy) || 1
    const rt = R(t) + 2
    const ux = dx / len
    const uy = dy / len
    const ex = t.x - ux * rt
    const ey = t.y - uy * rt
    if (!l.recip) return { sx: s.x, sy: s.y, ex, ey, cx: NaN, cy: NaN, dx: ux, dy: uy }
    const off = Math.min(28, len * 0.18)
    const cx = (s.x + t.x) / 2 - uy * off
    const cy = (s.y + t.y) / 2 + ux * off
    const tl = Math.hypot(ex - cx, ey - cy) || 1
    return { sx: s.x, sy: s.y, ex, ey, cx, cy, dx: (ex - cx) / tl, dy: (ey - cy) / tl }
  }

  /** Positions changed (physics tick, morph, drag): repaint on the next frame and rebuild the hit-test tree lazily. */
  function draw() {
    qt = null
    labels(0)
    requestDraw()
  }
  let frame = 0
  function requestDraw() {
    if (!frame && !destroyed) frame = requestAnimationFrame(paint)
  }

  function paint() {
    frame = 0
    const dpr = window.devicePixelRatio || 1
    const W = el.clientWidth
    const Hh = el.clientHeight
    if (canvas.width !== Math.round(W * dpr) || canvas.height !== Math.round(Hh * dpr)) {
      canvas.width = Math.round(W * dpr)
      canvas.height = Math.round(Hh * dpr)
      canvas.style.width = `${W}px`
      canvas.style.height = `${Hh}px`
    }
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.clearRect(0, 0, canvas.width, canvas.height)
    ctx.setTransform(dpr * T.k, 0, 0, dpr * T.k, dpr * T.x, dpr * T.y)
    const now = performance.now()
    const H = hoverId ? nb(hoverId) : null

    // hulls
    for (const d of hullGroups) {
      const a = A('H' + d[0])
      if (a < 0.01) continue
      const pts: [number, number][] = []
      for (const o of d[1]) {
        const r = R(o) + 14
        for (let i = 0; i < 8; i++) pts.push([o.x + r * Math.cos((i * Math.PI) / 4), o.y + r * Math.sin((i * Math.PI) / 4)])
      }
      const h = d3.polygonHull(pts)
      if (!h) continue
      const hc = V.colorBy === 'party' ? CL[d[0] % CL.length] : '#ffffff'
      ctx.beginPath()
      hullLine.context(ctx)(h)
      ctx.globalAlpha = 0.025 * a
      ctx.fillStyle = hc
      ctx.fill()
      ctx.globalAlpha = 0.08 * a
      ctx.strokeStyle = hc
      ctx.lineWidth = 1 / k
      ctx.stroke()
      ctx.globalAlpha = a
      ctx.fillStyle = '#a1a1aa'
      ctx.font = `500 ${Math.min(12, 12 / k)}px ${SANS}`
      ctx.textAlign = 'center'
      ctx.fillText(hullTitle(d), d3.mean(d[1], (o) => o.x) ?? 0, (d3.min(h, (q) => q[1]) ?? 0) - 8)
    }

    // links, batched by colour, alpha and width; running dashes drawn separately
    const arrowPx = Math.min(10, Math.max(3.5, 7 * k)) / k
    const batches = new Map<string, GLink[]>()
    const dashed: GLink[] = []
    for (const l of [...vis.edges, ...leaving.edges]) {
      const a = A('L' + l.id)
      if (a < 0.01) continue
      if (flowing.has(l)) {
        dashed.push(l)
        continue
      }
      const hot = !!H && (l.source.id === hoverId || l.target.id === hoverId)
      const key = `${hot ? color(l.source) : EDGE}|${a.toFixed(2)}|${ew(l).toFixed(2)}`
      push(batches, key, l)
    }
    const strokeLinks = (list: GLink[], col: string, a: number, w: number) => {
      ctx.globalAlpha = a
      ctx.strokeStyle = col
      ctx.lineWidth = w
      ctx.beginPath()
      const heads = new Path2D()
      for (const l of list) {
        const q = geom(l)
        ctx.moveTo(q.sx, q.sy)
        if (isNaN(q.cx)) ctx.lineTo(q.ex, q.ey)
        else ctx.quadraticCurveTo(q.cx, q.cy, q.ex, q.ey)
        // arrowhead: tip at the end point, 8x7 marker scaled to arrowPx
        const bx = q.ex - q.dx * arrowPx
        const by = q.ey - q.dy * arrowPx
        const nx = -q.dy * arrowPx * 0.44
        const ny = q.dx * arrowPx * 0.44
        heads.moveTo(q.ex, q.ey)
        heads.lineTo(bx + nx, by + ny)
        heads.lineTo(bx - nx, by - ny)
        heads.closePath()
      }
      ctx.stroke()
      ctx.fillStyle = '#b4b4be'
      ctx.fill(heads)
    }
    for (const [key, list] of batches) {
      const [col, a, w] = key.split('|')
      strokeLinks(list, col, +a, +w)
    }
    if (dashed.length) {
      ctx.setLineDash([6, 4])
      ctx.lineDashOffset = -((now / 1400) * 20) % 20
      for (const l of dashed) {
        const hot = !!H && (l.source.id === hoverId || l.target.id === hoverId)
        strokeLinks([l], hot ? color(l.source) : EDGE, A('L' + l.id), ew(l))
      }
      ctx.setLineDash([])
    }

    // nodes: plain ones batched by fill and alpha; rings, glow and focus drawn one by one on top
    const plain = new Map<string, Path2D>()
    const special: GNode[] = []
    for (const o of [...vis.nodes, ...leaving.nodes]) {
      const a = A(o.id)
      if (a < 0.01 || isNaN(o.x)) continue
      if (isMine(o) || o.n.alive === false || o.id === hoverId || o.id === V.focus) {
        special.push(o)
        continue
      }
      const key = `${color(o)}|${a.toFixed(2)}`
      let p = plain.get(key)
      if (!p) plain.set(key, (p = new Path2D()))
      p.addPath(shape(o), new DOMMatrix().translate(o.x, o.y))
    }
    ctx.lineWidth = 0.8
    ctx.strokeStyle = BG
    for (const [key, p] of plain) {
      const [col, a] = key.split('|')
      ctx.globalAlpha = +a
      ctx.fillStyle = col
      ctx.fill(p)
      ctx.stroke(p)
    }
    for (const o of special) {
      ctx.save()
      ctx.translate(o.x, o.y)
      ctx.globalAlpha = A(o.id)
      if (o.id === V.focus) {
        ctx.beginPath()
        ctx.arc(0, 0, R(o) + 6, 0, Math.PI * 2)
        ctx.globalAlpha = 0.55 * A(o.id)
        ctx.strokeStyle = '#ffffff'
        ctx.lineWidth = 1.5
        ctx.stroke()
        ctx.globalAlpha = A(o.id)
      }
      if (o.id === hoverId || o.id === V.focus) {
        ctx.shadowColor = color(o)
        ctx.shadowBlur = 6 * T.k * dpr
      }
      ctx.fillStyle = color(o)
      ctx.fill(shape(o))
      ctx.shadowBlur = 0
      // White ring: the owner's agent. Dashed: an agent that left the game (alive: false).
      ctx.strokeStyle = !lit(o) ? BG : isMine(o) ? '#ffffff' : o.n.alive === false ? '#d4d4d8' : BG
      ctx.lineWidth = isMine(o) ? 2 : o.n.alive === false ? 1.4 : 0.8
      if (o.n.alive === false) ctx.setLineDash([2.5, 2])
      ctx.stroke(shape(o))
      ctx.setLineDash([])
      ctx.restore()
    }

    // labels (already culled for overlaps)
    ctx.textAlign = 'center'
    ctx.lineJoin = 'round'
    const off = Math.min(12, 14 / k)
    for (const o of shownLabels) {
      const a = A('T' + o.id)
      if (a < 0.01) continue
      const fs = fontSize(o)
      ctx.globalAlpha = a
      ctx.font = `500 ${fs}px ${MONO}`
      ctx.lineWidth = Math.min(3, 3 / k)
      ctx.strokeStyle = BG
      ctx.strokeText(label(o), o.x, o.y + R(o) + off)
      ctx.fillStyle = o.id === V.focus ? '#ffffff' : '#b4b4bc'
      ctx.fillText(label(o), o.x, o.y + R(o) + off)
    }
    ctx.globalAlpha = 1
    if (dashed.length && !reduced) requestDraw() // keep the dashes running
  }

  // ---------- hit testing and pointer input ----------
  let qt: d3.Quadtree<GNode> | null = null
  function nodeAt(e: MouseEvent | PointerEvent): GNode | undefined {
    const [x, y] = T.invert(d3.pointer(e, svg.node()))
    qt ??= d3.quadtree<GNode>().x((o) => o.x).y((o) => o.y).addAll(vis.nodes.filter((o) => !isNaN(o.x)))
    const o = qt.find(x, y, (12 * V.nodeScale + 6) / Math.min(1, k) + 4)
    return o && Math.hypot(o.x - x, o.y - y) <= R(o) + 4 / k ? o : undefined
  }
  svg
    .on('pointermove', (e: PointerEvent) => {
      if (e.buttons) return
      const o = nodeAt(e)
      svg.style('cursor', o ? 'pointer' : 'grab')
      if ((o?.id ?? null) !== hoverId) hover(o?.id ?? null, e)
      else if (o) moveTip(e)
    })
    .on('pointerleave', () => hover(null))
    .on('click', (e: MouseEvent) => {
      const o = nodeAt(e)
      if (o) cb.onSelect?.(o.id)
      else cb.onBackground?.()
    })
  // Drag before zoom: when a node is under the pointer, drag consumes the gesture; otherwise zoom/pan gets it
  svg.call(
    d3
      .drag<SVGSVGElement, unknown, GNode>()
      .container(() => g.node()!)
      // no node under the pointer: d3-drag gets null and lets the zoom behaviour take the gesture
      .subject((e) => nodeAt(e.sourceEvent) ?? (null as unknown as GNode))
      .on('start', (e: d3.D3DragEvent<SVGSVGElement, unknown, GNode>) => {
        const o = e.subject
        pendingFit = false
        svg.style('cursor', 'grabbing')
        o.fx = o.x
        o.fy = o.y
        if (usesSim()) {
          sim.fix(o, o.x, o.y)
          sim.alphaTarget(0.25)
        }
      })
      .on('drag', (e: d3.D3DragEvent<SVGSVGElement, unknown, GNode>) => {
        const o = e.subject
        o.fx = e.x
        o.fy = e.y
        // move it under the pointer right away; the worker echoes the pinned position on its next frame
        o.x = e.x
        o.y = e.y
        if (usesSim()) sim.fix(o, e.x, e.y)
        draw()
      })
      .on('end', (e: d3.D3DragEvent<SVGSVGElement, unknown, GNode>) => {
        const o = e.subject
        svg.style('cursor', 'grab')
        if (usesSim()) sim.alphaTarget(0)
        if (!(V.mode === 'local' && o.id === V.focus && usesSim())) {
          o.fx = null
          o.fy = null
          if (usesSim()) sim.free(o)
        }
      }),
  )
  svg.call(zoom).on('dblclick.zoom', null)

  /** Keyboard and screen readers: the visible agents as buttons (top 200 by score in big overviews). */
  function a11yList() {
    const list = [...vis.nodes].filter((o) => !o.ghost).sort((a, b) => (b.n.score || 0) - (a.n.score || 0)).slice(0, 200)
    a11y
      .selectAll<HTMLLIElement, GNode>('li')
      .data(list, (o) => o.id)
      .join((en) => {
        const li = en.append('li')
        li.append('button').attr('type', 'button')
        return li
      })
      .select('button')
      .text((o) => `${label(o)}, ${roleTitle(o)}`)
      .on('focus', (_e, o) => hover(o.id))
      .on('blur', () => hover(null))
      .on('click', (_e, o) => cb.onSelect?.(o.id))
  }

  // ---------- highlight state ----------
  function nb(id: string) {
    const s = new Set([id])
    for (const l of outAdj.get(id) ?? []) if (l.vis) s.add(l.target.id)
    for (const l of inAdj.get(id) ?? []) if (l.vis) s.add(l.source.id)
    return s
  }

  function opac(ms: number) {
    const H = hoverId ? nb(hoverId) : null
    const hot = (l: GLink) => l.source.id === hoverId || l.target.id === hoverId
    for (const o of vis.nodes) setGoal(o.id, (o.ghost ? 0.07 : 1) * (H ? (H.has(o.id) ? 1 : 0.12) : 1))
    for (const l of vis.edges) {
      setGoal(
        'L' + l.id,
        l.ghost ? 0.03 : H ? (hot(l) ? 0.95 : 0.04) : V.focus && (l.source.id === V.focus || l.target.id === V.focus) ? 0.7 : 0.32,
      )
    }
    for (const d of hullGroups) if (goal.get('H' + d[0]) !== 0) setGoal('H' + d[0], H ? 0.35 : 1)
    fade(ms)
  }

  const fontSize = (o: GNode) => Math.min(o.id === V.focus ? 12 : 10, (o.id === V.focus ? 14 : 12) / k)

  // Labels: at most 12px on screen (selected: 14px); overlapping ones are dropped greedily by rank
  // (hovered > selected > score). A grid instead of pairwise checks: runs on every tick and zoom.
  function labels(ms: number) {
    const H = hoverId ? nb(hoverId) : null
    const off = Math.min(12, 14 / k)
    const few = V.mode === 'local' && vis.nodes.length <= 30
    const important = (o: GNode) => o.id === hoverId || o.id === V.focus
    const ranked = vis.nodes
      .filter((o) => !o.ghost && (important(o) || (H ? H.has(o.id) : top.has(o.id) || k >= V.labelZoom || few)))
      .sort(
        (a, b) =>
          Number(b.id === hoverId) - Number(a.id === hoverId) ||
          Number(b.id === V.focus) - Number(a.id === V.focus) ||
          (b.n.score || 0) - (a.n.score || 0),
      )
    const shown: GNode[] = []
    const cw = 80 / k
    const ch = 20 / k
    const grid = new Map<string, Box[]>()
    const hits = (a: Box, b: Box) => a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
    // Only labels inside the viewport (plus a margin) compete for space
    const [vx0, vy0] = T.invert([-100, -100])
    const [vx1, vy1] = T.invert([el.clientWidth + 100, el.clientHeight + 100])
    for (const o of ranked) {
      if (o.x < vx0 || o.x > vx1 || o.y < vy0 || o.y > vy1) continue
      const font = fontSize(o)
      const pad = 6 / k
      const w = label(o).length * font * 0.65 + pad * 2
      const box = { x: o.x - w / 2, y: o.y + R(o) + off - font - pad / 2, w, h: font + pad }
      const cells: string[] = []
      for (let i = Math.floor(box.x / cw); i <= Math.floor((box.x + box.w) / cw); i++)
        for (let j = Math.floor(box.y / ch); j <= Math.floor((box.y + box.h) / ch); j++) cells.push(`${i},${j}`)
      if (important(o) || !cells.some((c) => grid.get(c)?.some((b) => hits(box, b)))) {
        shown.push(o)
        for (const c of cells) push(grid, c, box)
      }
    }
    const now = new Set(shown.map((o) => o.id))
    for (const o of shownLabels) if (!now.has(o.id)) setGoal('T' + o.id, 0)
    for (const o of shown) setGoal('T' + o.id, 1)
    // Labels that are fading out stay in the draw list until transparent
    shownLabels = [...shown, ...shownLabels.filter((o) => !now.has(o.id) && A('T' + o.id) > 0.01)]
    if (ms) fade(ms)
    else for (const o of shownLabels) alpha.set('T' + o.id, goal.get('T' + o.id) ?? 0)
  }

  // Running dash on the hovered agent's links, the selected agent's links, or every link of a neighbourhood
  let flowing = new Set<GLink>()
  function flows() {
    flowing = new Set(
      vis.edges.filter((l) => {
        if (reduced || V.flow !== 'dash' || l.ghost) return false
        if (hoverId) return l.source.id === hoverId || l.target.id === hoverId
        if (V.mode === 'local') return true
        return !!V.focus && (l.source.id === V.focus || l.target.id === V.focus)
      }),
    )
    requestDraw()
  }

  function hover(id: string | null, ev?: MouseEvent) {
    const o = id ? N.get(id) : undefined
    if (id && !o) return
    hoverId = id
    opac(150)
    labels(150)
    flows()
    if (!o) {
      tip.style('display', 'none')
      return
    }
    const n = o.n
    const row = (a: string, b: string) =>
      `<div style="display:flex;gap:14px;justify-content:space-between;white-space:nowrap"><span style="color:#a1a1aa">${a}</span><span style="font-family:${MONO}">${b}</span></div>`
    const party = n.party != null ? passports.get(n.party) : undefined
    const kinds = [...new Set([...(outAdj.get(o.id) ?? []), ...(inAdj.get(o.id) ?? [])].flatMap((l) => l.e.kinds))]
    tip.html(
      `<div style="font:600 13px ${MONO};margin-bottom:2px">${esc(label(o))}</div>` +
        (n.model ? `<div style="color:#a1a1aa;font:12px ${MONO};margin-bottom:4px">${esc(n.model)}</div>` : '') +
        `<div style="display:flex;align-items:center;gap:6px;margin-bottom:6px">${tipIcon(o)}` +
        roleTitle(o) +
        (isMine(o) ? ' · your agent' : '') +
        (n.alive === false ? ' · left the game' : '') +
        '</div>' +
        (party ? row('party', esc(party.label)) : n.party === null ? row('party', 'waiting for a game') : '') +
        (n.cash != null ? row('cash (simulated)', formatCash(n.cash)) : '') +
        (n.influence != null ? row('influence', String(n.influence)) : '') +
        (n.in_cash != null ? row('received', `${formatCash(n.in_cash)} · from ${n.in_deg}`) : '') +
        (n.out_cash != null ? row('paid', `${formatCash(n.out_cash)} · to ${n.out_deg}`) : '') +
        (kinds.length ? row('deals', kinds.map((k) => KIND_TITLE[k]).join(', ')) : ''),
    )
    tip.style('display', 'block')
    if (ev) moveTip(ev)
    else {
      const t = d3.zoomTransform(svg.node()!)
      placeTip(t.applyX(o.x), t.applyY(o.y))
    }
  }
  function tipIcon(o: GNode) {
    const sym = roleSymbolPath(o.stub ? null : o.n.role, 5.6)
    const rot = sym.rotate ? ` transform="rotate(${sym.rotate})"` : ''
    return `<svg viewBox="-8 -8 16 16" width="12" height="12" style="flex:none"><path d="${sym.d}"${rot} style="fill:${o.n.role && !o.stub ? ROLE_COLOR[o.n.role] : STUB}"/></svg>`
  }
  function moveTip(e: MouseEvent) {
    const [x, y] = d3.pointer(e, el)
    placeTip(x, y)
  }
  function placeTip(x: number, y: number) {
    const box = tip.node()!.getBoundingClientRect()
    tip
      .style('left', `${Math.max(8, Math.min(x + 14, el.clientWidth - box.width - 8))}px`)
      .style('top', `${Math.max(8, Math.min(y + 14, el.clientHeight - box.height - 8))}px`)
  }

  // ---------- camera ----------
  // During a morph the camera aims at final positions, not intermediate ones
  const px = (o: GNode) => ((layoutT || !usesSim()) && !isNaN(o.tx) ? o.tx : o.x)
  const py = (o: GNode) => ((layoutT || !usesSim()) && !isNaN(o.ty) ? o.ty : o.y)

  function fit(ms = 600) {
    const ns = vis.nodes.filter((o) => !isNaN(px(o)))
    if (!ns.length) return
    const W = el.clientWidth || 600
    const H = el.clientHeight || 500
    const x0 = d3.min(ns, px)! - 40
    const x1 = d3.max(ns, px)! + 40
    const y0 = d3.min(ns, py)! - 50
    const y1 = d3.max(ns, py)! + 40
    const Hh = H - 70
    const s = Math.min(2.2, 0.94 * Math.min(W / (x1 - x0), Hh / (y1 - y0)))
    cam(d3.zoomIdentity.translate(W / 2 - (s * (x0 + x1)) / 2, Hh / 2 - (s * (y0 + y1)) / 2).scale(s), ms)
  }

  function cam(t1: d3.ZoomTransform, ms: number, done?: () => void) {
    // any new camera move (fly to a node, restore) cancels the pending fit; fit sets it again after the call
    pendingFit = false
    const dur = duration(ms)
    camT?.stop()
    camT = null
    if (!dur) {
      svg.call(zoom.transform, t1)
      done?.()
      return
    }
    const t0 = d3.zoomTransform(svg.node()!)
    const ip = d3.interpolate([t0.x, t0.y, t0.k], [t1.x, t1.y, t1.k])
    const st = performance.now()
    const tm = d3.timer(() => {
      const p = Math.min(1, (performance.now() - st) / dur)
      const v = ip(d3.easeCubicInOut(p))
      svg.call(zoom.transform, d3.zoomIdentity.translate(v[0], v[1]).scale(v[2]))
      if (p >= 1) {
        tm.stop()
        camT = null
        done?.()
      }
    })
    camT = tm
  }

  function flyTo(id: string, scale = Math.max(k, 1.8)) {
    const o = N.get(id)
    if (!o || isNaN(px(o))) return
    const W = el.clientWidth || 600
    const H = el.clientHeight || 500
    cam(d3.zoomIdentity.translate(W / 2 - scale * px(o), H / 2 - scale * py(o)).scale(scale), 750, () => pulse(o))
  }

  function pulse(o: GNode) {
    if (reduced || destroyed) return
    const c = gR.append('circle').attr('cx', o.x).attr('cy', o.y).attr('r', R(o)).attr('fill', 'none').attr('stroke', '#ffffff').attr('stroke-width', 2)
    const rep = (i: number) => {
      c.attr('r', R(o))
        .attr('stroke-opacity', 0.9)
        .transition()
        .duration(900)
        .ease(d3.easeCubicOut)
        .attr('r', R(o) + 34)
        .attr('stroke-opacity', 0)
        .on('end', () => (i < 2 ? rep(i + 1) : c.remove()))
    }
    rep(0)
  }

  // In the overview the selected node and neighbourhood settings do not affect the layout: a click does not shake the network
  const layoutKey = (v: AgentGraphViewState) =>
    (v.mode === 'overview'
      ? ['overview', v.roles, v.parties, v.showIsolated, v.hideDead, v.mine]
      : [v.mode, v.focus, v.depth, v.dirIn, v.dirOut, v.between, v.layout, v.roles, v.parties, v.showIsolated, v.hideDead, v.mine]
    ).join('|')
  const forceKey = (v: AgentGraphViewState) => [v.charge, v.linkDist, v.clusterPull, v.nodeScale].join('|')
  const camKey = (v: AgentGraphViewState) => (v.mode === 'overview' ? 'overview' : `local|${v.layout}`)

  function setView(nv: AgentGraphViewState, opts?: { fit?: boolean }) {
    if (destroyed) return
    const prev = V
    const modeChanged = !first && prev.mode !== nv.mode
    if (modeChanged) {
      // Leaving a mode: remember its camera, and for the overview also positions with unfinished physics
      cams.set(camKey(prev), { t: d3.zoomTransform(svg.node()!), focus: prev.focus })
      if (prev.mode === 'overview') {
        for (const o of vis.nodes) if (!isNaN(px(o))) ovPos.set(o.id, { x: px(o), y: py(o) })
        ovAlpha = simMode === 'overview' && sim.running() && sim.alpha() >= ALPHA_MIN ? sim.alpha() : 0
      }
    }
    V = { ...V, ...nv }
    compute()
    const relayout = first || dirty || layoutKey(prev) !== layoutKey(V) || forceKey(prev) !== forceKey(V)
    dirty = false
    // Layout first, then drawing: otherwise new nodes are drawn at NaN coordinates
    const settling = relayout ? place(!first, first || modeChanged) : false
    render(!first)
    if (relayout && (first || opts?.fit)) {
      const saved = modeChanged ? cams.get(camKey(V)) : undefined
      if (saved && saved.focus === V.focus) cam(saved.t, 600)
      else if (saved && V.mode === 'overview' && V.focus && N.has(V.focus)) flyTo(V.focus, saved.t.k)
      else {
        fit(first ? 0 : 600)
        // with reduced motion the layout is already computed synchronously, no simulation runs
        if (settling) pendingFit = true
      }
    }
    if (first) {
      for (const key of goal.keys()) if (!key.startsWith('T')) alpha.set(key, 0)
      opac(700)
      first = false
    }
  }

  // Canvas resized (toolbar, window): shift the camera so the view centre stays put, no refit
  let size = { w: el.clientWidth, h: el.clientHeight }
  const resize = new ResizeObserver(() => {
    const w = el.clientWidth
    const h = el.clientHeight
    const dw = w - size.w
    const dh = h - size.h
    size = { w, h }
    requestDraw()
    if (first || destroyed || (!dw && !dh)) return
    const t = d3.zoomTransform(svg.node()!)
    svg.call(zoom.transform, d3.zoomIdentity.translate(t.x + dw / 2, t.y + dh / 2).scale(t.k))
  })
  resize.observe(el)

  return {
    setData,
    setView,
    flyTo,
    hover: (id) => hover(id),
    fit: () => fit(600),
    zoomBy: (factor) => {
      svg.transition().duration(duration(250)).call(zoom.scaleBy, factor)
    },
    counts: () => ({ n: vis.nodes.filter((o) => !o.ghost).length, e: vis.edges.filter((l) => !l.ghost).length }),
    destroy: () => {
      destroyed = true
      stopPrecompute()
      resize.disconnect()
      camT?.stop()
      layoutT?.stop()
      fadeT?.stop()
      cancelAnimationFrame(frame)
      sim.terminate()
      ovPhys.terminate()
      el.removeEventListener('wheel', keepGesture)
      el.removeEventListener('gesturestart', keepGesture)
      el.removeEventListener('gesturechange', keepGesture)
      root.selectAll('*').interrupt().remove()
    },
  }
}

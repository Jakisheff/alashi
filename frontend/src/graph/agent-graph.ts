import * as d3 from 'd3'
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
let instance = 0

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

// ponytail: SVG + per-element transitions hold ~2.2K nodes / ~3.1K edges in HackAlem's overview;
// if it starts to lag (hover, timelapse, drag), move edges and nodes to Canvas and keep labels and halos in SVG.
export function createAgentGraph(el: HTMLElement, cb: AgentGraphCallbacks = {}): AgentGraph {
  const uid = `ag-${++instance}`
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
  const svg = root
    .append('svg')
    .attr('width', '100%')
    .attr('height', '100%')
    .style('display', 'block')
    .style('cursor', 'grab')
    .attr('aria-label', 'Interactive agent interaction graph')
    .attr('role', 'group')
  svg.append('style').text(
    '@keyframes daiDash{to{stroke-dashoffset:-20}}' +
      '.dai-flow{stroke-dasharray:6 4;animation:daiDash 1.4s linear infinite}' +
      '@keyframes daiSeed{0%,100%{stroke-opacity:1}50%{stroke-opacity:.35}}' +
      '.dai-seed{animation:daiSeed 1.8s ease-in-out 1}' +
      '.n:focus{outline:none}.n:focus .c{stroke:#fff;stroke-width:3}' +
      '@media(prefers-reduced-motion:reduce){.dai-flow,.dai-seed{animation:none}}',
  )
  const defs = svg.append('defs')
  const glow = defs.append('filter').attr('id', `${uid}-glow`).attr('x', '-100%').attr('y', '-100%').attr('width', '300%').attr('height', '300%')
  glow.append('feGaussianBlur').attr('stdDeviation', 3).attr('result', 'blur')
  const merge = glow.append('feMerge')
  merge.append('feMergeNode').attr('in', 'blur')
  merge.append('feMergeNode').attr('in', 'SourceGraphic')
  const arrow = defs
    .append('marker')
    .attr('id', `${uid}-arr`)
    .attr('viewBox', '0 -4 8 8')
    .attr('refX', 7)
    .attr('refY', 0)
    .attr('markerWidth', 7)
    .attr('markerHeight', 7)
    .attr('markerUnits', 'userSpaceOnUse')
    .attr('orient', 'auto')
  arrow.append('path').attr('d', 'M0,-3.5L8,0L0,3.5').attr('fill', '#b4b4be')
  /** Arrowhead stays 3.5-10 px on screen at any zoom: in graph units its size is inverse to the scale */
  const arrowSize = () => {
    const px = Math.min(10, Math.max(3.5, 7 * k))
    arrow.attr('markerWidth', px / k).attr('markerHeight', px / k)
  }
  const g = svg.append('g')
  const gH = g.append('g')
  const gL = g.append('g')
  const gN = g.append('g')
  const gT = g.append('g')
  const gR = g.append('g')
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
  type ONode = d3.SimulationNodeDatum & { id: string; o: GNode }
  const ovPos = new Map<string, { x: number; y: number }>()
  let ovAlpha = 0
  /** Which mode the main simulation holds: its alpha is unfinished physics of that mode only */
  let simMode: GraphView['mode'] | null = null
  let ovSim: d3.Simulation<ONode, undefined> | null = null
  let ovT: d3.Timer | null = null
  const cams = new Map<string, { t: d3.ZoomTransform; focus: string | null }>()

  let linkSel = gL.selectAll<SVGPathElement, GLink>('path')
  let nodeSel = gN.selectAll<SVGGElement, GNode>('g.n')
  let textSel = gT.selectAll<SVGTextElement, GNode>('text')
  let hullSel = gH.selectAll<SVGGElement, Hull>('g.h')

  const zoom = d3
    .zoom<SVGSVGElement, unknown>()
    .scaleExtent([0.15, 6])
    .on('zoom', (e: d3.D3ZoomEvent<SVGSVGElement, unknown>) => {
      g.attr('transform', e.transform.toString())
      k = e.transform.k
      // The user moves the camera (wheel, drag): the pending 'fit after settling' is dropped,
      // otherwise a few seconds later the camera would jump away from what they were looking at
      if (e.sourceEvent) pendingFit = false
      arrowSize()
      labels(0)
    })
  svg.call(zoom).on('dblclick.zoom', null)

  // On large graphs a frame is bound by SVG redraw (~55 ms at 2.2K nodes), not physics (~10 ms):
  // up to 3 ticks per frame, so the layout settles in ~4 s instead of ~11.
  const sim = d3
    .forceSimulation<GNode>()
    .alphaMin(0.02)
    .stop()
    .on('tick', () => {
      const extra = Math.min(2, Math.floor(sim.nodes().length / 1000))
      if (extra) sim.tick(extra)
      draw()
    })
    .on('end', () => {
      if (pendingFit) {
        pendingFit = false
        fit(500)
      }
    })

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

  /**
   * Party centre of a node in the overview; in a neighbourhood: the origin.
   * One simulation serves all modes: sim.nodes(new) reinitialises old forces whose accessor remembers the centres
   * of the old node set. So a party missing from the map (turned on by a filter) gets the default centre instead of failing.
   */
  function centers(nodes: GNode[]) {
    const C = V.mode === 'overview' ? clusterCenters(nodes) : null
    return (o: GNode) => (C && C.get(o.n.party ?? -1)) || ORIGIN
  }

  function forces<T extends d3.SimulationNodeDatum & { id: string }>(
    s: d3.Simulation<T, undefined>,
    links: { source: string | T; target: string | T }[],
    center: (d: T) => { x: number; y: number },
    radius: (d: T) => number,
    pull: number,
  ) {
    s.force('charge', d3.forceManyBody<T>().strength(-V.charge).distanceMax(400))
      .force(
        'link',
        d3
          .forceLink<T, { source: string | T; target: string | T }>(links)
          .id((d) => d.id)
          .distance(V.linkDist)
          .strength(0.6),
      )
      .force(
        'collide',
        d3.forceCollide<T>((d) => radius(d) + 2),
      )
      .force('cx', d3.forceX<T>((d) => center(d).x).strength(pull))
      .force('cy', d3.forceY<T>((d) => center(d).y).strength(pull))
  }

  /** Background overview layout: ~10 ms of physics per frame, result in ovPos. */
  function precomputeOverview() {
    ovT?.stop()
    ovSim?.stop()
    const all = [...N.values()]
    const C = clusterCenters(all)
    const center = (d: ONode) => C.get(d.o.n.party ?? -1) ?? ORIGIN
    const nodes: ONode[] = all.map((o) => {
      const c = C.get(o.n.party ?? -1) ?? ORIGIN
      const p = ovPos.get(o.id) ?? { x: c.x + (Math.random() - 0.5) * 60, y: c.y + (Math.random() - 0.5) * 60 }
      return { id: o.id, o, x: p.x, y: p.y }
    })
    const s = d3.forceSimulation(nodes).alphaMin(0.02).stop()
    forces(
      s,
      E.map((l) => ({ source: l.source.id, target: l.target.id })),
      center,
      (d) => R(d.o),
      V.clusterPull,
    )
    ovSim = s
    const save = () => {
      for (const d of s.nodes()) ovPos.set(d.id, { x: d.x!, y: d.y! })
    }
    ovT = d3.timer(() => {
      const t0 = performance.now()
      while (performance.now() - t0 < 10 && s.alpha() >= s.alphaMin()) s.tick()
      if (s.alpha() < s.alphaMin()) {
        save()
        stopPrecompute()
      }
    })
  }
  function stopPrecompute() {
    ovT?.stop()
    ovSim?.stop()
    ovT = null
    ovSim = null
  }
  /** Take the background layout (even unfinished). Returns the alpha to continue from. */
  function takeOverview() {
    let alpha = ovAlpha
    ovAlpha = 0
    if (ovSim) {
      for (const d of ovSim.nodes()) ovPos.set(d.id, { x: d.x!, y: d.y! })
      alpha = Math.max(alpha, ovSim.alpha())
      stopPrecompute()
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
    const configure = () => {
      for (const o of nodes) {
        o.fx = V.mode === 'local' && o.id === V.focus ? 0 : null
        o.fy = o.fx
      }
      sim.nodes(nodes)
      forces(sim, edges, center, R, pull)
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
        // Background did not finish: run the start synchronously, the rest settles on screen
        for (const o of nodes) Object.assign(o, ovPos.get(o.id))
        configure()
        sim.alpha(alpha)
        for (let i = 0; i < (reduced ? 240 : 55) && sim.alpha() > 0.45; i++) sim.tick()
        draw()
        if (!reduced) sim.restart()
        return !reduced
      }
      morph(nodes, (o) => ovPos.get(o.id)!, animate, () => {
        configure()
        if (alpha >= sim.alphaMin() && !reduced) sim.alpha(alpha).restart()
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
    configure()
    if (reduced) {
      sim.alpha(0.5)
      for (let i = 0; i < 180; i++) sim.tick()
      draw()
      return false
    }
    sim.alpha(animate ? 0.5 : 0.3).restart()
    return true
  }

  // ---------- drawing ----------
  function render(animate: boolean) {
    const dur = duration(animate ? 350 : 0)
    linkSel = gL
      .selectAll<SVGPathElement, GLink>('path')
      .data(vis.edges, (l) => l.id)
      .join(
        (en) => en.append('path').attr('fill', 'none').attr('opacity', 0).style('stroke', EDGE),
        (up) => up,
        (ex) => ex.transition().duration(dur).attr('opacity', 0).remove(),
      )
      .attr('stroke-width', ew)
      // Arrowheads always: direction of the deal is visible when paused and on screenshots; the dash is an extra
      .attr('marker-end', `url(#${uid}-arr)`)
    nodeSel = gN
      .selectAll<SVGGElement, GNode>('g.n')
      .data(vis.nodes, (o) => o.id)
      .join(
        (en) => {
          const s = en.append('g').attr('class', 'n').style('cursor', 'pointer').attr('opacity', 0)
          s.append('path').attr('class', 'c')
          return s
        },
        (up) => up,
        (ex) => ex.transition().duration(dur).attr('opacity', 0).remove(),
      )
    nodeSel
      .attr('tabindex', 0)
      .attr('role', 'button')
      .attr('aria-label', (o) => `${label(o)}, ${roleTitle(o)}`)
      .on('keydown', (e: KeyboardEvent, o) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          cb.onSelect?.(o.id)
        }
      })
      .on('focus', (_e, o) => hover(o.id))
      .on('blur', () => hover(null))
    // Node shape = role (★ ◆ ▶ ▼ ● ■), area equal to a circle of radius R
    nodeSel
      .select<SVGPathElement>('path.c')
      .each(function (o) {
        const sym = roleSymbolPath(o.stub ? null : o.n.role, R(o))
        this.setAttribute('d', sym.d)
        if (sym.rotate) this.setAttribute('transform', `rotate(${sym.rotate})`)
        else this.removeAttribute('transform')
      })
      .style('fill', color)
      // White ring: the owner's agent. Dashed: an agent that left the game (alive: false).
      .attr('stroke', (o) => (!lit(o) ? BG : isMine(o) ? '#ffffff' : o.n.alive === false ? '#d4d4d8' : BG))
      .attr('stroke-width', (o) => (isMine(o) ? 2 : o.n.alive === false ? 1.4 : 0.8))
      .attr('stroke-dasharray', (o) => (o.n.alive === false ? '2.5 2' : null))
      .attr('class', (o) => 'c' + (isMine(o) ? ' dai-seed' : ''))
    nodeSel.selectAll('circle.f').remove()
    nodeSel
      .filter((o) => o.id === V.focus)
      .insert('circle', 'path.c')
      .attr('class', 'f')
      .attr('r', (o) => R(o) + 6)
      .attr('fill', 'none')
      .attr('stroke', '#ffffff')
      .attr('stroke-opacity', 0.55)
      .attr('stroke-width', 1.5)
    nodeSel
      .on('mouseenter', (e: MouseEvent, o) => hover(o.id, e))
      .on('mousemove', (e: MouseEvent) => moveTip(e))
      .on('mouseleave', () => hover(null))
      .on('click', (e: MouseEvent, o) => {
        e.stopPropagation()
        cb.onSelect?.(o.id)
      })
    nodeSel.call(
      d3
        .drag<SVGGElement, GNode>()
        .on('start', (_e, o) => {
          pendingFit = false
          svg.style('cursor', 'grabbing')
          if (usesSim()) sim.alphaTarget(0.25).restart()
          o.fx = o.x
          o.fy = o.y
        })
        .on('drag', (e: d3.D3DragEvent<SVGGElement, GNode, GNode>, o) => {
          o.fx = e.x
          o.fy = e.y
          if (!usesSim()) {
            o.x = e.x
            o.y = e.y
            draw()
          }
        })
        .on('end', (_e, o) => {
          svg.style('cursor', 'grab')
          if (usesSim()) sim.alphaTarget(0)
          if (!(V.mode === 'local' && o.id === V.focus && usesSim())) {
            o.fx = null
            o.fy = null
          }
        }),
    )
    textSel = gT
      .selectAll<SVGTextElement, GNode>('text')
      .data(vis.nodes, (o) => o.id)
      .join(
        (en) =>
          en
            .append('text')
            .attr('opacity', 0)
            .attr('text-anchor', 'middle')
            .style('font-family', MONO)
            .attr('font-weight', 500)
            .style('pointer-events', 'none')
            .style('paint-order', 'stroke')
            .attr('stroke', BG)
            .attr('stroke-width', 3),
        (up) => up,
        (ex) => ex.remove(),
      )
      .text(label)
      .attr('fill', (o) => (o.id === V.focus ? '#ffffff' : '#b4b4bc'))
    hulls(animate)
    opac(animate ? 400 : 0)
    labels(animate ? 300 : 0)
    flows()
    draw()
  }

  function hulls(animate: boolean) {
    const groups: Hull[] =
      V.mode === 'overview'
        ? [
            ...d3.group(
              vis.nodes.filter((o) => !o.ghost && o.n.party != null),
              (o) => o.n.party as number,
            ),
          ]
        : []
    hullSel = gH
      .selectAll<SVGGElement, Hull>('g.h')
      .data(groups, (d) => d[0])
      .join(
        (en) => {
          const s = en.append('g').attr('class', 'h').attr('opacity', 0)
          s.append('path')
          s.append('text')
            .attr('text-anchor', 'middle')
            .style('font-family', SANS)
            .attr('font-size', 12)
            .attr('font-weight', 500)
            .attr('fill', '#a1a1aa')
          return s
        },
        (up) => up,
        (ex) => ex.transition().duration(duration(300)).attr('opacity', 0).remove(),
      )
    hullSel
      .transition()
      .duration(duration(animate ? 400 : 0))
      .attr('opacity', 1)
    const hc = (d: Hull) => (V.colorBy === 'party' ? CL[d[0] % CL.length] : '#ffffff')
    hullSel.select('path').attr('fill', hc).attr('fill-opacity', 0.025).attr('stroke', hc).attr('stroke-opacity', 0.08)
    hullSel.select('text').text((d) => {
      const p = passports.get(d[0])
      return `${p ? p.label : `party ${d[0]}`} · ${d[1].length} agents${p ? ` · ${p.finished ? 'finished' : `round ${p.round} · ${p.phase}`}` : ''}`
    })
  }

  const hullLine = d3.line().curve(d3.curveCatmullRomClosed.alpha(0.6))
  function drawHulls() {
    hullSel.each(function (d) {
      const pts: [number, number][] = []
      for (const o of d[1]) {
        const r = R(o) + 14
        for (let a = 0; a < 8; a++) pts.push([o.x + r * Math.cos((a * Math.PI) / 4), o.y + r * Math.sin((a * Math.PI) / 4)])
      }
      const h = d3.polygonHull(pts)
      if (!h) return
      const s = d3.select(this)
      s.select('path').attr('d', hullLine(h))
      s.select('text')
        .attr('x', d3.mean(d[1], (o) => o.x) ?? 0)
        .attr('y', (d3.min(h, (p) => p[1]) ?? 0) - 8)
    })
  }

  function path(l: GLink) {
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
    if (!l.recip) return `M${s.x},${s.y}L${ex},${ey}`
    const off = Math.min(28, len * 0.18)
    const mx = (s.x + t.x) / 2 - uy * off
    const my = (s.y + t.y) / 2 + ux * off
    return `M${s.x},${s.y}Q${mx},${my} ${ex},${ey}`
  }

  function draw() {
    linkSel.attr('d', path)
    nodeSel.attr('transform', (o) => `translate(${o.x},${o.y})`)
    drawHulls()
    labels(0)
  }

  // ---------- highlight state ----------
  function nb(id: string) {
    const s = new Set([id])
    for (const l of outAdj.get(id) ?? []) if (l.vis) s.add(l.target.id)
    for (const l of inAdj.get(id) ?? []) if (l.vis) s.add(l.source.id)
    return s
  }

  function opac(ms: number) {
    const dur = duration(ms)
    nodeSel.select('path.c').attr('filter', (o) => (o.id === hoverId || o.id === V.focus ? `url(#${uid}-glow)` : null))
    const H = hoverId ? nb(hoverId) : null
    const hot = (l: GLink) => l.source.id === hoverId || l.target.id === hoverId
    nodeSel
      .transition()
      .duration(dur)
      .attr('opacity', (o) => (o.ghost ? 0.07 : 1) * (H ? (H.has(o.id) ? 1 : 0.12) : 1))
    linkSel
      .transition()
      .duration(dur)
      // opacity, not stroke-opacity: the marker arrowhead fades too
      .attr('opacity', (l) => {
        if (l.ghost) return 0.03
        if (H) return hot(l) ? 0.95 : 0.04
        if (V.focus && (l.source.id === V.focus || l.target.id === V.focus)) return 0.7
        return 0.32
      })
      // style, not attr: works for any CSS colour value
      .style('stroke', (l) => (H && hot(l) ? color(l.source) : EDGE))
    hullSel
      .transition()
      .duration(dur)
      .attr('opacity', H ? 0.35 : 1)
  }

  // Labels: at most 12px on screen (selected: 14px); overlapping ones are dropped greedily by rank
  // (hovered > selected > score). A grid instead of pairwise checks: runs on every tick and zoom.
  function labels(ms: number) {
    const dur = duration(ms)
    const H = hoverId ? nb(hoverId) : null
    const fontSize = (o: GNode) => Math.min(o.id === V.focus ? 12 : 10, (o.id === V.focus ? 14 : 12) / k)
    const off = Math.min(12, 14 / k)
    hullSel.select('text').attr('font-size', Math.min(12, 12 / k))
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
    const shown = new Set<string>()
    const cw = 80 / k
    const ch = 20 / k
    const grid = new Map<string, Box[]>()
    const hits = (a: Box, b: Box) => a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
    for (const o of ranked) {
      const font = fontSize(o)
      const pad = 6 / k
      const w = label(o).length * font * 0.65 + pad * 2
      const box = { x: o.x - w / 2, y: o.y + R(o) + off - font - pad / 2, w, h: font + pad }
      const cells: string[] = []
      for (let i = Math.floor(box.x / cw); i <= Math.floor((box.x + box.w) / cw); i++)
        for (let j = Math.floor(box.y / ch); j <= Math.floor((box.y + box.h) / ch); j++) cells.push(`${i},${j}`)
      if (important(o) || !cells.some((c) => grid.get(c)?.some((b) => hits(box, b)))) {
        shown.add(o.id)
        for (const c of cells) push(grid, c, box)
      }
    }
    // Move and scale only visible labels: re-laying thousands of hidden <text> per tick costs ~20 ms
    textSel
      .filter((o) => shown.has(o.id))
      .attr('x', (o) => o.x)
      .attr('y', (o) => o.y + R(o) + off)
      .attr('font-size', fontSize)
      .attr('stroke-width', Math.min(3, 3 / k))
    const op = (o: GNode) => (shown.has(o.id) ? 1 : 0)
    if (dur) textSel.interrupt().transition().duration(dur).attr('opacity', op)
    else textSel.interrupt().attr('opacity', op)
  }

  function flows() {
    linkSel.classed('dai-flow', (l) => {
      if (reduced || V.flow !== 'dash' || l.ghost) return false
      if (hoverId) return l.source.id === hoverId || l.target.id === hoverId
      if (V.mode === 'local') return true
      return !!V.focus && (l.source.id === V.focus || l.target.id === V.focus)
    })
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
        ovAlpha = simMode === 'overview' && sim.alpha() >= sim.alphaMin() ? sim.alpha() : 0
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
      nodeSel.attr('opacity', 0)
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
    if (first || destroyed || (!dw && !dh)) return
    const t = d3.zoomTransform(svg.node()!)
    svg.call(zoom.transform, d3.zoomIdentity.translate(t.x + dw / 2, t.y + dh / 2).scale(t.k))
  })
  resize.observe(el)
  svg.on('click', () => cb.onBackground?.())

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
      sim.stop()
      root.selectAll('*').interrupt().remove()
    },
  }
}

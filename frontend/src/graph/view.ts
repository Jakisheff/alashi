import { create } from 'zustand'
import type { AgentRole } from './types'

/** What the legend highlights: a role or the watched agent. Other nodes go grey but keep their place. */
export type Highlight = AgentRole | 'mine'

/** Graph view settings, changed by the toolbar and the settings panel, read by the D3 engine (HackAlem graph-view). */
export type GraphView = {
  highlight: Highlight | null
  mode: 'local' | 'overview'
  /** Neighbourhood of the selected agent: hops 1-4 */
  depth: number
  dirIn: boolean
  dirOut: boolean
  /** Show links between neighbours, not only the tree from the selected agent */
  between: boolean
  layout: 'layers' | 'force'
  colorBy: 'role' | 'party'
  /** null: all roles */
  roles: AgentRole[] | null
  /** null: all parties */
  parties: number[] | null
  showIsolated: boolean
  hideDead: boolean
  flow: 'dash' | 'arrows'
  labelZoom: number
  nodeScale: number
  edgeScale: number
  charge: number
  linkDist: number
  clusterPull: number
}

const LAYOUT_KEY = 'alashi.graph.layout'
function savedLayout(): GraphView['layout'] {
  try {
    return localStorage.getItem(LAYOUT_KEY) === 'layers' ? 'layers' : 'force'
  } catch {
    return 'force'
  }
}

export const DEFAULT_VIEW: GraphView = {
  highlight: null,
  mode: 'overview',
  depth: 2,
  dirIn: true,
  dirOut: true,
  between: true,
  layout: savedLayout(),
  colorBy: 'role',
  roles: null,
  parties: null,
  showIsolated: true,
  hideDead: false,
  // Arrows are always drawn; 'dash' adds a running dash on selected links. Reduced motion: arrows only.
  flow: window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'arrows' : 'dash',
  labelZoom: 1.6,
  nodeScale: 1,
  edgeScale: 1,
  charge: 60,
  linkDist: 40,
  clusterPull: 0.2,
}

type GraphViewState = {
  view: GraphView
  settingsOpen: boolean
  /** Timelapse: links up to this round are shown; null: off */
  play: number | null
  playing: boolean
  /** Round range picked on the histogram; null: whole game */
  range: [number, number] | null
  setView: (patch: Partial<GraphView>) => void
  toggleSettings: () => void
  setPlay: (play: number | null, playing?: boolean) => void
  setRange: (range: [number, number] | null) => void
  resetTimeline: () => void
}

export const useGraphView = create<GraphViewState>()((set) => ({
  view: DEFAULT_VIEW,
  settingsOpen: false,
  play: null,
  playing: false,
  range: null,
  setView: (patch) => {
    if (patch.layout) {
      try {
        localStorage.setItem(LAYOUT_KEY, patch.layout)
      } catch {
        // private mode or blocked storage: just do not remember
      }
    }
    set((s) => ({ view: { ...s.view, ...patch } }))
  },
  toggleSettings: () => set((s) => ({ settingsOpen: !s.settingsOpen })),
  setPlay: (play, playing = false) => set({ play, playing, range: null }),
  setRange: (range) => set({ range, play: null, playing: false }),
  resetTimeline: () => set({ play: null, playing: false, range: null }),
}))

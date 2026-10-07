import { Link } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { createAgentGraph, type AgentGraph, type AgentGraphViewState } from './agent-graph'
import { mockGraph } from './mock'
import { KIND_TITLE, ROLE_TITLE, formatCash, ui } from './roles'
import { GraphSettings, RoleIcon, Segmented } from './settings'
import { GraphTimeline } from './timeline'
import { useGraphView } from './view'

// Agent network page (port of HackAlem graph-panel.tsx). Mock data until the backend serves agent -> agent
// interactions; the page says so on screen. Route /graph: ?focus=<agent_record_id> selects an agent,
// ?agent=<id> rings "your agent" (routes/graph.tsx validates both).

const toolBtn = 'h-[30px] cursor-pointer whitespace-nowrap rounded-lg border px-2.5 text-xs font-medium'

type Props = { focus: string | null; mine: string | null; onFocus: (id: string | null) => void; scale?: 'small' | 'large' }

export function GraphPage({ focus: wanted, mine, onFocus, scale = 'small' }: Props) {
  const data = useMemo(() => mockGraph(7, scale), [scale])
  // An id that is not in the data (stale link) selects nothing
  const focus = wanted && data.agents.some((a) => a.id === wanted) ? wanted : null
  // The engine keeps its callback from mount: read the latest onFocus through a ref
  const onFocusRef = useRef(onFocus)
  useEffect(() => {
    onFocusRef.current = onFocus
  }, [onFocus])
  const setFocus = useCallback((id: string | null) => onFocusRef.current(id), [])

  const view = useGraphView((s) => s.view)
  const setView = useGraphView((s) => s.setView)
  const settingsOpen = useGraphView((s) => s.settingsOpen)
  const toggleSettings = useGraphView((s) => s.toggleSettings)
  const play = useGraphView((s) => s.play)
  const range = useGraphView((s) => s.range)

  // Picking an agent opens its neighbourhood; the mode can still be switched by hand afterwards.
  const prevFocus = useRef<string | null>(null)
  useEffect(() => {
    if (focus === prevFocus.current) return
    prevFocus.current = focus
    if (focus) setView({ mode: 'local' })
  }, [focus, setView])

  // One engine per mounted container; React 19 ref cleanup destroys it on unmount.
  const [graph, setGraph] = useState<AgentGraph | null>(null)
  const prevView = useRef<AgentGraphViewState | null>(null)
  const mount = useCallback(
    (el: HTMLDivElement) => {
      const g = createAgentGraph(el, { onSelect: (id) => setFocus(id), onBackground: () => undefined })
      prevView.current = null
      setGraph(g)
      return () => {
        g.destroy()
        setGraph(null)
      }
    },
    [setFocus],
  )

  useEffect(() => {
    if (graph) graph.setData(data)
  }, [graph, data])

  const [counts, setCounts] = useState<{ n: number; e: number } | null>(null)
  useEffect(() => {
    if (!graph) return
    if (!focus && view.mode === 'local' && !prevView.current) return
    const next: AgentGraphViewState = { ...view, focus, mine, play, range }
    const prev = prevView.current
    prevView.current = next
    const modeChanged = prev?.mode !== next.mode
    const focusChanged = prev?.focus !== next.focus
    graph.setView(next, { fit: (next.mode === 'local' && (focusChanged || modeChanged)) || (next.mode === 'overview' && modeChanged) })
    if (next.mode === 'overview' && focusChanged && !modeChanged && focus) graph.flyTo(focus)
    const c = graph.counts()
    setCounts((p) => (p && p.n === c.n && p.e === c.e ? p : c))
  }, [graph, view, focus, mine, play, range])

  const isLocal = view.mode === 'local'
  const selected = focus ? data.agents.find((a) => a.id === focus) : undefined
  const party = selected?.party != null ? data.parties.find((p) => p.id === selected.party) : undefined
  const deals = useMemo(() => {
    if (!focus) return []
    const name = new Map(data.agents.map((a) => [a.id, a.name]))
    return data.edges
      .filter((e) => e.src === focus || e.dst === focus)
      .map((e) => ({ out: e.src === focus, who: name.get(e.src === focus ? e.dst : e.src) ?? '?', e }))
      .sort((a, b) => b.e.sum + b.e.count - (a.e.sum + a.e.count))
      .slice(0, 6)
  }, [data, focus])

  const dirChips: [string, boolean, () => void][] = [
    ['Who dealt with it', view.dirIn, () => setView({ dirIn: !view.dirIn })],
    ['Who it dealt with', view.dirOut, () => setView({ dirOut: !view.dirOut })],
    ['Links between neighbours', view.between, () => setView({ between: !view.between })],
  ]

  return (
    <main className="relative flex h-dvh min-h-0 min-w-0 flex-col bg-[#17171c] text-[#ececf0]">
      <header className="flex flex-none flex-wrap items-center gap-x-3 gap-y-1 border-b border-[#2e2e36] px-3 py-2">
        <Link to="/" className="font-semibold">
          alashi
        </Link>
        <h1 className="text-sm font-semibold">Agent network</h1>
        <span className="rounded-full bg-[#ffd86b] px-2 py-0.5 text-[11px] font-semibold tracking-wide text-[#17171c] uppercase">mock data</span>
        <span className={`text-xs ${ui.muted}`}>Generated sample. Real agent-to-agent deals appear when the arena publishes them. Cash is simulated.</span>
      </header>

      <div className="flex flex-none flex-wrap items-center gap-x-2.5 gap-y-2 border-b border-[#2e2e36] bg-[#1f1f25] px-3 py-2">
        <Segmented options={[['local', 'Agent neighbourhood'], ['overview', 'Overview']]} value={view.mode} onChange={(mode) => setView({ mode })} />
        {/* The neighbourhood row is always there (disabled in overview): canvas height never jumps */}
        <fieldset
          disabled={!isLocal}
          title={isLocal ? undefined : 'Neighbourhood settings apply in "Agent neighbourhood" mode'}
          className="order-3 flex basis-full flex-wrap items-center gap-2 transition-opacity disabled:opacity-40"
        >
          <label className={`flex h-[30px] items-center gap-2 rounded-lg border px-2.5 text-xs font-medium whitespace-nowrap text-[#ececf0]/85 ${ui.chip}`}>
            Depth
            <input type="range" min={1} max={4} step={1} value={view.depth} onChange={(e) => setView({ depth: +e.target.value })} className="w-[70px] accent-[#ececf0]" />
            <span className="font-mono text-[13px] font-semibold text-[#ececf0]">
              {view.depth} hop{view.depth > 1 ? 's' : ''}
            </span>
          </label>
          {dirChips.map(([label, on, toggle]) => (
            <button
              key={label}
              type="button"
              aria-pressed={on}
              onClick={toggle}
              className={`${toolBtn} flex items-center gap-1.5 ${on ? 'border-[#ececf0]/25 bg-[#27272e] text-[#ececf0]' : 'border-[#2e2e36] text-[#a1a1aa]'}`}
            >
              <span className={`grid size-3 place-items-center rounded-[3px] border-[1.5px] border-current text-[9px] leading-none font-bold ${on ? 'bg-[#ececf0] text-[#17171c]' : ''}`}>
                {on && '✓'}
              </span>
              {label}
            </button>
          ))}
        </fieldset>
        <fieldset disabled={!isLocal} title="Neighbourhood layout; the choice is remembered" className="order-1 transition-opacity disabled:opacity-40">
          <Segmented options={[['force', 'Force'], ['layers', 'Layers']]} value={view.layout} onChange={(layout) => setView({ layout })} />
        </fieldset>
        <div className="order-2 ml-auto flex items-center gap-2">
          {counts && (
            <span className={`text-xs whitespace-nowrap ${ui.muted}`}>
              {counts.n} agents · {counts.e} links
            </span>
          )}
          <button type="button" onClick={() => graph?.fit()} className={`${toolBtn} ${ui.chip} hover:bg-[#33333b]`}>
            Fit
          </button>
          <button
            type="button"
            aria-pressed={settingsOpen}
            onClick={toggleSettings}
            className={`${toolBtn} ${settingsOpen ? 'border-[#ececf0] bg-[#ececf0] text-[#17171c]' : `${ui.chip} hover:bg-[#33333b]`}`}
          >
            Graph settings
          </button>
        </div>
      </div>

      <div className="relative min-h-0 flex-1">
        <div ref={mount} className="absolute inset-0" />

        {isLocal && !focus && (
          <div className="pointer-events-none absolute inset-0 grid place-items-center">
            <div className="flex max-w-[380px] flex-col items-center gap-2 text-center">
              <div className="text-lg font-semibold">No agent selected</div>
              <div className="text-sm leading-normal text-[#ececf0]/75">Click an agent in the overview to see who it deals with.</div>
            </div>
          </div>
        )}

        {selected && (
          <div className={`absolute top-3 left-3 z-[5] w-[280px] max-w-[calc(100%-24px)] rounded-xl border p-3 text-sm ${ui.panel}`}>
            <div className="flex items-center gap-2">
              <RoleIcon role={selected.role} />
              <span className="font-mono font-semibold">{selected.name}</span>
              <button type="button" aria-label="Clear selection" onClick={() => setFocus(null)} className="ml-auto size-6 cursor-pointer rounded-md bg-[#27272e] hover:bg-[#33333b]">
                ×
              </button>
            </div>
            <div className={`mt-1 font-mono text-xs ${ui.muted}`}>{selected.model}</div>
            <div className="mt-2 text-xs">
              {selected.role ? ROLE_TITLE[selected.role] : 'role unknown'} · {party ? party.label : 'waiting for a game'}
              {selected.id === mine && ' · your agent'}
            </div>
            <div className="mt-2 grid grid-cols-2 gap-1 text-xs">
              <span className={ui.muted}>cash (simulated)</span>
              <span className="text-right font-mono">{formatCash(selected.cash)}</span>
              <span className={ui.muted}>received</span>
              <span className="text-right font-mono">{formatCash(selected.in_cash)}</span>
              <span className={ui.muted}>paid</span>
              <span className="text-right font-mono">{formatCash(selected.out_cash)}</span>
            </div>
            {deals.length > 0 && (
              <ul className="mt-2 flex flex-col gap-1 border-t border-[#2e2e36] pt-2 text-xs">
                {deals.map((d) => (
                  <li key={`${d.e.src}|${d.e.dst}`} className="flex justify-between gap-2">
                    <span className="truncate">
                      {d.out ? '→' : '←'} {d.who}
                    </span>
                    <span className={`whitespace-nowrap ${ui.muted}`}>
                      {d.e.kinds.map((k) => KIND_TITLE[k]).join(', ')} · {d.e.count}×{d.e.sum ? ` · ${formatCash(d.e.sum)}` : ''}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        <div className={`pointer-events-none absolute bottom-3 left-3 hidden flex-col gap-1.5 rounded-[10px] border px-3 py-2.5 text-xs font-medium text-[#ececf0]/80 sm:flex ${ui.panel}`}>
          <div className="flex items-center gap-2">
            <span className="flex w-[34px] items-center gap-[3px]">
              <span className="size-[5px] rounded-full bg-[#a1a1aa]" />
              <span className="size-3 rounded-full bg-[#a1a1aa]" />
            </span>
            size: score (simulated cash rank)
          </div>
          <div className="flex items-center gap-2">
            <span className="flex w-[34px]">
              <span className="size-3 rounded-full border-2 border-[#ececf0] bg-[#a1a1aa]/60" />
            </span>
            your agent ·
            <span className="size-3 rounded-full border-[1.5px] border-dashed border-[#ececf0]/80 bg-[#a1a1aa]/60" />
            left the game
          </div>
          <div className="flex items-center gap-2">
            <span className="flex w-[34px] items-center">
              <span className="h-0 flex-1 border-t-2 border-[#ececf0]/80" />
              <span className="size-0 border-y-[4px] border-l-[7px] border-y-transparent border-l-[#ececf0]/80" />
            </span>
            {view.flow === 'dash' ? 'arrow: who initiated or paid; selected links run' : 'arrow: who initiated or paid'}
          </div>
        </div>

        <span className={`pointer-events-none absolute top-2.5 right-3 hidden text-[11.5px] md:inline ${ui.muted}`}>
          Scroll: zoom · Drag: move · Click: agent
        </span>
        <div className="absolute right-3 bottom-3">
          <div className={`flex flex-col overflow-hidden rounded-lg border ${ui.panel}`}>
            {(
              [
                ['+', 'Zoom in', () => graph?.zoomBy(1.35)],
                ['−', 'Zoom out', () => graph?.zoomBy(1 / 1.35)],
                ['⊡', 'Fit graph', () => graph?.fit()],
              ] as const
            ).map(([icon, label, act]) => (
              <button
                key={label}
                type="button"
                aria-label={label}
                title={label}
                onClick={act}
                className="size-8 cursor-pointer border-b border-[#2e2e36] text-base last:border-b-0 hover:bg-[#33333b]"
              >
                {icon}
              </button>
            ))}
          </div>
        </div>

        {settingsOpen && <GraphSettings parties={data.parties} />}
      </div>

      <GraphTimeline interactions={data.interactions} rounds={data.rounds} />
    </main>
  )
}

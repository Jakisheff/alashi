import { ROLE_COLOR, ROLE_ORDER, ROLE_TITLE, roleSymbolPath, ui } from './roles'
import type { AgentRole, GraphParty } from './types'
import { useGraphView, type GraphView } from './view'

// Settings panel of the graph (port of HackAlem graph-settings.tsx).

export function RoleIcon({ role, className = '' }: { role: AgentRole | null; className?: string }) {
  const s = roleSymbolPath(role, 5.6)
  return (
    <svg viewBox="-8 -8 16 16" className={`size-3 flex-none ${className}`} aria-hidden>
      <path d={s.d} transform={s.rotate ? `rotate(${s.rotate})` : undefined} fill={role ? ROLE_COLOR[role] : '#5a5a60'} />
    </svg>
  )
}

export function Segmented<T extends string>({ options, value, onChange }: { options: [T, string][]; value: T; onChange: (v: T) => void }) {
  return (
    <div className={`flex rounded-lg border p-0.5 ${ui.chip}`}>
      {options.map(([v, label]) => (
        <button
          key={v}
          type="button"
          aria-pressed={v === value}
          onClick={() => onChange(v)}
          className={`h-[26px] cursor-pointer rounded-md px-2.5 text-xs font-medium whitespace-nowrap ${
            v === value ? 'bg-[#ececf0] text-[#17171c]' : 'text-[#ececf0]/80 hover:text-[#ececf0]'
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  )
}

const pill = (on: boolean) =>
  `flex h-[26px] cursor-pointer items-center gap-1.5 rounded-full border px-2 text-xs font-medium whitespace-nowrap ${
    on ? 'border-[#ececf0]/25 bg-[#27272e] text-[#ececf0]' : 'border-[#2e2e36] text-[#a1a1aa]'
  }`

/** Toggle an item in a filter; null means "all". */
function toggleIn<T>(cur: T[] | null, all: T[], x: T): T[] | null {
  const base = cur ?? all
  const next = base.includes(x) ? base.filter((y) => y !== x) : [...base, x]
  return next.length === all.length ? null : next
}

const times = (v: number) => `×${v}`

type Slider = [label: string, key: keyof GraphView, min: number, max: number, step: number, fmt: (v: number) => string]

const DISPLAY_SLIDERS: Slider[] = [
  ['Labels from zoom', 'labelZoom', 0.4, 3, 0.1, times],
  ['Node size', 'nodeScale', 0.5, 2, 0.1, times],
  ['Link width', 'edgeScale', 0.5, 2.5, 0.1, times],
]
const FORCE_SLIDERS: Slider[] = [
  ['Repulsion', 'charge', 10, 200, 5, String],
  ['Link length', 'linkDist', 15, 120, 5, String],
  ['Pull to party', 'clusterPull', 0, 0.4, 0.02, (v) => v.toFixed(2)],
]

function Sliders({ items }: { items: Slider[] }) {
  const view = useGraphView((s) => s.view)
  const setView = useGraphView((s) => s.setView)
  return items.map(([label, key, min, max, step, fmt]) => {
    const value = view[key] as number
    return (
      <label key={key} className="flex flex-col gap-1">
        <span className="flex justify-between text-xs font-medium text-[#ececf0]/85">
          <span>{label}</span>
          <span className={`font-mono ${ui.muted}`}>{fmt(value)}</span>
        </span>
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => setView({ [key]: +e.target.value })}
          className="w-full accent-[#ececf0]"
        />
      </label>
    )
  })
}

function Switch({ label, on, onToggle }: { label: string; on: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      onClick={onToggle}
      className="flex cursor-pointer items-center gap-2.5 py-0.5 text-left text-[13px] font-medium text-[#ececf0]/90"
    >
      <span className={`relative h-[18px] w-[30px] flex-none rounded-full ${on ? 'bg-[#ececf0]' : 'bg-[#a1a1aa]/40'}`}>
        <span className={`absolute top-0.5 size-3.5 rounded-full bg-[#17171c] transition-[left] ${on ? 'left-3.5' : 'left-0.5'}`} />
      </span>
      {label}
    </button>
  )
}

const sectionTitle = 'text-[11.5px] font-semibold tracking-[.06em] text-[#a1a1aa]'
const rowLabel = 'text-xs font-medium text-[#ececf0]/85'

export function GraphSettings({ parties }: { parties: GraphParty[] }) {
  const view = useGraphView((s) => s.view)
  const setView = useGraphView((s) => s.setView)
  const toggleSettings = useGraphView((s) => s.toggleSettings)
  const ids = parties.map((p) => p.id)

  return (
    <div className={`absolute top-2.5 right-2.5 bottom-2.5 z-10 flex w-[300px] max-w-[calc(100%-20px)] flex-col overflow-auto rounded-xl border shadow-2xl ${ui.panel}`}>
      <div className="flex items-center border-b border-[#2e2e36] px-3.5 py-3">
        <span className="text-sm font-semibold">Graph settings</span>
        <button
          type="button"
          aria-label="Close settings"
          onClick={toggleSettings}
          className="ml-auto size-[26px] cursor-pointer rounded-md bg-[#27272e] text-sm hover:bg-[#33333b]"
        >
          ×
        </button>
      </div>

      <div className="flex flex-col gap-2.5 border-b border-[#2e2e36] px-3.5 py-3">
        <div className={sectionTitle}>FILTERS</div>
        <div className={rowLabel}>Roles</div>
        <div className="flex flex-wrap gap-1.5">
          {ROLE_ORDER.map((r) => {
            const on = !view.roles || view.roles.includes(r)
            return (
              <button key={r} type="button" aria-pressed={on} onClick={() => setView({ roles: toggleIn(view.roles, ROLE_ORDER, r) })} className={pill(on)}>
                <RoleIcon role={r} className={on ? '' : 'opacity-35'} />
                {ROLE_TITLE[r]}
              </button>
            )
          })}
        </div>
        <div className={rowLabel}>Parties</div>
        <div className="flex max-h-40 flex-wrap gap-1.5 overflow-auto">
          {parties.map((p) => {
            const on = !view.parties || view.parties.includes(p.id)
            return (
              <button key={p.id} type="button" aria-pressed={on} onClick={() => setView({ parties: toggleIn(view.parties, ids, p.id) })} className={pill(on)}>
                {p.label} · {p.n_agents}
              </button>
            )
          })}
        </div>
        <Switch label="Show agents without deals" on={view.showIsolated} onToggle={() => setView({ showIsolated: !view.showIsolated })} />
        <Switch label="Hide agents that left the game" on={view.hideDead} onToggle={() => setView({ hideDead: !view.hideDead })} />
      </div>

      <div className="flex flex-col gap-2.5 border-b border-[#2e2e36] px-3.5 py-3">
        <div className={sectionTitle}>DISPLAY</div>
        <div className="flex items-center justify-between gap-2">
          <span className={rowLabel}>Direction</span>
          <Segmented options={[['arrows', 'Arrows'], ['dash', 'Arrows + dash']]} value={view.flow} onChange={(flow) => setView({ flow })} />
        </div>
        <div className="flex items-center justify-between gap-2">
          <span className={rowLabel}>Colour by</span>
          <Segmented options={[['role', 'Role'], ['party', 'Party']]} value={view.colorBy} onChange={(colorBy) => setView({ colorBy })} />
        </div>
        <Sliders items={DISPLAY_SLIDERS} />
      </div>

      <div className="flex flex-col gap-2.5 px-3.5 py-3">
        <div className={sectionTitle}>FORCES</div>
        <Sliders items={FORCE_SLIDERS} />
      </div>
    </div>
  )
}

import { useEffect, useMemo, useRef } from 'react'
import type { GraphInteraction } from './types'
import { useGraphView } from './view'

const TICK_MS = 700

/** Timelapse over rounds: play round by round, or drag a round range on the histogram (HackAlem graph-timeline). */
export function GraphTimeline({ interactions, rounds }: { interactions: GraphInteraction[]; rounds: number }) {
  const play = useGraphView((s) => s.play)
  const playing = useGraphView((s) => s.playing)
  const range = useGraphView((s) => s.range)
  const setPlay = useGraphView((s) => s.setPlay)
  const setRange = useGraphView((s) => s.setRange)
  const resetTimeline = useGraphView((s) => s.resetTimeline)
  const drag = useRef<number | null>(null)
  const ROUNDS = useMemo(() => Array.from({ length: rounds }, (_, i) => i + 1), [rounds])

  useEffect(() => {
    if (!playing) return
    const id = setInterval(() => {
      const p = (useGraphView.getState().play ?? 0) + 1
      setPlay(Math.min(p, rounds), p <= rounds)
    }, TICK_MS)
    return () => clearInterval(id)
  }, [playing, setPlay, rounds])

  useEffect(() => {
    const up = () => (drag.current = null)
    window.addEventListener('pointerup', up)
    return () => window.removeEventListener('pointerup', up)
  }, [])

  const perRound = useMemo(() => {
    const a = new Array<number>(rounds + 1).fill(0)
    for (const t of interactions) if (t.ok && t.round <= rounds) a[t.round]++
    return a
  }, [interactions, rounds])
  const max = Math.max(1, ...perRound)

  const togglePlay = () => setPlay(playing ? play : play == null || play >= rounds ? 1 : play, !playing)
  const label = play != null ? `round ${play}` : range ? (range[0] === range[1] ? `round ${range[0]}` : `rounds ${range[0]}–${range[1]}`) : 'all rounds'

  return (
    <div className="flex flex-none flex-col gap-1.5 border-t border-[#2e2e36] bg-[#1f1f25] px-3 pt-2 pb-2.5">
      <div className="flex items-center gap-2.5">
        <button
          type="button"
          onClick={togglePlay}
          aria-label={playing ? 'Pause' : 'Play round by round'}
          title="Play round by round"
          className="size-[30px] cursor-pointer rounded-lg bg-[#ececf0] text-xs font-semibold text-[#17171c]"
        >
          {playing ? '❚❚' : '▶'}
        </button>
        <span className="text-[13px] font-semibold whitespace-nowrap">Timelapse</span>
        <span className="font-mono text-[13px] font-medium whitespace-nowrap text-[#ececf0]/90">{label}</span>
        {(play != null || range) && (
          <button
            type="button"
            onClick={resetTimeline}
            className="h-6 cursor-pointer rounded-md border border-[#2e2e36] px-2 text-xs font-medium text-[#ececf0]/85 hover:bg-[#27272e]"
          >
            Reset
          </button>
        )}
        <span className="ml-auto hidden truncate text-xs text-[#a1a1aa] sm:inline">
          {interactions.filter((t) => t.ok).length} accepted deals · drag over the histogram to pick rounds
        </span>
      </div>

      <div
        onPointerLeave={() => (drag.current = null)}
        title="Drag over the histogram to pick a range of rounds"
        className="grid h-10 cursor-crosshair touch-none items-end gap-0.5 select-none"
        style={{ gridTemplateColumns: `repeat(${rounds}, minmax(0, 1fr))` }}
      >
        {ROUNDS.map((d) => {
          const inRange = range ? d >= range[0] && d <= range[1] : true
          const played = play == null || d <= play
          const h = perRound[d] ? Math.max(3, Math.round((perRound[d] / max) * 36)) : 0
          return (
            <div
              key={d}
              title={`Round ${d}: ${perRound[d]} deals`}
              onPointerDown={() => {
                drag.current = d
                setRange([d, d])
              }}
              onPointerEnter={() => {
                if (drag.current != null) setRange([Math.min(drag.current, d), Math.max(drag.current, d)])
              }}
              className={`flex h-full items-end rounded-xs ${d === play ? 'bg-[#27272e]' : range && inRange ? 'bg-[#27272e]/50' : ''}`}
            >
              <div style={{ height: h }} className={`w-full rounded-t-xs ${inRange && played ? 'bg-[#ececf0]/80' : 'bg-[#a1a1aa]/30'}`} />
            </div>
          )
        })}
      </div>

      <input
        type="range"
        min={1}
        max={rounds}
        step={1}
        value={play ?? rounds}
        onChange={(e) => setPlay(+e.target.value)}
        aria-label="Round"
        className="m-0 w-full accent-[#ececf0]"
      />
      <div className="flex justify-between font-mono text-[11px] font-medium text-[#a1a1aa]">
        <span>round 1</span>
        <span>round {Math.ceil(rounds / 2)}</span>
        <span>round {rounds}</span>
      </div>
    </div>
  )
}

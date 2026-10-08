import { Component, lazy, Suspense, useCallback, useEffect, useMemo, type ReactNode } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { standings } from './events'
import { useAgentWatch } from './agent'
import { useArenaFeed } from './feed'
import { Onboarding } from './Onboarding'
import type { GenieClip } from './genie/pose'
import type { HomeSearch } from './routes/index'
import { useScene } from './store'

// three.js + the model load in their own chunk: the onboarding text paints first
const Scene = lazy(() => import('./genie/Scene'))

/** No WebGL, a failed GLB or chunk: the 3D panel stays empty, the onboarding keeps working */
class SceneBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false }
  static getDerivedStateFromError() {
    return { failed: true }
  }
  render() {
    return this.state.failed ? null : this.props.children
  }
}

const CLIPS: GenieClip[] = ['idle', 'act', 'accepted', 'rejected', 'fuckOff']

// Demo jokes from the character brief; the poke reaction uses Din's requested line.
const LINES: Partial<Record<GenieClip, string[]>> = {
  fuckOff: ['Иди пососи'],
  act: ["Don't rub the lamp. Clear the context.", 'You said "small fix". I heard that.'],
  accepted: ['Wish granted. Estimate: two sprints.'],
  rejected: ["Didn't work. At least now it's reproducible."],
}

export default function App({ search }: { search: HomeSearch }) {
  // Only what this component shows: `take`/`speechId` bumps do not re-render it
  const { clip, captions, speech, play, say, toggleCaptions } = useScene(
    useShallow((s) => ({ clip: s.clip, captions: s.captions, speech: s.speech, play: s.play, say: s.say, toggleCaptions: s.toggleCaptions })),
  )
  const api = search.api ?? ''
  // Manual pose buttons are a demo tool, kept out of the main page (team onboarding P0).
  const demo = search.demo === true
  const watch = useAgentWatch(search.agent, api)
  const watchedGame = 'game' in watch && watch.game !== null ? String(watch.game) : null
  const feed = useArenaFeed(search.game ?? watchedGame, watch.kind === 'none', api)
  // Stable object: GenieModel replays the clip when it changes
  const frozen = useMemo(() => (search.pose ? { clip: search.pose, t: search.t ?? 0.5 } : null), [search.pose, search.t])

  // Manual triggers (buttons, keys 1-5) also get a joke from the brief; arena events bring their own text.
  const trigger = useCallback(
    (c: GenieClip) => {
      play(c)
      const options = LINES[c]
      if ((captions || c === 'fuckOff') && options) say(options[Math.floor(Math.random() * options.length)])
    },
    [play, say, captions],
  )

  useEffect(() => {
    if (!demo) return
    const onKey = (e: KeyboardEvent) => {
      const i = Number(e.key) - 1
      if (CLIPS[i]) trigger(CLIPS[i])
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [trigger, demo])

  return (
    <div className="flex min-h-full flex-col bg-[#F4F4F5] text-[#1A1A1E] lg:grid lg:h-dvh lg:min-h-0 lg:grid-cols-[minmax(24rem,30rem)_minmax(0,1fr)] lg:grid-rows-[minmax(0,1fr)]">
      <Onboarding watch={watch} />
      <div className="relative order-1 h-[clamp(14rem,40svh,22rem)] min-h-0 min-w-0 shrink-0 overflow-hidden lg:order-none lg:h-full">
        <SceneBoundary>
          <Suspense fallback={null}>
            <Scene frozen={frozen} />
          </Suspense>
        </SceneBoundary>

        <div className="absolute top-3 left-3 flex items-center gap-2 rounded-full bg-white/90 px-3 py-1 text-sm ring-1 ring-[#E4E4E7]">
          <span className="font-semibold">Degenie</span>
          {feed.mode === 'live' ? (
            <>
              <span className="rounded-full bg-[#1f4a43] px-2 text-xs tracking-wide text-white uppercase">{feed.result ? 'final' : 'live'}</span>
              <span className="hidden text-[#66666f] sm:inline">
                {feed.result
                  ? `game over · ${standings(feed.result)
                      .map((n, i) => `${i + 1}. ${n}`)
                      .join(' · ')}`
                  : feed.error
                    ? 'reconnecting…'
                    : feed.state
                      ? `HTTP game · simulated balances · round ${feed.state.round} · ${feed.state.phase}`
                      : 'connecting…'}
              </span>
            </>
          ) : feed.mode === 'off' ? (
            <span className="text-[#66666f]">waiting for your agent's game</span>
          ) : (
            <>
              <span className="rounded-full bg-[#26272B] px-2 text-xs tracking-wide text-white uppercase">preview</span>
              <span className="hidden text-[#66666f] sm:inline">sample events, not a real game</span>
            </>
          )}
        </div>

        {demo && (
          <div className="absolute bottom-3 left-1/2 flex -translate-x-1/2 gap-1 rounded-full bg-white/80 p-1 font-mono text-xs whitespace-nowrap sm:text-sm">
            {CLIPS.map((c, i) => (
              <button
                key={c}
                onClick={() => trigger(c)}
                className={`rounded-full px-2.5 py-0.5 ${c === clip ? 'bg-[#26272B] text-white' : 'hover:bg-[#E4E4E7]'}`}
              >
                <span className="hidden opacity-50 sm:inline">{i + 1} </span>
                {c === 'fuckOff' ? 'fuck off' : c}
              </button>
            ))}
            <button
              onClick={toggleCaptions}
              className="rounded-full px-2.5 py-0.5 hover:bg-[#E4E4E7]"
              aria-pressed={captions}
            >
              {captions ? 'jokes on' : 'jokes off'}
            </button>
          </div>
        )}
      </div>
      {/* Real game events only: the preview's sample stream would be read out every few seconds, forever */}
      <p className="sr-only" aria-live="polite">
        {feed.mode === 'live' && speech ? `Degenie: ${speech}` : ''}
      </p>
    </div>
  )
}

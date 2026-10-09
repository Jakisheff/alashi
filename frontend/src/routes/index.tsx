import { createFileRoute, Link } from '@tanstack/react-router'
import App from '../App'
import { devApi, gameId } from '../feed'
import { CLIP_SECONDS, type GenieClip } from '../genie/pose'

// Search params of the Degenie page, checked here (main.tsx skips JSON parsing so long hex ids stay strings).
export type HomeSearch = {
  /** Watch link: the owner's agent_record_id */
  agent?: string
  /** Live game id: GET /game/:id/state */
  game?: string
  /** Arena base URL, same-origin when absent */
  api?: string
  /** Manual pose buttons and keys 1-4 */
  demo?: boolean
  /** Frozen pose for review screenshots: ?pose=accepted&t=0.55 */
  pose?: GenieClip
  t?: number
}

// The router's query decoder already turns '1' into 1 and 'true' into true, so ids may arrive as numbers.
const str = (v: unknown) => (typeof v === 'number' ? String(v) : typeof v === 'string' && v !== '' ? v : undefined)

/** Drop absent keys so links and the address bar stay clean (the router revalidates its own output too). */
const defined = <T extends object>(o: T) => Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined)) as T

export const Route = createFileRoute('/')({
  validateSearch: (s: Record<string, unknown>): HomeSearch => ({
    ...defined({
      agent: str(s.agent),
      demo: s.demo === undefined || s.demo === false ? undefined : true, // bare ?demo decodes to ''
      pose: typeof s.pose === 'string' && s.pose in CLIP_SECONDS ? (s.pose as GenieClip) : undefined,
      t: s.t !== undefined && Number.isFinite(Number(s.t)) ? Number(s.t) : undefined,
    }),
    // Explicit even when undefined: the router merges this result over the raw query, so a dropped key would keep
    // the raw value (?game=../x, ?api=<any site>). Undefined values are left out of links.
    game: gameId(s.game),
    api: devApi(s.api),
  }),
  component: Home,
})

function Home() {
  const search = Route.useSearch()
  // A new ?agent= in the same tab starts from scratch instead of keeping the previous agent's game
  return <>
    <App key={search.agent ?? ''} search={search} />
    <Link to="/arena" search={{ game: undefined }} className="fixed right-4 bottom-4 z-20 rounded-full bg-[#1f4a43] px-4 py-2 text-sm font-medium text-white shadow-lg hover:bg-[#163c30] focus:outline-2 focus:outline-offset-2 focus:outline-[#1f4a43]">Watch arena</Link>
  </>
}

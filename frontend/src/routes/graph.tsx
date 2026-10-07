import { createFileRoute } from '@tanstack/react-router'
import { useEffect } from 'react'
import { parseAgentId } from '../agent'
import { GraphPage } from '../graph/GraphPage'

export type GraphSearch = {
  /** Selected agent: opens its neighbourhood */
  focus?: string
  /** The owner's agent, ringed on the graph */
  agent?: string
  /** ?mock=large: ~2.5K-agent mock network to test rendering at scale */
  mock?: 'large'
}

const hex64 = (v: unknown) => (typeof v === 'string' ? (parseAgentId(v) ?? undefined) : undefined)

export const Route = createFileRoute('/graph')({
  // Every key explicit, even undefined: the router merges this over the raw query, so a dropped key kept its raw value
  validateSearch: (s: Record<string, unknown>): GraphSearch => ({
    focus: hex64(s.focus),
    agent: hex64(s.agent),
    mock: s.mock === 'large' ? 'large' : undefined,
  }),
  component: GraphRoute,
})

function GraphRoute() {
  const search = Route.useSearch()
  const navigate = Route.useNavigate()
  useEffect(() => {
    const prev = document.title
    document.title = 'Agent network · alashi'
    // Not a landing page (mock data): index.html's robots tag says index for the whole SPA
    const robots = document.querySelector('meta[name=robots]')
    const indexed = robots?.getAttribute('content')
    robots?.setAttribute('content', 'noindex')
    return () => {
      document.title = prev
      if (indexed) robots?.setAttribute('content', indexed)
    }
  }, [])
  return (
    <GraphPage
      focus={search.focus ?? null}
      mine={search.agent ?? null}
      scale={search.mock === 'large' ? 'large' : 'small'}
      onFocus={(id) => void navigate({ search: (s) => ({ ...s, focus: id ?? undefined }), replace: true })}
    />
  )
}

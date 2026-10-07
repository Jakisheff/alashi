import { createFileRoute } from '@tanstack/react-router'
import { useEffect } from 'react'
import { GraphPage } from '../graph/GraphPage'

export type GraphSearch = {
  /** Selected agent: opens its neighbourhood */
  focus?: string
  /** The owner's agent, ringed on the graph */
  agent?: string
  /** ?mock=large: ~2.5K-agent mock network to test rendering at scale */
  mock?: 'large'
}

const hex64 = (v: unknown) => (typeof v === 'string' && /^[0-9a-f]{64}$/.test(v) ? v : undefined)

export const Route = createFileRoute('/graph')({
  validateSearch: (s: Record<string, unknown>): GraphSearch => {
    const out: GraphSearch = {}
    const focus = hex64(s.focus)
    const agent = hex64(s.agent)
    if (focus) out.focus = focus
    if (agent) out.agent = agent
    if (s.mock === 'large') out.mock = 'large'
    return out
  },
  component: GraphRoute,
})

function GraphRoute() {
  const search = Route.useSearch()
  const navigate = Route.useNavigate()
  useEffect(() => {
    const prev = document.title
    document.title = 'Agent network · alashi'
    return () => {
      document.title = prev
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

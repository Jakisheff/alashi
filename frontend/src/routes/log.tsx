import { createFileRoute } from '@tanstack/react-router'
import { useEffect } from 'react'
import { gameId } from '../feed'
import { AdminLogGate } from '../log/AdminLogGate'

export type LogSearch = { game?: string; api?: string }

export const Route = createFileRoute('/log')({
  // Both keys explicit even when undefined: the router merges this over the raw query (see routes/index.tsx)
  validateSearch: (s: Record<string, unknown>): LogSearch => ({ game: gameId(s.game), api: undefined }),
  component: LogRoute,
})

function LogRoute() {
  const { game } = Route.useSearch()
  useEffect(() => {
    const prev = document.title
    document.title = game ? `Game ${game} log · alashi` : 'Game logs · alashi'
    // Not a landing page: index.html's robots tag says index for the whole SPA
    const robots = document.querySelector('meta[name=robots]')
    const indexed = robots?.getAttribute('content')
    robots?.setAttribute('content', 'noindex')
    return () => {
      document.title = prev
      if (indexed) robots?.setAttribute('content', indexed)
    }
  }, [game])
  return <AdminLogGate game={game} />
}

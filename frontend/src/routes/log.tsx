import { createFileRoute } from '@tanstack/react-router'
import { useEffect } from 'react'
import { devApi, gameId } from '../feed'
import { LogPage } from '../log/LogPage'

export type LogSearch = { game?: string; api?: string }

export const Route = createFileRoute('/log')({
  // Both keys explicit even when undefined: the router merges this over the raw query (see routes/index.tsx)
  validateSearch: (s: Record<string, unknown>): LogSearch => ({ game: gameId(s.game), api: devApi(s.api) }),
  component: LogRoute,
})

function LogRoute() {
  const { game = '1', api } = Route.useSearch()
  useEffect(() => {
    const prev = document.title
    document.title = `Game ${game} log · alashi`
    // Not a landing page: index.html's robots tag says index for the whole SPA
    const robots = document.querySelector('meta[name=robots]')
    const indexed = robots?.getAttribute('content')
    robots?.setAttribute('content', 'noindex')
    return () => {
      document.title = prev
      if (indexed) robots?.setAttribute('content', indexed)
    }
  }, [game])
  return <LogPage key={game} game={game} api={api} />
}

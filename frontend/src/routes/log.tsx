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
    return () => {
      document.title = prev
    }
  }, [game])
  return <LogPage key={game} game={game} api={api} />
}

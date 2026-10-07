import { createFileRoute } from '@tanstack/react-router'
import { useEffect } from 'react'
import { LogPage } from '../log/LogPage'

export type LogSearch = { game?: string; api?: string }

export const Route = createFileRoute('/log')({
  validateSearch: (s: Record<string, unknown>): LogSearch => {
    const out: LogSearch = {}
    const game = typeof s.game === 'number' ? String(s.game) : s.game
    if (typeof game === 'string' && /^[1-9][0-9]*$/.test(game)) out.game = game
    if (typeof s.api === 'string') out.api = s.api
    return out
  },
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

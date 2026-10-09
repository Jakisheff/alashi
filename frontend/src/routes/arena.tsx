import { createFileRoute } from '@tanstack/react-router'
import { ArenaPage } from '../arena/ArenaPage'

const gameId = (value: unknown) => {
  const text = typeof value === 'number' ? String(value) : value
  return typeof text === 'string' && /^[1-9][0-9]*$/.test(text) ? text : undefined
}

function ArenaRoute() {
  const { game } = Route.useSearch()
  return <ArenaPage requestedGame={game} />
}

export const Route = createFileRoute('/arena')({
  validateSearch: (search: Record<string, unknown>) => ({ game: gameId(search.game) }),
  component: ArenaRoute,
})

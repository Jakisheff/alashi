import { createFileRoute } from '@tanstack/react-router'
import { DevnetPage } from '../devnet/DevnetPage'
function DevnetRoute() {
  const { game, player } = Route.useSearch()
  return <DevnetPage key={game} requestedGame={game} requestedPlayer={player} />
}
export const Route = createFileRoute('/devnet')({
  validateSearch: (search: Record<string, unknown>) => ({
    game: typeof search.game === 'string' ? search.game : '',
    player: typeof search.player === 'string' ? search.player : '',
  }),
  component: DevnetRoute,
})

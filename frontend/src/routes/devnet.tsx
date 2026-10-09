import { createFileRoute } from '@tanstack/react-router'
import { DevnetPage } from '../devnet/DevnetPage'
function DevnetRoute() {
  const { game } = Route.useSearch()
  return <DevnetPage key={game} requestedGame={game} />
}
export const Route = createFileRoute('/devnet')({
  validateSearch: (search: Record<string, unknown>) => ({ game: typeof search.game === 'string' ? search.game : '' }),
  component: DevnetRoute,
})

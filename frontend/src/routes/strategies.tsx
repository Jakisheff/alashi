import { createFileRoute } from '@tanstack/react-router'
import { StrategiesPage } from '../strategies/Page'
export const Route = createFileRoute('/strategies')({
  validateSearch: (s: Record<string, unknown>) => ({ agent: typeof s.agent === 'string' && /^[0-9a-f]{64}$/.test(s.agent) ? s.agent : undefined, parent: typeof s.parent === 'string' && /^[0-9a-f]{64}$/.test(s.parent) ? s.parent : undefined }),
  component: function StrategiesRoute() { return <StrategiesPage agent={Route.useSearch().agent} requestedParent={Route.useSearch().parent} /> },
})

import { createFileRoute } from '@tanstack/react-router'
import LivePage from '../live/LivePage'

// Separate preview route: no tokens, API calls or game mutations.
export const Route = createFileRoute('/live')({ component: LivePage })

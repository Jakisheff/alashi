import { createFileRoute } from '@tanstack/react-router'
import StreamPage from '../live/StreamPage'

// Feature-gated independently from the always-offline /live demonstration.
export const Route = createFileRoute('/stream')({ component: StreamPage })

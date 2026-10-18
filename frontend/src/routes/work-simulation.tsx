import { createFileRoute } from '@tanstack/react-router'
import { WorkSimulationPage } from '../work-simulation/Page'

export const Route = createFileRoute('/work-simulation')({ component: WorkSimulationPage })

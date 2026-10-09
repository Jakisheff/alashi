import { createRootRoute, Outlet } from '@tanstack/react-router'
import { StatusPage } from '../status/StatusPage'

export const Route = createRootRoute({
  component: Outlet,
  notFoundComponent: () => <StatusPage code="404" title="Page not found" message="This address does not lead to a page. Check the link or return to alashi." />,
  // Never expose raw exception or API text to the visitor.
  errorComponent: ({ reset }) => <StatusPage code={navigator.onLine ? '500' : 'Offline'} title={navigator.onLine ? 'This page could not load' : 'You’re offline'} message={navigator.onLine ? 'Try again to reload this page. If the problem continues, return to alashi.' : 'Reconnect to the internet, then try again.'} onRetry={reset} />,
})

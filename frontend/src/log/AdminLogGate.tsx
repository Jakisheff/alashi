import { lazy, Suspense } from 'react'
import { useQuery } from '@tanstack/react-query'
import { StatusPage } from '../status/StatusPage'
const LogPage = lazy(() => import('./LogPage').then((m) => ({ default: m.LogPage })))

// Proposed admin contract, pending Ivan's server implementation. Fail closed:
// owner pairing is not an admin grant, query strings never grant access, and no
// public /game/:id/state fallback is allowed for this screen.
export function AdminLogGate({ game }: { game?: string }) {
  const access = useQuery({
    queryKey: ['admin-log-session'], retry: false, staleTime: 0, gcTime: 0,
    queryFn: async ({ signal }) => {
      const response = await fetch('/admin/session', { credentials: 'same-origin', redirect: 'error', cache: 'no-store', signal: AbortSignal.any([signal, AbortSignal.timeout(10_000)]) })
      if (response.status === 401 || response.status === 403) return false
      if (!response.ok || !response.headers.get('content-type')?.includes('application/json')) throw new Error('admin_unavailable')
      const body: unknown = await response.json()
      return Boolean(body && typeof body === 'object' && 'ok' in body && body.ok === true && 'role' in body && body.role === 'admin')
    },
  })
  if (access.isPending) return <StatusPage code="Admin" title="Checking access" message="Verifying this browser’s administrator session." />
  if (access.isError) return <StatusPage code="503" title="Admin access unavailable" message="The administrator service is not available. The game log stays closed." onRetry={() => void access.refetch()} />
  if (!access.data) return <StatusPage code="403" title="Administrator access required" message="Game logs are available only to administrators. A player’s owner session does not grant access." />
  return <Suspense fallback={<StatusPage code="Admin" title="Loading game log" message="Opening the administrator view." />}><LogPage key={game ?? 'picker'} game={game} /></Suspense>
}

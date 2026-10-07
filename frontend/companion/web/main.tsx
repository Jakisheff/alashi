import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, createRootRoute, createRoute, createRouter, RouterProvider } from '@tanstack/react-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { gameId } from '@fe/feed'
import { Companion, LogPanel } from './Companion'
import './companion.css'

const T = (window as unknown as { __TAURI__?: { http?: { fetch: typeof fetch } } }).__TAURI__
if (!T) document.documentElement.classList.add('browser')

// The built app is served from tauri://localhost, where the shared code's same-origin '/game/..' and '/agents/..'
// requests would go nowhere and the arena sends no CORS. Route them through the Rust HTTP plugin instead
// (scope: https://alashi.network/* in capabilities). In `tauri dev` the page is on Vite, whose proxy handles them.
if (T?.http && location.protocol !== 'http:') {
  const native = window.fetch.bind(window)
  const http = T.http
  window.fetch = (input, init) =>
    typeof input === 'string' && /^\/(game|agents)\//.test(input) ? http.fetch(`https://alashi.network${input}`, init) : native(input, init)
}

// The router only drives the log panel: '/' = closed, '/log?game=N' = open. LogPage's own links
// ("alashi" -> '/', game picker -> '/log?game=') then close the panel or switch the game for free.
const root = createRootRoute({ component: Companion })
const closed = createRoute({ getParentRoute: () => root, path: '/', component: () => null })
const log = createRoute({
  getParentRoute: () => root,
  path: '/log',
  validateSearch: (s: Record<string, unknown>) => ({ game: gameId(s.game) }),
  component: LogPanel,
})
const router = createRouter({
  routeTree: root.addChildren([closed, log]),
  history: createMemoryHistory({ initialEntries: ['/'] }),
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={new QueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
)

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createRouter, parseSearchWith, RouterProvider, stringifySearchWith } from '@tanstack/react-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import { routeTree } from './routeTree.gen'
import { primeOwnerFragment } from './owner/pairing'
import { initRules } from './rules/browserInit'

const queryClient = new QueryClient()

primeOwnerFragment()

// No JSON.parse on search values (the default would turn an agent id like "1234e567..." into a number);
// the query decoder still maps plain numbers and booleans. Routes check every value in validateSearch.
const asString = (v: string) => v

const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
  defaultPreloadStaleTime: 0, // TanStack Query owns caching
  parseSearch: parseSearchWith(asString),
  stringifySearch: stringifySearchWith((v) => (typeof v === 'string' ? v : JSON.stringify(v)), asString),
})

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

// Home page: fetch the lazy 3D chunk (three.js + Degenie, which also preloads the GLB) in parallel with the route
// chunk instead of after it. App's React.lazy then resolves from the same module request.
if (location.pathname === '/') void import('./genie/Scene')

// Canonical game terms live in the rules crate (WASM); every rules-derived view
// (law names, donkey terms) calls into it, so init must precede the first render.
initRules().then(() => {
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </StrictMode>,
  )
})

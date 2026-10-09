import tailwindcss from '@tailwindcss/vite'
import { tanstackRouter } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// Opt-in SSH tunnel for the isolated Live D staging service. Never send owner
// requests through the existing production read proxies by accident.
const liveTarget = process.env.ALASHI_LIVE_API_TARGET
if (liveTarget && !/^http:\/\/(127\.0\.0\.1|localhost):[0-9]+$/.test(liveTarget)) throw new Error('ALASHI_LIVE_API_TARGET must be a local SSH tunnel')

// BASE=/releases/<sha>/ npm run build — release assets live under an immutable per-SHA prefix (NIGHT_WORK §4.7).
// The router itself stays at "/": nginx serves the current release at the site root and falls back to index.html.
export default defineConfig({
  base: process.env.BASE ?? '/',
  // tanstackRouter must come before react(); autoCodeSplitting lazy-loads each route's component
  plugins: [tanstackRouter({ target: 'react', autoCodeSplitting: true }), react(), tailwindcss()],
  // Dev only: read the public arena GET routes from the live site (same paths nginx exposes; POST stays closed there)
  server: {
    proxy: {
      ...(liveTarget ? { '/live-api': {
        target: liveTarget, changeOrigin: false,
        rewrite: (path: string) => path.replace(/^\/live-api/, ''),
        bypass(req: import('node:http').IncomingMessage, res: import('node:http').ServerResponse | undefined) {
          const path = (req.url ?? '').split('?')[0]
          const publicRead = /^\/live-api\/agents\/[0-9a-f]{64}(\/live\/events)?$/.test(path) && req.method === 'GET'
          const ownerRead = /^\/live-api\/agents\/[0-9a-f]{64}\/owner\/wishes$/.test(path) && req.method === 'GET'
          const ownerWrite = /^\/live-api\/agents\/[0-9a-f]{64}\/owner\/(challenge|session|revoke|wishes)$/.test(path) && req.method === 'POST'
          if (!publicRead && !ownerRead && !ownerWrite) { if (res) { res.statusCode = 403; res.end() } return false }
        },
      } } : {}),
      '/chain': {
        target: 'https://alashi.network', changeOrigin: true,
        bypass(req: import('node:http').IncomingMessage, res: import('node:http').ServerResponse | undefined) {
          if (req.method !== 'GET' || !/^\/chain\/devnet\/games\/[1-9A-HJ-NP-Za-km-z]{32,44}$/.test((req.url ?? '').split('?')[0])) { if (res) { res.statusCode = 403; res.end() } return false }
        },
      },
      '/game': { target: 'https://alashi.network', changeOrigin: true },
      '/agents': { target: 'https://alashi.network', changeOrigin: true },
    },
  },
})

import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// BASE=/releases/<sha>/ npm run build — release assets live under an immutable per-SHA prefix (NIGHT_WORK §4.7).
// Two pages: the Degenie scene (index.html) and the agent network graph (graph/index.html, served at /graph/).
export default defineConfig({
  base: process.env.BASE ?? '/',
  plugins: [
    react(),
    tailwindcss(),
    {
      // nginx redirects /graph to /graph/ itself; the dev server would serve the main page instead
      name: 'graph-trailing-slash',
      configureServer(server) {
        server.middlewares.use((req, res, next) => {
          if (req.url === '/graph' || req.url?.startsWith('/graph?')) {
            res.writeHead(301, { Location: req.url.replace('/graph', '/graph/') }).end()
            return
          }
          next()
        })
      },
    },
  ],
  build: {
    rolldownOptions: {
      input: { main: 'index.html', graph: 'graph/index.html' },
    },
  },
})

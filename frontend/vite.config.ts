import tailwindcss from '@tailwindcss/vite'
import { tanstackRouter } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// BASE=/releases/<sha>/ npm run build — release assets live under an immutable per-SHA prefix (NIGHT_WORK §4.7).
// The router itself stays at "/": nginx serves the current release at the site root and falls back to index.html.
export default defineConfig({
  base: process.env.BASE ?? '/',
  // tanstackRouter must come before react(); autoCodeSplitting lazy-loads each route's component
  plugins: [tanstackRouter({ target: 'react', autoCodeSplitting: true }), react(), tailwindcss()],
})

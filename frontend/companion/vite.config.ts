import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'

// Desktop companion (Tauri). Imports Degenie, the log board and the feed straight from frontend/src and uses
// frontend/node_modules, so the site and the companion share one React and one three.js.
const fe = fileURLToPath(new URL('..', import.meta.url))

export default defineConfig({
  root: fileURLToPath(new URL('./web', import.meta.url)),
  publicDir: `${fe}/public`, // GLB, Jura font
  cacheDir: `${fe}/node_modules/.vite-companion`,
  plugins: [react(), tailwindcss()],
  resolve: { alias: { '@fe': `${fe}/src` } },
  server: {
    port: 5180,
    strictPort: true,
    fs: { allow: [fileURLToPath(new URL('.', import.meta.url)), fe] },
    proxy: {
      '/game': { target: 'https://alashi.network', changeOrigin: true },
      '/agents': { target: 'https://alashi.network', changeOrigin: true },
    },
  },
  // Deps imported from frontend/src are outside root, so the scanner finds them late and re-optimizes mid-load,
  // which leaves two copies of React/R3F in one page. List them up front.
  optimizeDeps: {
    include: [
      'react', 'react-dom/client', 'react/jsx-runtime', 'three', '@react-three/fiber', '@react-three/drei',
      '@react-three/postprocessing', 'postprocessing', '@tanstack/react-query', '@tanstack/react-router', 'zustand',
    ],
  },
  build: { outDir: '../dist', emptyOutDir: true },
})

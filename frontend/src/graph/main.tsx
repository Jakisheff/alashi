import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '../index.css'
import { GraphPage } from './GraphPage'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <GraphPage />
  </StrictMode>,
)

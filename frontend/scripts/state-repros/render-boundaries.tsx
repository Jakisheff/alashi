// Test-only adapters for the renderer and unrelated navigation/private controls.
// StreamPage, its actual error boundary and scheduler are loaded from production.
import { useEffect, useState, type ReactNode } from 'react'

export function Link({ children, to, ...props }: { children?: ReactNode; to?: string }) {
  return <a href={to} {...props}>{children}</a>
}
export function OwnerPanel() { return null }
export default function Scene({ children }: { children?: ReactNode }) { return <div>{children}</div> }
export function LivingGenie() { return <span>Renderer idle</span> }
export function MarketTrade({ preview, onFinished }: {
  preview: { action: string; take: number }; onFinished: () => void
}) {
  const [failed, setFailed] = useState(false)
  useEffect(() => {
    const fail = () => setFailed(true)
    window.addEventListener('state-repro-renderer-failure', fail)
    window.__stateRepro.animationLog.push({ event: 'started', action: preview.action, take: preview.take })
    window.__stateRepro.finishCallbacks.push(onFinished)
    const timer = window.__stateRepro.completeRenderers ? window.setTimeout(() => {
      window.__stateRepro.animationLog.push({ event: 'finished', action: preview.action, take: preview.take })
      onFinished()
    }, 1000) : undefined
    return () => { window.clearTimeout(timer); window.removeEventListener('state-repro-renderer-failure', fail) }
  }, [preview.action, preview.take, onFinished])
  if (failed) throw new Error('Injected renderer failure during active action')
  return <span>Renderer playing {preview.action}</span>
}
export const ScenarioAction = MarketTrade

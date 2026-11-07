// Navigation and private controls are isolated; the scene and action meshes are real.
import { useEffect, useState, type ComponentProps, type ReactNode } from 'react'
import Scene from '../../src/genie/Scene'

export function Link({ children, to }: { children?: ReactNode; to?: string }) {
  return <a href={to}>{children}</a>
}
export function OwnerPanel() { return null }
export default function VisualScene(props: ComponentProps<typeof Scene>) {
  const [failed, setFailed] = useState(false)
  useEffect(() => {
    const fail = () => setFailed(true)
    window.addEventListener('visual-renderer-failure', fail)
    return () => window.removeEventListener('visual-renderer-failure', fail)
  }, [])
  if (failed) throw new Error('Injected visual fixture failure around the real renderer')
  return <Scene {...props} />
}

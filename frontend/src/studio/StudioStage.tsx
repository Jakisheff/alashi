import { useEffect, useMemo, useRef, type ReactNode } from 'react'
import { ConfirmationContext, stageInViewport, type Confirmation } from './confirmation'
import { BRAND, SYMBOL_PATH } from '../brand/geometry'
import './studio.css'

const asset = (name: string) => `${import.meta.env.BASE_URL}scene/alashi/${name}`

/** Decorative layers only. Gameplay and the single existing Canvas remain owned by the caller. */
export function StudioStage({ children }: { children: ReactNode }) {
  const root = useRef<HTMLDivElement>(null), pulse = useRef<HTMLDivElement>(null)
  const epoch = useRef(0), visible = useRef(true), animation = useRef<Animation | null>(null)
  const seen = useRef(new Set<string>())
  const confirmation = useMemo<Confirmation>(() => {
    const available = () => {
      return visible.current && stageInViewport(root.current)
    }
    const current = (token: number | null) => token !== null && token === epoch.current && available()
    return {
      capture: () => available() ? epoch.current : null,
      current,
      confirm: (id, token) => {
        if (seen.current.has(id)) return false
        seen.current.add(id)
        if (!current(token) || matchMedia('(prefers-reduced-motion: reduce)').matches) return false
        // One active 3D coin; coalesce closely spaced confirmations into one small platform pulse.
        animation.current?.cancel()
        animation.current = pulse.current?.animate([{ opacity: 0 }, { opacity: .28, offset: .35 }, { opacity: 0 }], { duration: 400, easing: 'ease-out' }) ?? null
        return true
      },
    }
  }, [])
  useEffect(() => {
    const cancel = () => { epoch.current++; animation.current?.cancel(); animation.current = null }
    const observer = new IntersectionObserver(([entry]) => {
      if (visible.current !== entry.isIntersecting) { visible.current = entry.isIntersecting; cancel() }
    })
    if (root.current) observer.observe(root.current)
    document.addEventListener('visibilitychange', cancel)
    window.addEventListener('resize', cancel)
    const motion = matchMedia('(prefers-reduced-motion: reduce)')
    motion.addEventListener('change', cancel)
    return () => {
      cancel(); observer.disconnect()
      document.removeEventListener('visibilitychange', cancel)
      window.removeEventListener('resize', cancel)
      motion.removeEventListener('change', cancel)
    }
  }, [])
  return <ConfirmationContext.Provider value={confirmation}><div ref={root} className="studio-stage">
    <div className="studio-decoration" aria-hidden="true">
      <img className="studio-backdrop" src={asset('backdrop.webp')} alt="" />
      <svg className="studio-watermark" viewBox="0 0 364 332"><path fill={BRAND.lilac} fillRule="evenodd" d={SYMBOL_PATH} /></svg>
      <img className="studio-halo" src={asset('halo-lavender.svg')} alt="" />
      <img className="studio-shadow" src={asset('ground-shadow.svg')} alt="" />
      <img className="studio-platform" src={asset('platform.webp')} alt="" />
      <img className="studio-reflection" src={asset('tail-reflection.svg')} alt="" />
      <div ref={pulse} className="studio-pulse" />
    </div>
    <div className="studio-model">{children}</div>
    <div className="studio-edges" aria-hidden="true" />
  </div></ConfirmationContext.Provider>
}

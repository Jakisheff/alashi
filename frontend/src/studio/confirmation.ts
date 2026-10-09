import { createContext, useContext } from 'react'
export type Confirmation = {
  capture: () => number | null
  current: (epoch: number | null) => boolean
  confirm: (id: string, epoch: number | null) => boolean
}
export const ConfirmationContext = createContext<Confirmation | null>(null)
export const useStudioConfirmation = () => useContext(ConfirmationContext)
export function stageInViewport(element: HTMLElement | null) {
  const rect = element?.getBoundingClientRect()
  return Boolean(rect && !document.hidden && rect.bottom > 0 && rect.top < innerHeight && rect.right > 0 && rect.left < innerWidth)
}

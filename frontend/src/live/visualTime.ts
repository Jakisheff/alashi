import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { actionStageAt, type ScenarioActionName } from './actions/definitions.ts'

export type VisualAction = 'buy' | 'sell' | ScenarioActionName

/** Only publish a new displayed time when a visible stage can change. */
export function visualStage(action: VisualAction, time: number) {
  if (action === 'buy') return time < 2.7 ? 0 : time < 3.65 ? 1 : time < 5.5 ? 2 : 3
  if (action === 'sell') return time < 2.8 ? 0 : time < 3.65 ? 1 : time < 5.5 ? 2 : 3
  return `${actionStageAt(action, time).at}:${time >= 6.5}`
}

/** A stale scene callback must not replace the frame of a newer event or seek. */
export function useVisualTime(identity: string | null, seek: number | null, fallback: number) {
  const key = JSON.stringify([identity, seek])
  const generation = useMemo(() => ({ key }), [key])
  const active = useRef<typeof generation | null>(null)
  const [shown, setShown] = useState<{ generation: typeof generation | null; time: number }>({ generation: null, time: 0 })
  useLayoutEffect(() => {
    active.current = generation
    return () => { if (active.current === generation) active.current = null }
  }, [generation])
  const report = useCallback((time: number) => {
    if (active.current !== generation) return
    setShown(previous => active.current !== generation || (previous.generation === generation && previous.time === time) ? previous : { generation, time })
  }, [generation])
  return [shown.generation === generation ? shown.time : seek ?? fallback, report] as const
}

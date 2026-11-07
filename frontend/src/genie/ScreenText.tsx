import { Text } from '@react-three/drei'
import { useFrame } from '@react-three/fiber'
import { useMemo, useRef, useState } from 'react'
import { Color } from 'three'
import { useScene } from '../store'
import { speechFrame, speechPages } from './screenTimeline.ts'

// Degenie "speaks" on the lower band of its own screen: text is typed out, long lines are split
// into two-line pages that are typed one after another.
const GLOW = new Color(1.5, 1.15, 0.55) // above 1 so the bloom pass picks it up, like the eyes

export function ScreenText({ clock, reducedMotion = false }: { clock?: () => number; reducedMotion?: boolean } = {}) {
  const speech = useScene((s) => s.speech)
  const speechId = useScene((s) => s.speechId)
  const clip = useScene((s) => s.clip)
  const pages = useMemo(() => speechPages(speech), [speech])
  // typed text and the length of the page it belongs to (the cursor shows while typing)
  const [shown, setShown] = useState({ text: '', of: 0 })
  const autonomous = useRef({ id: -1, time: 0 })

  // Scene clocks update at -2; sample their current frame before rendering.
  useFrame((_, delta) => {
    const own = autonomous.current
    if (own.id !== speechId) Object.assign(own, { id: speechId, time: 0 })
    const time = clock ? clock() : own.time + (Number.isFinite(delta) ? Math.max(0, delta) : 0)
    own.time = Number.isFinite(time) ? Math.max(0, time) : 0
    const next = speechFrame(pages, own.time, !clock && clip === 'fuckOff', reducedMotion)
    if (next.text !== shown.text || next.of !== shown.of) setShown(next)
  }, -1)

  const cursor = shown.text && shown.text.length < shown.of ? '_' : ''
  return (
    <Text
      position={[0, 0, 0.002]}
      font={`${import.meta.env.BASE_URL}fonts/Jura.ttf`}
      fontSize={0.07}
      lineHeight={1.15}
      maxWidth={0.8}
      textAlign="center"
      anchorX="center"
      anchorY="middle"
      color={GLOW}
      material-toneMapped={false}
    >
      {shown.text + cursor}
    </Text>
  )
}

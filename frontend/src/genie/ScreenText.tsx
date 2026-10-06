import { Text } from '@react-three/drei'
import { useFrame } from '@react-three/fiber'
import { useMemo, useRef, useState } from 'react'
import { Color } from 'three'
import { useScene } from '../store'

// Degenie "speaks" on the lower band of its own screen: text is typed out, long lines are split
// into two-line pages that are typed one after another.
const PAGE_CHARS = 46
const CHARS_PER_SECOND = 32
const HOLD_SECONDS = 1.8
const GLOW = new Color(1.5, 1.15, 0.55) // above 1 so the bloom pass picks it up, like the eyes

function paginate(text: string) {
  const pages: string[] = []
  let page = ''
  for (const word of text.split(/\s+/).filter(Boolean)) {
    if (page && (page + ' ' + word).length > PAGE_CHARS) {
      pages.push(page)
      page = word
    } else page = page ? `${page} ${word}` : word
  }
  if (page) pages.push(page)
  return pages
}

export function ScreenText() {
  const speech = useScene((s) => s.speech)
  const speechId = useScene((s) => s.speechId)
  const pages = useMemo(() => paginate(speech), [speech])
  const [shown, setShown] = useState('')
  const clock = useRef({ id: -1, page: 0, t: 0 })

  useFrame((_, delta) => {
    const c = clock.current
    if (c.id !== speechId) Object.assign(c, { id: speechId, page: 0, t: 0 })
    const page = pages[c.page]
    if (page === undefined) {
      if (shown) setShown('')
      return
    }
    c.t += delta
    const typed = page.slice(0, Math.floor(c.t * CHARS_PER_SECOND))
    if (typed !== shown) setShown(typed)
    if (c.t > page.length / CHARS_PER_SECOND + HOLD_SECONDS) Object.assign(c, { page: c.page + 1, t: 0 })
  })

  const cursor = shown && shown.length < (pages[clock.current.page]?.length ?? 0) ? '▍' : ''
  return (
    <Text
      position={[0, 0, 0.002]}
      font="/fonts/Jura.ttf"
      fontSize={0.07}
      lineHeight={1.15}
      maxWidth={0.8}
      textAlign="center"
      anchorX="center"
      anchorY="middle"
      color={GLOW}
      material-toneMapped={false}
    >
      {shown + cursor}
    </Text>
  )
}

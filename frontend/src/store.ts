import { create } from 'zustand'
import type { GenieClip } from './genie/pose'

type SceneState = {
  clip: GenieClip
  // Bumped on every trigger so repeating the same clip restarts it.
  take: number
  captions: boolean
  play: (clip: GenieClip) => void
  toggleCaptions: () => void
}

export const useScene = create<SceneState>()((set) => ({
  clip: 'idle',
  take: 0,
  captions: true,
  play: (clip) => set((s) => ({ clip, take: s.take + 1 })),
  toggleCaptions: () => set((s) => ({ captions: !s.captions })),
}))

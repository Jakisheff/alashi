import { create } from 'zustand'
import type { GenieClip } from './genie/pose'

type SceneState = {
  clip: GenieClip
  // Bumped on every trigger so repeating the same clip restarts it.
  take: number
  captions: boolean
  // What Degenie types on its screen; speechId restarts typing even for the same text.
  speech: string
  speechId: number
  play: (clip: GenieClip) => void
  say: (text: string) => void
  toggleCaptions: () => void
}

export const useScene = create<SceneState>()((set) => ({
  clip: 'idle',
  take: 0,
  captions: true,
  speech: '',
  speechId: 0,
  play: (clip) => set((s) => ({ clip, take: s.take + 1 })),
  say: (speech) => set((s) => ({ speech, speechId: s.speechId + 1 })),
  toggleCaptions: () => set((s) => ({ captions: !s.captions })),
}))

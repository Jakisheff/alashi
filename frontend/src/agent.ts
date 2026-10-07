import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'

// Watch link /?agent=<agent_record_id>. Contract: team docs ONBOARDING_FOR_DIN (6aa34b1), arena e162740
// GET /agents/:record_id. Only the public record id ever goes in the URL; never a secret or token.

export type AgentSlot = {
  game_id: number
  party_no: number
  label: string
  phase: string
  round: number
  faction_idx: number
  faction_name: string | null
}

export type AgentProfile =
  | { ok: true; agent_record_id: string; character_id: string; registered: true; active_slots: AgentSlot[] }
  | { ok: false; error: string }

/** 64 lowercase hex, exactly what the arena accepts. Anything else is never sent. */
export const parseAgentId = (raw: string | null) => (raw && /^[0-9a-f]{64}$/.test(raw) ? raw : null)

/** The game to watch: the agent's current slot, else the last one seen (a finished game keeps its result). */
export function followGame(prev: number | null, profile: AgentProfile | undefined) {
  if (!profile?.ok) return prev
  return profile.active_slots[0]?.game_id ?? prev
}

export type AgentWatch =
  | { kind: 'none' }
  | { kind: 'invalid' }
  | { kind: 'loading' | 'unreachable' | 'unknown'; game: number | null }
  | { kind: 'waiting' | 'playing'; game: number | null; slot: AgentSlot | null }

/** raw: the ?agent= value as given (undefined when absent); api: arena base, same-origin when empty. */
export function useAgentWatch(raw: string | undefined, api = ''): AgentWatch {
  const id = parseAgentId(raw ?? null)
  const [game, setGame] = useState<number | null>(null)
  const profile = useQuery({
    queryKey: ['agent', api, id],
    enabled: id !== null,
    refetchInterval: 5000,
    retry: 1, // the poll retries anyway; show "can't reach" quickly
    queryFn: async () => {
      const res = await fetch(`${api}/agents/${id}`)
      if (!res.ok) throw new Error(`agent ${res.status}`)
      return (await res.json()) as AgentProfile
    },
  })
  const next = followGame(game, profile.data)
  if (next !== game) setGame(next) // adjust state during render: no extra effect pass

  if (raw === undefined) return { kind: 'none' }
  if (id === null) return { kind: 'invalid' }
  const data = profile.data
  if (!data) return { kind: profile.isError ? 'unreachable' : 'loading', game: next }
  if (!data.ok) return { kind: 'unknown', game: next }
  const slot = data.active_slots[0] ?? null
  return { kind: slot ? 'playing' : 'waiting', game: next, slot }
}

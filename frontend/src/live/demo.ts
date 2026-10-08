import type { GenieClip } from '../genie/pose'

// Offline UI fixtures based on LIVE-STREAM-IVAN-20261008-03 (docs b8accd20).
// seq and timestamps belong to this demonstration, never to the production arena.
export type Scenario = 'dialogue' | 'silence' | 'reconnect' | 'between' | 'preview'
export type DemoEvent = {
  event_id: string
  seq: number
  at: number
  kind: 'agent_message' | 'phase_changed' | 'game_action' | 'preview' | 'final_result'
  author: string
  text: string
  host?: boolean
  to?: string
  reply_to_message_id?: string
  clip?: GenieClip
  action?: 'sell' | 'buy'
  actor?: 'alpha'
  ok?: boolean
}

const event = (seq: number, at: number, kind: DemoEvent['kind'], author: string, text: string, extra: Partial<DemoEvent> = {}): DemoEvent =>
  ({ event_id: `demo-${seq}`, seq, at, kind, author, text, ...extra })

export const SCENARIOS: Record<Scenario, { label: string; title: string; description: string; events: DemoEvent[] }> = {
  dialogue: {
    label: 'Conversation', title: 'An agent in its element', description: 'Alpha talks with the group and makes a decision. Messages are words; actions are confirmed separately.',
    events: [
      event(101, 0, 'phase_changed', 'Arena', 'Market open · round 2'),
      event(102, 1, 'agent_message', 'Beta', 'The price may change if several agents sell.'),
      event(103, 3, 'agent_message', 'Alpha', 'I’ll hold my goods for now. Let’s see where the price goes.', { host: true, clip: 'act' }),
      event(104, 8, 'agent_message', 'Gamma', 'Alpha, are you planning to buy?', { to: 'Alpha' }),
      event(105, 11, 'agent_message', 'Alpha', 'I’ll watch a little longer. The price is moving too fast.', { host: true, to: 'Gamma', reply_to_message_id: 'demo-104', clip: 'act' }),
      event(107, 19, 'game_action', 'Arena', 'Alpha sold 1 good · action accepted', { clip: 'accepted', action: 'sell', actor: 'alpha', ok: true }),
    ],
  },
  silence: {
    label: 'Silence', title: 'Silence is an option', description: 'The agent is connected, with no messages yet. The game and timer continue; silence says nothing about its next move.',
    events: [event(201, 0, 'phase_changed', 'Arena', 'Market open · no messages yet')],
  },
  reconnect: {
    label: 'Reconnect', title: 'Back on air', description: 'The browser loses connection, but the agent stays connected. The feed resumes from its last cursor when the connection returns.',
    events: [
      event(301, 0, 'phase_changed', 'Arena', 'Market open · round 2'),
      event(302, 1, 'agent_message', 'Alpha', 'Let’s see what this round brings.', { host: true, clip: 'act' }),
      event(303, 5, 'agent_message', 'Beta', 'I’m watching the price for now.'),
      event(304, 7, 'game_action', 'Arena', 'Beta produced 1 good · action accepted', { clip: 'accepted' }),
      event(305, 10, 'agent_message', 'Alpha', 'Now I can see how the market changed.', { host: true, clip: 'act' }),
    ],
  },
  between: {
    label: 'Between games', title: 'The game ends. The stream stays.', description: 'The same agent, the same personal stream. It can discuss public results and reply between games.',
    events: [
      event(401, 0, 'final_result', 'Arena', 'Game finished · result saved'),
      event(402, 2, 'agent_message', 'Alpha', 'The game is over. I’ll stay here until the next one.', { host: true, clip: 'act' }),
      event(403, 7, 'agent_message', 'Beta', 'What did you think of the last round?', { to: 'Alpha' }),
      event(404, 11, 'agent_message', 'Alpha', 'The market surprised me. Next time I’ll look earlier.', { host: true, to: 'Beta', reply_to_message_id: 'demo-403', clip: 'act' }),
    ],
  },
  preview: {
    label: 'B0 preview', title: 'The world resets. Memory stays.', description: 'A separate repeat-probe hypothesis: B0 results come from a copy of the world and leave the main balance unchanged. This is not part of Classic rules.',
    events: [
      event(501, 0, 'phase_changed', 'Arena', 'P0 closed · calculating a copy of S'),
      event(502, 3, 'preview', 'Arena', 'B0 preview: Alpha 22/0, Beta 17/0 · main S: 0/2 for both', { clip: 'accepted' }),
      event(503, 7, 'agent_message', 'Alpha', 'I can see the preview result. It hasn’t been credited.', { host: true, clip: 'act' }),
      event(504, 10, 'phase_changed', 'Arena', 'Return to S · preview history retained'),
      event(505, 14, 'phase_changed', 'Arena', 'P1 open · other agents’ intentions and readiness stay private'),
    ],
  },
}

export const formatDuration = (seconds: number) => {
  const n = Math.max(0, Math.ceil(seconds))
  return `${String(Math.floor(n / 60)).padStart(2, '0')}:${String(n % 60).padStart(2, '0')}`
}

export function demoState(scenario: Scenario, elapsed: number) {
  const reconnecting = scenario === 'reconnect' && elapsed >= 2 && elapsed < 8
  // Entries created during disconnection arrive together on reconnect, preserving canonical order/IDs.
  const visibleUntil = reconnecting ? 2 : elapsed
  const events = SCENARIOS[scenario].events.filter((e) => e.at <= visibleUntil)
  const game = scenario === 'between' ? 'finished' : scenario === 'preview' ? (elapsed < 10 ? 'B0' : 'P1') : elapsed >= 30 ? 'action' : 'market'
  if (scenario !== 'between' && scenario !== 'preview') {
    const seqs = scenario === 'dialogue' ? [106, 108, 109] : scenario === 'silence' ? [202, 203, 204] : [306, 307, 308]
    for (const [i, at, text] of [[0, 15, 'Market ends in 15 seconds'], [1, 25, 'Market ends in 5 seconds'], [2, 30, 'Action open · new 30-second window']] as const) {
      const seq = seqs[i]
      if (at <= visibleUntil) events.push(event(seq, at, 'phase_changed', 'Arena', text))
    }
  }
  events.sort((a, b) => a.at - b.at || a.seq - b.seq)
  // Finite preview: it never invents repeated rounds or agent speech to fill the stream.
  return { events, game, reconnecting, remaining: game === 'market' ? 30 - elapsed : game === 'action' ? 60 - elapsed : null }
}

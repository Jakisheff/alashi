import { mergePublic, projectPublicPage } from '../src/live/api/client.ts'

const base = { ok: true, next_cursor: 4, has_more: false, history_truncated: false, server_now: 1, presence: 'connected' }
const event = (id: string, seq: number, visibility: string, cue?: unknown) => ({
  event_id: id, seq, kind: 'agent_message', visibility, server_created_at: '2026-10-09T00:00:00Z',
  room_id: 'agent:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
  author_agent_record_id: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', text: 'public', ...(cue === undefined ? {} : { gesture_cue: cue }),
})
const page = projectPublicPage({ ...base, events: [event('valid', 1, 'public', 'realization'), event('unknown', 2, 'public', 'wink'), event('private', 3, 'private', 'facepalm')] })
if (page.events.length !== 2) throw new Error('private event entered public projection')
if (page.events[0].gestureCue !== 'realization') throw new Error('valid public cue missing')
if (page.events[1].gestureCue !== undefined) throw new Error('unknown cue must not react')
if (mergePublic(page.events, [page.events[0]]).length !== 2) throw new Error('cursor retry duplicated canonical event')
console.log('live gesture projection: PASS')

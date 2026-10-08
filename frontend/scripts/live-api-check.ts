import assert from 'node:assert/strict'
import { createLiveApi, LiveApiError, mergePublic, mergeWishes, projectPublicPage, projectWishes } from '../src/live/api/client.ts'
import { base58, validateChallenge } from '../src/live/api/wallet.ts'

const id = 'a'.repeat(64), wallet = '5Y4iLucyhEA8DWnA89YY6tt87dF2bawJ8uNNGn1TMHFT'
const baseEvent = { event_id: 'live-1', seq: 1, server_created_at: '2026-10-08T19:49:14Z', room_id: `agent:${id}`, visibility: 'public' }
const page = { ok: true, events: [{ ...baseEvent, kind: 'agent_message', author_agent_record_id: id, text: 'Public hello', owner_session: 'DO_NOT_RETAIN', private_wish: 'DO_NOT_RETAIN' }, { ...baseEvent, seq: 2, event_id: 'private', visibility: 'owner', kind: 'owner_wish', text: 'DO_NOT_RETAIN' }], next_cursor: 2, has_more: false, history_truncated: false, server_now: 123, presence: 'connected' }
const projected = projectPublicPage(page)
assert.equal(projected.events.length, 1)
assert(!JSON.stringify(projected).includes('DO_NOT_RETAIN'))
assert.equal(mergePublic(projected.events, projected.events).length, 1)
assert.throws(() => projectPublicPage({ ...page, next_cursor: 0 }), LiveApiError)
assert.throws(() => projectPublicPage({ ...page, has_more: 'yes' }), LiveApiError)
const rawWish = { wish_id: 'w1', seq: 1, status_seq: 1, game_id: 2, text: 'Private guidance', status: 'received', reply: null, accepted_at: 123 }
const privatePage = projectWishes({ wishes: [rawWish], next_cursor: 1, last_seq: 1, remaining_by_game: { 2: 2 } })
const replied = projectWishes({ wishes: [{ ...rawWish, status_seq: 3, status: 'replied', reply: 'Private reply' }], next_cursor: 3, last_seq: 3, remaining_by_game: { 2: 0 } })
assert.equal(mergeWishes(mergeWishes(privatePage.wishes, replied.wishes), privatePage.wishes)[0].status, 'replied')
assert.equal(replied.remaining['2'], 0)
assert.throws(() => projectWishes({ wishes: [], next_cursor: 0, last_seq: 0, remaining_by_game: { 2: 4 } }), LiveApiError)
assert.throws(() => createLiveApi('https://other.example'), LiveApiError)
const requests: { url: string; init?: RequestInit }[] = []
const api = createLiveApi('/live-api', (async (url, init) => {
  requests.push({ url: String(url), init })
  return new Response(JSON.stringify(String(url).includes('/live/events') ? page : { ok: true, wishes: [], next_cursor: 0, last_seq: 0, remaining_by_game: { 2: 3 } }), { headers: { 'Content-Type': 'application/json' } })
}) as typeof fetch)
await api.public(id, 0)
await api.wishes(id, 'owner-only-test-token', 0)
assert(!JSON.stringify(requests[0]).includes('owner-only-test-token'))
assert(!requests[1].url.includes('owner-only-test-token'))
assert(requests[1].init)
assert.equal((requests[1].init.headers as Record<string, string>).Authorization, 'Bearer owner-only-test-token')
assert.equal(requests[1].init?.credentials, 'omit'); assert.equal(requests[1].init?.cache, 'no-store'); assert.equal(requests[1].init?.redirect, 'error')
const failing = createLiveApi('', (async () => new Response(JSON.stringify({ ok: false, error: 'PRIVATE_SECRET_NOT_AN_ERROR_CODE' }), { status: 500 })) as typeof fetch)
await assert.rejects(() => failing.public(id, 0), (e: unknown) => e instanceof LiveApiError && e.message === 'unavailable')
const nonce = 'b'.repeat(64), origin = 'https://localhost:5173'
const challenge = { id: 'c'.repeat(64), wallet, issuedAt: 100, expiresAt: 200, message: `alashi-owner-auth-v1\norigin:${origin}\nagent_record_id:${id}\nwallet:${wallet}\nnonce:${nonce}\nissued_at:100\nexpires_at:200\n` }
validateChallenge(challenge, id, wallet, origin, 150)
assert.throws(() => validateChallenge(challenge, id, wallet, 'https://evil.example', 150), LiveApiError)
assert.throws(() => validateChallenge(challenge, 'd'.repeat(64), wallet, origin, 150), LiveApiError)
assert.throws(() => validateChallenge(challenge, id, wallet, origin, 250), LiveApiError)
assert.throws(() => validateChallenge({ ...challenge, message: `${challenge.message}send transaction` }, id, wallet, origin, 150), LiveApiError)
assert.equal(base58(new Uint8Array([0, 0, 1])), '112')
assert.equal(base58(new Uint8Array([255])), '5Q')
console.log('PASS Live D projection, private/public separation, cursor validation, status merging, server quota, safe transport, error sanitization and wallet challenge binding')

import assert from 'node:assert/strict'
import { createLiveApi, LiveApiError } from '../src/live/api/client.ts'

const record = 'a'.repeat(64)
const wallet = '5Y4iLucyhEA8DWnA89YY6tt87dF2bawJ8uNNGn1TMHFT'
const requests: { url: string; init?: RequestInit }[] = []
const api = createLiveApi('', (async (url, init) => {
  requests.push({ url: String(url), init })
  const path = new URL(String(url), 'https://alashi.network').pathname
  const method = init?.method ?? 'GET'
  if (path.endsWith('/browser/session') && method === 'GET') return new Response(JSON.stringify({ ok: true, wallet, expires_at: 2_000_000_000 }))
  if (path.endsWith('/browser/session') && method === 'POST') return new Response(JSON.stringify({ ok: true, expires_at: 2_000_000_000, scope: ['owner_wishes'] }))
  if (path.endsWith('/browser/logout')) return new Response(JSON.stringify({ ok: true }))
  if (path.endsWith('/owner/wishes') && method === 'GET') return new Response(JSON.stringify({ ok: true, wishes: [], next_cursor: 0, last_seq: 0, remaining_by_game: { 2: 3 } }))
  if (path.endsWith('/owner/wishes') && method === 'POST') return new Response(JSON.stringify({ ok: true, wish_id: 'w1', remaining: 2 }))
  return new Response(JSON.stringify({ ok: false, error: 'unknown_agent' }), { status: 404 })
}) as typeof fetch)

const restored = await api.restoreBrowserSession(record)
assert.equal(restored.wallet, wallet)
await api.browserSession(record, 'b'.repeat(64), 'signature')
const persisted = await api.restoreBrowserSession(record)
assert.equal(persisted.wallet, wallet)
await api.wishes(record, 0)
await api.submit(record, { game_id: 2, client_wish_id: 'wish-one', text: 'Private guidance' })
await api.logoutBrowserSession(record)
for (const request of requests) {
  assert.equal(request.init?.credentials, 'include')
  const headers = request.init?.headers as Record<string, string> | undefined
  assert.equal(headers?.Authorization, undefined)
  assert(!JSON.stringify(request).includes('owner_session'))
  assert(!request.url.includes('signature'))
}
assert.equal(requests.filter((request) => request.url.endsWith('/browser/session') && request.init?.method === 'GET').length, 2)
assert.equal(requests.filter((request) => request.url.endsWith('/browser/logout')).length, 1)
const unavailable = createLiveApi('', (async () => new Response(JSON.stringify({ ok: false, error: 'owner_session_expired' }), { status: 401 })) as typeof fetch)
await assert.rejects(() => unavailable.restoreBrowserSession(record), (error: unknown) => error instanceof LiveApiError && error.code === 'owner_session_expired')
console.log('PASS browser-cookie owner API: restore/login/logout/wishes use credentials include, expose no bearer and preserve private request boundaries')

import assert from 'node:assert/strict'
import { createChainOwnerApi, mergeChainWishes, projectChainWishes } from '../src/devnet/ownerClient.ts'
import { LiveApiError } from '../src/live/api/client.ts'
import type { ChainFaction } from '../src/devnet/client.ts'
const game = 'GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b', record = 'a'.repeat(64), signature = '1'.repeat(64)
const faction: ChainFaction = { pda: '11111111111111111111111111111111', wallet: 'DmqGyi9wBz1RQyueGFRmF7ZuZ4p44YbQKR9LHysbduFo', name: 'Private unit fixture', cash: '1', hard: '0', alive: true, goods: 1, influence: 1, vote: 'Yes' }
const wish = { wish_id: 'private-unit-1', seq: 1, status_seq: 2, intent: 'sell_one', text: 'PRIVATE_UNIT_TEXT', status: 'consumed', accepted_at: 1, reply: null, signature: null, slot: null, event_id: null }
const page = { ok: true, game_pda: game, binding: { faction_pda: faction.pda, faction_wallet: faction.wallet, active: true }, remaining: 2, wishes: [wish], next_cursor: 2, last_seq: 2 }
const projected = projectChainWishes({ ...page, runner_token: 'SECRET_MUST_BE_REMOVED', wishes: [{ ...wish, runner_token: 'SECRET_MUST_BE_REMOVED' }] }, game, faction)
assert.ok(!JSON.stringify(projected).includes('SECRET_MUST_BE_REMOVED'))
assert.equal(projected.remaining, 2)
const unbound = projectChainWishes({ ...page, binding: null, wishes: [], next_cursor: 0, last_seq: 0, remaining: 3 }, game, faction)
assert.equal(unbound.active, false, 'an owner without a bound live runner keeps the private journal readable')
assert.equal(unbound.remaining, 3)
assert.throws(() => projectChainWishes({ ...page, game_pda: faction.pda }, game, faction), LiveApiError)
assert.throws(() => projectChainWishes({ ...page, binding: { ...page.binding, faction_wallet: faction.pda } }, game, faction), LiveApiError)
assert.throws(() => projectChainWishes({ ...page, remaining: 4 }, game, faction), LiveApiError)
assert.throws(() => projectChainWishes({ ...page, wishes: [{ ...wish, status: 'confirmed' }] }, game, faction), LiveApiError)
const confirmed = projectChainWishes({ ...page, wishes: [{ ...wish, status: 'confirmed', signature, slot: 100, event_id: `${signature}:1` }] }, game, faction)
assert.equal(confirmed.wishes[0].status, 'confirmed')
const unconfirmed = projectChainWishes({ ...page, wishes: [{ ...wish, status: 'unconfirmed', status_seq: 3 }], next_cursor: 3, last_seq: 3 }, game, faction)
assert.equal(unconfirmed.wishes[0].status, 'unconfirmed')
const reconciled = projectChainWishes({ ...page, wishes: [{ ...wish, status: 'confirmed', status_seq: 4, signature, slot: 100, event_id: `${signature}:1` }], next_cursor: 4, last_seq: 4 }, game, faction)
assert.equal(mergeChainWishes(unconfirmed.wishes, reconciled.wishes)[0].status, 'confirmed')
assert.equal(mergeChainWishes(reconciled.wishes, unconfirmed.wishes)[0].status, 'confirmed', 'stale unconfirmed response cannot undo a matched receipt')
assert.equal(mergeChainWishes(confirmed.wishes, projected.wishes).length, 1)
let url = '', init: RequestInit | undefined
const api = createChainOwnerApi('', async (u, i) => { url = String(u); init = i; return new Response(JSON.stringify(i?.method === 'POST' ? { ok: true, game_pda: game, wish_id: wish.wish_id, status: 'received', remaining: 2 } : page)) })
await api.wishes(record, game, faction, 0)
assert.equal(init?.credentials, 'include'); assert.equal(init?.cache, 'no-store'); assert.equal(init?.redirect, 'error')
assert.ok(!url.includes('PRIVATE_UNIT_TEXT')); assert.ok(!JSON.stringify(init?.headers).toLowerCase().includes('authorization'))
const body = { game_pda: game, intent: 'sell_one' as const, client_wish_id: 'unit-retry-id', text: wish.text }
await api.submit(record, body)
const original = init?.body
await api.submit(record, body)
assert.equal(init?.body, original, 'idempotent retry preserves complete body')
assert.ok(!url.includes(body.text) && !url.includes(body.client_wish_id))
assert.equal(init?.method, 'POST')
await assert.rejects(api.submit(record, { ...body, text: 'x'.repeat(513) }), LiveApiError)
await assert.rejects(api.submit(record, { ...body, text: 'line\nbreak' }), LiveApiError)
console.log('private chain binding, quota, confirmed receipt, cookie-only transport, exact retries and secret projection checks passed')

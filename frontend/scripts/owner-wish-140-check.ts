import assert from 'node:assert/strict'
import { inspectOwnerWish } from '../src/owner/wishText.ts'
import { createLiveApi, LiveApiError, projectWishes } from '../src/live/api/client.ts'
import { createChainOwnerApi, projectChainWishes } from '../src/devnet/ownerClient.ts'
const record = 'a'.repeat(64), game = 'GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b'
let checks = 0
function check(value: unknown, message: string) { assert.ok(value, message); checks++ }
const accepted = ['a'.repeat(140), 'Ж'.repeat(140), '漢'.repeat(140), '😀'.repeat(140), 'e\u0301'.repeat(70), '👩‍💻'.repeat(46), 'Guidance']
const rejected = ['', '   ', 'a'.repeat(141), '😀'.repeat(141), 'e\u0301'.repeat(71), 'two\nlines', 'tab\ttext', 'control\u0000', 'delete\u007f', 'c1\u0080', 'c1\u0085', 'c1\u009f', '\ud800', '\udfff']
for (const value of accepted) check(inspectOwnerWish(value).valid, 'valid scalar boundary')
for (const value of rejected) check(!inspectOwnerWish(value).valid, 'invalid admission')
check(inspectOwnerWish('😀'.repeat(140)).count === 140, 'astral symbols count once')
check(inspectOwnerWish('e\u0301').count === 2, 'combining mark counts separately')
check(inspectOwnerWish('👩‍💻').count === 3, 'ZWJ sequence counts scalars')
const calls: RequestInit[] = []
const fetcher: typeof fetch = async (_url, init) => { calls.push(init!); return new Response(JSON.stringify({ ok: true, game_pda: game, status: 'received', wish_id: 'unit', remaining: 2 })) }
const http = createLiveApi('', fetcher), chain = createChainOwnerApi('', fetcher)
for (const api of [http, chain]) {
  for (const text of rejected) {
    const before = calls.length
    await assert.rejects(api.submit(record, { game_id: 2, game_pda: game, intent: 'produce', client_wish_id: 'stable-retry', text }), (error: unknown) => error instanceof LiveApiError && error.code === 'bad_wish'); checks++
    check(calls.length === before, 'invalid text makes no request or quota debit')
  }
  for (const text of accepted) {
    const body = { game_id: 2, game_pda: game, intent: 'produce' as const, client_wish_id: 'stable-retry', text }
    await api.submit(record, body); const first = calls.at(-1)!
    await api.submit(record, body); const retry = calls.at(-1)!
    check(first.body === JSON.stringify(body) && retry.body === first.body, 'exact text/id preserved on retry')
    check(first.credentials === 'include' && first.redirect === 'error' && first.cache === 'no-store', 'cookie transport boundary retained')
  }
}
const historical = 'x'.repeat(512), reply = 'Old private reply'
const w = { wish_id: 'legacy', seq: 1, status_seq: 1, game_id: 2, text: historical, reply, status: 'received', intent: 'produce', accepted_at: 1 }
check(projectWishes({ remaining_by_game: { 2: 2 }, wishes: [w], next_cursor: 1, last_seq: 1 }).wishes[0].text === historical, 'HTTP old history retained')
const faction = { pda: '11111111111111111111111111111111', wallet: game, name: 'Fixture', cash: '0', hard: '0', alive: true, goods: 0, influence: 0, vote: 'Yes' as const }
const page = projectChainWishes({ ok: true, game_pda: game, binding: { faction_pda: faction.pda, faction_wallet: faction.wallet, active: true }, remaining: 2, wishes: [w], next_cursor: 1, last_seq: 1 }, game, faction)
check(page.wishes[0].text === historical && page.wishes[0].reply === reply, 'Chain old history and reply retained')
console.log(`PASS ${checks} Unicode admission, no-request, exact-retry, private transport and historical journal checks`)

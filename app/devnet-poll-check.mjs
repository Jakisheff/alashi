import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import vm from 'node:vm'
const html = await readFile(new URL('./devnet.html', import.meta.url), 'utf8')
const script = html.match(/<script>\n([\s\S]*?)\n<\/script>/)[1]
const flush = async () => { for (let i = 0; i < 40; i++) await Promise.resolve() }
function harness() {
  let now = 100_000, id = 0
  const timers = new Map(), nodes = new Map(), calls = [], events = {}
  const element = (name) => {
    if (!nodes.has(name)) nodes.set(name, { textContent: '', className: '', classList: { add() {}, remove() {} }, addEventListener() {}, replaceChildren() {}, append() {} })
    return nodes.get(name)
  }
  class Key { constructor(value) { this.value = value } equals(other) { return this.value === other.value } toBase58() { return this.value } }
  const owner = new Key('3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC')
  const game = (settled = false, factions = 0, gameId = 1n) => {
    const data = new Uint8Array(137)
    data.set([27, 90, 166, 125, 74, 100, 121, 18]); new DataView(data.buffer).setBigUint64(40, gameId, true)
    data[48] = settled ? 4 : 1; data[49] = 6; data[58] = factions; data[136] = Number(settled)
    return { owner, data }
  }
  const faction = new Uint8Array(93); faction.set([131, 20, 223, 22, 227, 204, 231, 35])
  const api = { game: async () => game(), discovery: async () => [], multiple: async () => [], activity: async () => [], fetch: async () => ({ status: 200 }) }
  Key.findProgramAddress = async () => [new Key('one')]
  let config
  class Connection {
    constructor(_rpc, options) { config = options }
    getAccountInfo(key) { calls.push(['game', key.value, now]); return api.game(key) }
    getProgramAccounts() { calls.push(['discovery', now]); return api.discovery() }
    getMultipleAccountsInfo() { calls.push(['multiple', now]); return api.multiple() }
    getSignaturesForAddress(key) { calls.push(['activity', key.value, now]); return api.activity() }
  }
  const document = { hidden: false, getElementById: element, createElement: element, addEventListener: (name, cb) => { events[name] = cb } }
  const context = vm.createContext({ Uint8Array, DataView, TextDecoder, TextEncoder, URLSearchParams, AbortController,
    Date: class extends Date { static now() { return now } }, Math: Object.assign(Object.create(Math), { random: () => 0 }),
    fetch: (...args) => api.fetch(...args), document, history: { replaceState() {} }, location: { pathname: '/devnet', search: '' },
    window: { setTimeout: (fn, ms) => { timers.set(++id, { fn, at: now + ms }); return id }, clearTimeout: (i) => timers.delete(i), addEventListener: (name, cb) => { events[name] = cb } }, solanaWeb3: { PublicKey: Key, Connection } })
  vm.runInContext(script, context)
  const advance = async (ms) => { now += ms; for (const [i, timer] of [...timers]) if (timer.at <= now) { timers.delete(i); timer.fn() } await flush() }
  return { context, api, calls, nodes, document, events, game, faction, owner, config, timers, advance, open: (key = 'one') => context.openGame(key) }
}
// A slow read and repeated clicks cannot start overlapping polls.
{
  const h = harness(); let finish
  h.api.game = () => new Promise((resolve) => { finish = resolve })
  h.open(); h.open(); await h.advance(60_000)
  assert.equal(h.calls.length, 1)
  finish(h.game()); await flush()
  assert.equal(h.calls.filter(([method]) => method === 'game').length, 1)
  await h.advance(11_999); assert.equal(h.calls.filter(([method]) => method === 'game').length, 1)
  await h.advance(1); assert.equal(h.calls.filter(([method]) => method === 'game').length, 2)
}
// Respect Retry-After, disable SDK retries, and recover automatically without click bypass.
{
  const h = harness(); assert.equal(h.config.disableRetryOnRateLimit, true)
  let requests = 0
  h.api.fetch = async () => { requests++; return { status: 429, headers: { get: () => '45' } } }
  h.api.game = async () => { try { await h.config.fetch('https://api.devnet.solana.com', {}) } catch (error) { throw new Error(`failed to get info about account: ${error.message}`) } return h.game() }
  h.open(); await flush(); assert.equal(requests, 1)
  assert.match(h.nodes.get('status').textContent, /Automatic retry in 45 seconds/)
  h.open(); h.open(); await h.advance(44_999); assert.equal(requests, 1)
  h.api.fetch = async () => { requests++; return { status: 200 } }
  await h.advance(1); assert.equal(requests, 2)
  assert.match(h.nodes.get('status').textContent, /Confirmed devnet state/)
}
// Exponential cooldown grows on consecutive 429s, including JSON RPC errors at HTTP 200.
{
  const h = harness(); h.api.game = async () => { throw new Error('429 Too many requests for a specific RPC call') }
  h.open(); await flush(); await h.advance(15_000)
  assert.equal(h.calls.length, 2); await h.advance(29_999); assert.equal(h.calls.length, 2)
  await h.advance(1); assert.equal(h.calls.length, 3)
  await h.advance(59_999); assert.equal(h.calls.length, 3)
}
// Poll known faction accounts in one request; do not rediscover them or fetch activity each cycle.
{
  const h = harness(); h.api.game = async () => h.game(false, 1)
  h.api.discovery = async () => [{ pubkey: {}, account: { owner: h.owner, data: h.faction } }]
  h.api.multiple = async () => [{ owner: h.owner, data: h.faction }]
  h.open(); await flush(); await h.advance(12_000); await h.advance(12_000)
  assert.equal(h.calls.filter(([m]) => m === 'discovery').length, 1)
  assert.equal(h.calls.filter(([m]) => m === 'multiple').length, 2)
  assert.equal(h.calls.filter(([m]) => m === 'activity').length, 1)
}
// Final settled state is stable: stop polls; manual refresh still observes the request budget.
{
  const h = harness(); h.api.game = async () => h.game(true)
  h.open(); await flush(); const count = h.calls.length
  await h.advance(600_000); assert.equal(h.calls.length, count)
  h.open(); await flush(); assert.equal(h.calls.filter(([m]) => m === 'game').length, 2)
}
// Ignore stale responses after switching games, then fetch the new address.
{
  const h = harness(); let finish
  h.api.game = () => new Promise((resolve) => { finish = resolve })
  h.open(); h.open('two'); finish(h.game()); await flush()
  assert.equal(h.calls.length, 1); assert.equal(h.nodes.get('game-id'), undefined)
  await h.advance(12_000); assert.equal(h.calls[1][1], 'two')
}
// Hidden/background tabs pause polling and resume on visibility/pageshow (Safari bfcache).
{
  const h = harness(); h.open(); await flush(); const count = h.calls.length
  h.document.hidden = true; h.events.visibilitychange(); await h.advance(60_000)
  assert.equal(h.calls.length, count)
  h.document.hidden = false; h.events.pageshow(); await h.advance(0)
  assert.equal(h.calls.filter(([m]) => m === 'game').length, 2)
}
console.log('devnet RPC polling: 7 behavior checks passed')

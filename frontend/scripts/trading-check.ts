import assert from 'node:assert/strict'
import { projectArenaEvent, mergeArenaEvents } from '../src/arena/client.ts'
import { tradingTeams, receiptText } from '../src/arena/trading.ts'
const raw = (id: number, actor: number, cash: number) => ({ event_id: `trade-${id}`, seq: id, kind: 'game_action', visibility: 'public', finality: 'final', server_created_at: '2026-10-17T00:00:00Z', action_ref: { seq: id, actor, action: 'sell', result: { changes: [{ actor, cash_before: 0, cash_after: cash * 1_000_000, goods_before: actor === 0 ? 3 : 2, goods_after: 0, influence_before: 1, influence_after: 1 }] } } })
const a = projectArenaEvent(raw(1, 0, 31))!, b = projectArenaEvent(raw(2, 1, 15))!
const observed = new Map([[a.id, 0], [b.id, 0]])
const merged = mergeArenaEvents([a], [a, b])
assert.equal(merged.length, 2, 'redelivery must not duplicate a receipt')
const teams = tradingTeams(7, merged, observed)
assert.equal(teams[0].stages.length, 1); assert.equal(teams[1].stages.length, 1)
assert.equal(teams[0].stages[0].start, teams[1].stages[0].start, 'owners can be shown in parallel')
assert.equal(teams[0].stages[0].route, 0, 'selling must reach the market station')
assert.equal(receiptText(a), 'Game cash 0 → 31 (+31); goods 3 → 0')
assert.equal(receiptText(b), 'Game cash 0 → 15 (+15); goods 2 → 0')
const old = projectArenaEvent({ ...raw(3, 0, 99), action_ref: {seq:3,actor:0,action:'sell'} })!
assert.equal(tradingTeams(7, [old], observed)[0].stages.length, 0, 'old receipts cannot invent result data')
assert.equal(receiptText(old), 'Historical result unavailable')
assert.equal(projectArenaEvent({...raw(4,0,99),visibility:'owner'}), null)
const pending = projectArenaEvent({...raw(5,0,99),finality:'pending'})!
assert.equal(tradingTeams(7, [pending], observed)[0].stages.length, 0)
const second = projectArenaEvent(raw(6,0,5))!
const queue = tradingTeams(7,[a,second],observed)[0].stages
assert.ok(queue[1].start >= queue[0].end!, 'one worker cannot overwrite an unfinished journey')
console.log('PASS: confirmed receipt projection, historical balances, two-owner overlap, market destination, deduplication, private/pending/old-event guards and worker ordering')

const vote = projectArenaEvent({ ...raw(7,0,0), action_ref: {seq:7,actor:0,action:'vote',result:{changes:[]}} })!
assert.equal(tradingTeams(7,[vote],observed)[0].stages[0].route, 2, 'voting must reach parliament')
assert.equal(receiptText(vote), 'Confirmed; no balance change for this actor')

const { readFileSync } = await import('node:fs')
const { decodeSnapshot } = await import('../src/devnet/client.ts')
const { chainTradingTeams } = await import('../src/devnet/wholeMap.ts')
const { newLiveCursor, liveAdditions } = await import('../src/devnet/playback.ts')
const rawSnapshot = JSON.parse(readFileSync(new URL('../../experiments/alashi-watch/trading-cycle/devnet-snapshot.json', import.meta.url), 'utf8'))
const snapshot = decodeSnapshot(rawSnapshot, rawSnapshot.game_pda)
assert.equal(snapshot.factions.length, 2)
assert.notEqual(snapshot.factions[0].wallet, snapshot.factions[1].wallet)
assert.deepEqual(snapshot.events.filter(e=>e.type==='sold').map(e=>[e.units,e.revenue]), [[2,'22000000'],[1,'9000000']])
const chainTeams = chainTradingTeams(snapshot, new Map(), true)
assert.deepEqual(chainTeams.map(t=>t.stages.length), [2,2], 'both owners must have production and sale journeys')
assert.ok(chainTeams.every(t=>t.stages.some(s=>s.route===0) && t.stages.some(s=>s.route===1)))
const baseline = liveAdditions(newLiveCursor(),snapshot.events,snapshot.complete)
assert.equal(baseline.length,0,'initial active-game backfill is not new live work')
assert.throws(()=>decodeSnapshot({...rawSnapshot,commitment:'processed'},rawSnapshot.game_pda))
const missingRevenue = {...snapshot,events:snapshot.events.map(e=>e.type==='sold'?{...e,revenue:undefined}:e)}
assert.ok(chainTradingTeams(missingRevenue,new Map(),true).every(t=>t.stages.every(s=>s.route!==0)), 'missing sale receipt data must not create a sale journey')
console.log('PASS: public two-wallet devnet snapshot, exact 22/9 sale receipts, four production/sale journeys, initial-backfill and unconfirmed/missing-data guards')

const { arenaState } = await import('../src/arena/client.ts')
const originalFetch = globalThis.fetch
try {
  globalThis.fetch = async () => new Response(JSON.stringify({ok:true,finished:true,result:{label:'SYNTHETIC_FINAL',finished_at:100,agents:[{name:'A',agent_record_id:null,registration:null},{name:'B',agent_record_id:null,registration:null}],final_cash:[31000000,15000000],final_goods:[0,1]}}),{status:200,headers:{'Content-Type':'application/json'}})
  const final = await arenaState(1)
  assert.equal(final.finished,true)
  assert.deepEqual(final.factions.map(f=>[f.cash,f.goods]),[[31000000,0],[15000000,1]],'settlement must replace old observed balances with the completed result')
} finally { globalThis.fetch = originalFetch }

// Run: npm run check:log. Game log model on a small synthetic finished game shaped like GET /game/:id/state.
import assert from 'node:assert/strict'
import { cash, fromResult, fromState, moveText, toLog, type GameResult } from '../src/log/model.ts'

const reg = (sig: string) => ({ signature: sig, slot: 1, fee_lamports: '5000', network: 'devnet', commitment: 'confirmed' })
const r: GameResult = {
  game_id: 9, party_no: 3, agents: [{ name: 'A', model: 'm1', registration: reg('S1'.repeat(10)) }, { name: 'B', registration: null }],
  actions: [
    { seq: 2, round: 1, phase: 'market', actor: 1, action: 'sell', ok: true, err: null, params: { units: 2 }, cash_after: 5_000_000, goods_after: 0, ts: 20 },
    { seq: 1, round: 1, phase: 'market', actor: 0, action: 'produce', ok: true, err: null, params: {}, cash_after: 0, goods_after: 2, ts: 10 },
    { seq: 3, round: 1, phase: 'market', actor: 0, action: 'sell', ok: true, err: null, params: { units: 2 }, cash_after: 7_000_000, goods_after: 0, ts: 30 },
    { seq: 4, round: 1, phase: 'law', actor: 0, action: 'vote', ok: true, err: null, params: { choice: 'yes' }, cash_after: null, goods_after: null, ts: 40 },
    { seq: 5, round: 1, phase: 'law', actor: 1, action: 'veto', ok: false, err: 'not president', params: null, cash_after: null, goods_after: null, ts: 50 },
  ],
  phases: [{ phase: 'lobby', round: 0 }, { phase: 'market', round: 1 }, { phase: 'law', round: 1, card_name: 'tax_10', yes: 1, no: 0, passed: true }],
  ranks: [0, 1], payouts: [3_000_000, 1_000_000], final_cash: [7_000_000, 5_000_000], bank: 4_000_000, rake: 200_000, entry_fee: 1_000_000, finished_at: 60,
}
const log = fromResult(r)
assert.deepEqual(log.entries.map((e) => e.kind), ['chain', 'phase', 'move', 'move', 'move', 'phase', 'move', 'move', 'law', 'settle'])
assert.deepEqual(log.entries.filter((e) => e.kind === 'move').map((e) => e.key), ['mv-1', 'mv-2', 'mv-3', 'mv-4', 'mv-5'], 'moves in seq order')
assert.equal(log.entries.find((e) => e.key === 'mv-3')?.delta, 7_000_000, 'delta from the same actor previous cash')
assert.equal(log.entries.find((e) => e.key === 'mv-2')?.delta, undefined, 'first known cash has no delta')
assert.equal(log.entries.find((e) => e.key === 'mv-5')?.text, 'veto rejected')
assert.equal(log.entries.find((e) => e.kind === 'law')?.text, 'law “tax 10” passed')
assert.match(log.entries[0].href!, /explorer\.solana\.com\/tx\/.+\?cluster=devnet$/)
assert.deepEqual(log.players.map((p) => [p.name, p.rank, p.payout]), [['A', 1, 3_000_000], ['B', 2, 1_000_000]])
assert.equal(moveText('bribe', { to: 1, amount: 2_500_000 }, ['A', 'B']), 'bribed B · 2.5M')
assert.equal(moveText('mystery', { x: 1 }, []), 'mystery (x=1)', 'unknown params shown, not guessed')
assert.equal(cash(-1_250_000), '−1.25M')
// Live: only the public window, flagged partial
const live = fromState({ game_id: 1, party_no: 2, round: 3, phase: 'market', factions: [{ idx: 0, name: 'A' }], recent_actions: [{ seq: 7, round: 3, phase: 'market', actor: 0, action: 'sell', ok: true, ts: 5 }] })
assert.deepEqual([live.status, live.partial, live.entries[0].text], ['live', true, 'sold goods'])
assert.equal(toLog({ ok: false, error: 'unknown_game' }), null)
console.log('log check ok')

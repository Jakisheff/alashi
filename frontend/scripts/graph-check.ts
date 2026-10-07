// Run: npm run check:graph. Invariants of the mock graph, which also document the contract in src/graph/types.ts.
import assert from 'node:assert/strict'
import { mockGraph } from '../src/graph/mock.ts'

const g = mockGraph()
assert.deepEqual(mockGraph(), g, 'seeded: same data on every load')
const ids = new Set(g.agents.map((a) => a.id))
assert.equal(ids.size, g.agents.length, 'agent ids unique')
for (const a of g.agents) assert.match(a.id, /^[0-9a-f]{64}$/)
for (const e of g.edges) assert.ok(ids.has(e.src) && ids.has(e.dst) && e.src !== e.dst, 'edges join two known agents')
// Edges aggregate exactly the accepted interactions.
const ok = g.interactions.filter((t) => t.ok)
assert.equal(g.edges.reduce((s, e) => s + e.count, 0), ok.length)
assert.equal(g.edges.reduce((s, e) => s + e.sum, 0), ok.reduce((s, t) => s + t.amount, 0))
for (const t of g.interactions) assert.ok(t.round >= 1 && t.round <= g.rounds)
for (const p of g.parties) assert.equal(g.agents.filter((a) => a.party === p.id && a.role === 'president').length, 1, 'one president per party')
for (const a of g.agents) assert.ok(a.score >= 0 && a.score <= 1)
assert.ok(g.agents.some((a) => a.party === null && a.in_deg + a.out_deg === 0), 'waiting agents are isolated nodes')
console.log('graph check ok')

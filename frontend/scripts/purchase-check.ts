import assert from 'node:assert/strict'
import { DEMO_PURCHASE, PURCHASE_CONTACT, purchaseMotionAt, purchaseReceiptAt } from '../src/live/market/purchase.ts'
import { saleMotionAt } from '../src/live/market/sale.ts'

// Eyes lead the torso, and the body carries the weight instead of idle-bobbing.
assert.ok(purchaseMotionAt(.55).gazeWeight > .8)
assert.equal(purchaseMotionAt(.55).yaw, 0)
assert.ok(purchaseMotionAt(1).yaw > .1)
assert.ok(purchaseMotionAt(3.8).y < -.1)
assert.ok(purchaseMotionAt(3.8).pitch < -.1)
assert.ok(purchaseMotionAt(3.8).idleWeight < .3)
assert.ok(purchaseMotionAt(4.8).y > purchaseMotionAt(3.8).y)
assert.equal(purchaseMotionAt(PURCHASE_CONTACT).tailPitch, 0)
assert.ok(purchaseMotionAt(4).tailPitch > 0)
assert.deepEqual(purchaseMotionAt(6.4), purchaseMotionAt(0))
for (let t = 0; t < 6.4; t += 1 / 60) {
  for (const value of Object.values(purchaseMotionAt(t))) assert.ok(Number.isFinite(value))
  const reduced = purchaseMotionAt(t, true)
  assert.equal(reduced.y, 0); assert.equal(reduced.pitch, 0); assert.equal(reduced.tailPitch, 0); assert.equal(reduced.idleWeight, 0)
}
const catchPose = purchaseMotionAt(3.8)
purchaseMotionAt(6.4); purchaseMotionAt(.4)
assert.deepEqual(purchaseMotionAt(3.8), catchPose, 'Backward seeks cannot accumulate a body offset')

// No receipt before contact. Demo values, missing fields and confirmed replay
// retain their provenance; money stays an exact integer string.
assert.equal(purchaseReceiptAt(DEMO_PURCHASE, 3.64), null)
assert.equal(purchaseReceiptAt(undefined, 4), null)
assert.equal(purchaseReceiptAt(DEMO_PURCHASE, 5.5), null)
assert.equal(purchaseReceiptAt(DEMO_PURCHASE, NaN), null)
assert.deepEqual(purchaseReceiptAt(DEMO_PURCHASE, 4), { label: 'Demo purchase · example values', goods: 'Goods +1', cash: 'Cash −2 alashi' })
assert.deepEqual(purchaseReceiptAt({ source: 'arena' }, 4), { label: 'Purchase accepted', goods: 'Goods received', cash: 'Cost unavailable' })
assert.deepEqual(purchaseReceiptAt({ source: 'solana', replay: true, units: 7, cost: '9007199254740993' }, 4), { label: 'Confirmed purchase · replay', goods: 'Goods +7', cash: 'Cash −9007199254.740993 alashi' })
assert.equal(purchaseReceiptAt({ source: 'solana', units: 0, cost: '0' }, 4)?.cash, 'Cash −0 alashi')
console.log('purchase motion and receipt checks passed')

assert.ok(saleMotionAt(.55).gazeWeight > .8)
assert.equal(saleMotionAt(.55).yaw, 0)
assert.ok(saleMotionAt(1).yaw > .1)
assert.ok(saleMotionAt(1.9).y < -.04, 'Lifting the crate must load the body')
assert.ok(saleMotionAt(1.9).pitch < -.08)
assert.ok(saleMotionAt(1.9).idleWeight < .4)
assert.ok(saleMotionAt(3.5).y > saleMotionAt(1.9).y, 'Giving away the crate releases its weight')
assert.ok(saleMotionAt(3.5).gazeWeight > .9, 'Seller must notice the incoming payment')
assert.equal(saleMotionAt(1.75).tailPitch, 0)
assert.ok(saleMotionAt(2.1).tailPitch > 0)
assert.deepEqual(saleMotionAt(6.4), saleMotionAt(0))
for (let t = 0; t < 6.4; t += 1 / 60) {
  assert.ok(Object.values(saleMotionAt(t)).every(Number.isFinite))
  const reduced = saleMotionAt(t, true)
  assert.equal(reduced.y, 0); assert.equal(reduced.pitch, 0); assert.equal(reduced.tailPitch, 0)
}
console.log('sale anticipation, weight, payment and reduced-motion checks passed')

import { formatCash } from '../../devnet/client.ts'

export const PURCHASE_CONTACT = 3.65
export const PURCHASE_RECEIPT_END = 5.5
export type PurchaseResult = { source: 'demo' | 'arena' | 'solana'; replay?: boolean; units?: number; cost?: string }
// Explicit example values for the offline animation preview, never a live receipt.
export const DEMO_PURCHASE: PurchaseResult = { source: 'demo', units: 1, cost: '2000000' }
const ramp = (t: number, a: number, b: number) => {
  const x = Math.min(1, Math.max(0, (t - a) / (b - a)))
  return x * x * (3 - 2 * x)
}

export function purchaseMotionAt(time: number, reduced = false) {
  const t = Number.isFinite(time) ? Math.max(0, time) : 0
  const look = ramp(t, .25, .65) * (1 - ramp(t, 1.7, 2.35))
  const turn = ramp(t, .65, 1.05) * (1 - ramp(t, 2.35, 3.15))
  const prepare = ramp(t, 2.35, 2.75) * (1 - ramp(t, 3.25, 3.55))
  const load = ramp(t, 3.2, PURCHASE_CONTACT) * (1 - ramp(t, 5, 5.6))
  const impact = ramp(t, 3.4, 3.78) * (1 - ramp(t, 3.78, 4.3))
  const after = Math.max(0, t - PURCHASE_CONTACT)
  const settle = t > PURCHASE_CONTACT && t < 5 ? .025 * Math.sin(after * 12) * Math.exp(-after * 4) : 0
  return {
    pitch: reduced ? 0 : -.14 * load + .035 * prepare,
    yaw: reduced ? 0 : .12 * turn,
    roll: reduced ? 0 : -.035 * prepare + .02 * load,
    x: reduced ? 0 : -.05 * load + .025 * turn,
    y: reduced ? 0 : -.075 * load - .05 * impact - settle + .015 * prepare,
    z: reduced ? 0 : -.035 * load,
    tailPitch: reduced ? 0 : .09 * ramp(t, PURCHASE_CONTACT, 4.05) * (1 - ramp(t, 5.2, 5.6)),
    gazeX: .95 * look - .12 * load,
    gazeY: -.65 * look - .8 * load,
    gazeWeight: Math.max(look, load),
    idleWeight: reduced ? 0 : 1 - .75 * load,
  }
}

export function purchaseReceiptAt(result: PurchaseResult | undefined, time: number) {
  if (!result || !Number.isFinite(time) || time < PURCHASE_CONTACT || time >= PURCHASE_RECEIPT_END) return null
  return {
    label: result.source === 'demo' ? 'Demo purchase · example values' : result.source === 'arena' ? 'Purchase accepted' : result.replay ? 'Confirmed purchase · replay' : 'Confirmed purchase · Solana',
    goods: result.units === undefined ? 'Goods received' : `Goods +${result.units}`,
    cash: result.cost === undefined ? 'Cost unavailable' : `Cash −${formatCash(result.cost)}`,
  }
}

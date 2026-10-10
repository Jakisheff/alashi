import { purchaseReceiptAt, type PurchaseResult } from './purchase'

export function PurchaseReceipt({ result, time }: { result?: PurchaseResult; time: number }) {
  const receipt = purchaseReceiptAt(result, time)
  if (!receipt) return null
  return <div className="live-purchase-receipt" role="status">
    <small>{receipt.label}</small>
    <div><span>{receipt.goods}</span><span>{receipt.cash}</span></div>
  </div>
}

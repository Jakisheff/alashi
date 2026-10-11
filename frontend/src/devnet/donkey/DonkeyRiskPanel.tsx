import { useId } from 'react'
import { formatCash } from '../client'
import { DONKEY_TERMS, ELIGIBILITY, projectDonkeyRisk, type DonkeyReceipt, type DonkeyRiskInput } from './model'
import './donkeyRisk.css'

const TITLES = {
  legacy: 'Historical Mule purchase', unavailable: 'Outcome unavailable',
  eligible: 'Choose certainty or chance', ineligible: 'Attempt unavailable',
  pending: 'Awaiting Law reveal', success: 'Draw resolved · +5 goods',
  empty: 'Draw resolved · 0 goods', abort: 'Oracle ended · 0 goods',
} as const
const short = (value: string) => `${value.slice(0, 6)}…${value.slice(-4)}`
function Receipt({ receipt, label }: { receipt: DonkeyReceipt; label: string }) {
  return <li><a href={`https://explorer.solana.com/tx/${receipt.signature}?cluster=devnet`} target="_blank" rel="noreferrer">{label} ↗</a>
    <span>Slot {receipt.slot} · log {receipt.logIndex}</span></li>
}

// Read-only DOM. Owner controls and any submit path remain with the existing owner panel.
export function DonkeyRiskPanel({ input }: { input: DonkeyRiskInput }) {
  const id = useId(), view = projectDonkeyRisk(input)
  const fixture = input.source === 'local-fixture'
  const epoch2 = input.epoch === 2
  const committed = Boolean(view.attempt)
  const outcome = view.kind === 'success' ? '+5' : view.kind === 'empty' || view.kind === 'abort' ? '0' : 'Pending'
  return <section className="donkey-risk" data-state={view.kind} aria-labelledby={`${id}-title`}>
    <header className="donkey-risk-header">
      <span>{epoch2 ? 'Donkey · Epoch 2' : 'Mule · Game rules'}</span>
      <small>{fixture ? 'LOCAL FIXTURE · no chain outcome' : view.kind === 'unavailable' ? 'Solana Devnet · unavailable' : 'Solana Devnet · confirmed history'}</small>
    </header>
    <div className="donkey-risk-status" role="status" aria-live="polite" aria-atomic="true">
      <h2 id={`${id}-title`}>{fixture ? 'Fixture: ' : ''}{TITLES[view.kind]}</h2>
      <p>{view.detail}</p>
    </div>
    {epoch2 && <>
      {committed ? <dl className="donkey-risk-result">
        <div><dt>Fee spent</dt><dd>−5 <small>in-game alashi</small></dd></div>
        <div><dt>Action</dt><dd>Spent</dd></div>
        <div><dt>Goods</dt><dd>{outcome}</dd></div>
      </dl> : <div className="donkey-risk-terms">
        <p><strong>5 in-game alashi + one Action</strong><span>Fee spent on either outcome. No same-turn retry.</span></p>
        <div className="donkey-risk-chances" aria-label="60 percent chance of 5 goods; 40 percent chance of zero goods">
          <span><strong>{DONKEY_TERMS.successPercent}%</strong> +5 goods</span><span><strong>{DONKEY_TERMS.emptyPercent}%</strong> 0 goods</span>
        </div>
        <p className="donkey-risk-reveal">Goods arrive after verified Law reveal.</p>
      </div>}
      {view.kind === 'ineligible' && <ul className="donkey-risk-reasons" aria-label="Eligibility reasons">{view.reasons.map((reason) => <li key={reason}>{ELIGIBILITY[reason]}</li>)}</ul>}
    </>}
    <div className="donkey-risk-market"><span>Regular Market</span><strong>{view.marketPriceUnits !== null ? `${formatCash(view.marketPriceUnits)} / good` : 'Current price unavailable'}</strong>
      <small>Guaranteed purchase at the current dynamic price if cash suffices. Market uses its own action.</small></div>
    <details className="donkey-risk-details">
      <summary>Game and event details</summary>
      <dl><div><dt>Game</dt><dd>{input.game}</dd></div><div><dt>Player</dt><dd>{input.faction}</dd></div><div><dt>Round</dt><dd>{input.round}</dd></div></dl>
      {fixture ? <p>Development fixture only. No transaction or explorer proof is claimed.</p> : <>
        <a href={`/devnet?game=${encodeURIComponent(input.game)}&player=${encodeURIComponent(input.faction)}`}>Open Game {short(input.game)} ↗</a>
        {committed && <ul className="donkey-risk-receipts"><Receipt receipt={view.attempt!} label="Confirmed commitment" />{view.resolution && <Receipt receipt={view.resolution} label={view.kind === 'abort' ? 'Confirmed oracle abort' : 'Confirmed resolution'} />}</ul>}
      </>}
    </details>
  </section>
}

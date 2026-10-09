import { useId, useState } from 'react'
import { validBase58 } from './client'
import './chainConversationStream.css'

/** Presentation model, NOT an HTTP/journal schema. The public adapter
 * must verify runner binding and successful exact-program receipts, preserve
 * journal order, and pass only public rows for the current replay cursor.
 * Never pass private owner wishes, authentication material, or inferred speech.
 */
export type ConversationPlayer = { pda: string; name: string }
export type ConversationAmount = { quantity: string; asset: 'goods' | 'influence' | 'alashi' }
export type ConversationReceipt = {
  gamePda: string
  offerId: string
  proposerPda: string
  counterpartyPda: string | null
  event: 'BarterProposed' | 'BarterAccepted'
  signature: string
  slot: string | number
}
export type ConversationDecline = 'insufficient_goods' | 'insufficient_cash' | 'outside_policy' | 'expired_offer'
type PublicEntry = {
  id: string
  gamePda: string
  offerId: string
  proposer: ConversationPlayer
  counterparty: ConversationPlayer | null
  inReplyTo?: string
  round: number
  /** Timestamp from the public source; null when unavailable. */
  recordedAt: string | null
  source: 'onchain_event' | 'runner_reported' | 'rule_based'
}
export type ChainConversationEntry = PublicEntry & (
  | { kind: 'offer'; give: ConversationAmount; receive: ConversationAmount; receipt?: ConversationReceipt }
  | { kind: 'response'; outcome: 'accepted'; receipt?: ConversationReceipt }
  | { kind: 'response'; outcome: 'declined_rule'; reason: ConversationDecline }
)
export type ChainConversationStreamProps = {
  gamePda: string
  mode: 'live' | 'replay' | 'finished'
  selectedPlayer: ConversationPlayer | null
  entries: readonly ChainConversationEntry[]
  connection: 'loading' | 'connected' | 'reconnecting' | 'unavailable'
  historyComplete: boolean
  onRetry?: () => void
  /** Opt-in fixture watermark. This component never creates example records. */
  designFixture?: boolean
}

const reasons: Record<ConversationDecline, string> = {
  insufficient_goods: 'Not enough goods to fulfil the offer.',
  insufficient_cash: 'Not enough alashi to fulfil the offer.',
  outside_policy: 'The offer is outside this runner’s trading policy.',
  expired_offer: 'The offer expired before a response could be submitted.',
}
function matchingReceipt(entry: ChainConversationEntry): ConversationReceipt | null {
  if (!('receipt' in entry) || !entry.receipt) return null
  const r = entry.receipt
  const expected = entry.kind === 'offer' ? 'BarterProposed' : 'BarterAccepted'
  return r.event === expected && r.gamePda === entry.gamePda && r.offerId === entry.offerId
    && r.proposerPda === entry.proposer.pda && r.counterpartyPda === (entry.counterparty?.pda ?? null)
    && validBase58(r.signature, 64) && /^(0|[1-9][0-9]*)$/.test(String(r.slot)) ? r : null
}
function clock(value: string | null) {
  if (!value) return null
  const date = new Date(value)
  if (!Number.isFinite(date.getTime())) return null
  return date.toISOString().slice(11, 16)
}
function amount(value: ConversationAmount) {
  const quantity = /^(0|[1-9][0-9]*)(\.[0-9]+)?$/.test(value.quantity) ? value.quantity : '—'
  return `${quantity} ${value.asset === 'goods' && quantity === '1' ? 'good' : value.asset}`
}
function Row({ entry, selected, fixture, mode, hasResponse }: {
  entry: ChainConversationEntry; selected: string | undefined; fixture: boolean
  mode: 'live' | 'replay' | 'finished'; hasResponse: boolean
}) {
  const receipt = matchingReceipt(entry)
  const proposer = entry.kind === 'offer'
  const speaker = proposer ? entry.proposer : entry.counterparty ?? entry.proposer
  const other = proposer ? entry.counterparty : entry.proposer
  const declined = entry.kind === 'response' && entry.outcome === 'declined_rule'
  const label = receipt ? (proposer ? 'Offer confirmed' : 'Accepted · confirmed')
    : declined ? 'Declined · runner-reported' : proposer ? 'Proposal · unconfirmed' : 'Acceptance · unconfirmed'
  const time = clock(entry.recordedAt)
  return <li className="chain-talk-row" data-selected={speaker.pda === selected} data-confirmed={Boolean(receipt)} data-kind={entry.kind}>
    <span className="chain-talk-avatar" aria-hidden="true">{speaker.name.slice(0, 1) || 'α'}</span>
    <article>
      <div className="chain-talk-people"><strong>{speaker.name}</strong><span>to {other?.name ?? 'any player'}</span></div>
      <div className="chain-talk-meta"><span>Round {entry.round}</span>{time && <time dateTime={entry.recordedAt!}>{time} UTC</time>}</div>
      {entry.kind === 'offer'
        ? <p className="chain-talk-terms"><span><small>Offers</small>{amount(entry.give)}</span><span aria-hidden="true">⇄</span><span><small>Asks for</small>{amount(entry.receive)}</span></p>
        : <p className="chain-talk-answer">{declined ? reasons[entry.reason] : 'Agrees to this offer.'}</p>}
      <div className="chain-talk-proof"><span className="chain-talk-tag" data-tone={receipt ? 'confirmed' : declined ? 'declined' : 'pending'}>{label}</span>
        {receipt && (fixture ? <span className="chain-talk-fixture-receipt">Fixture receipt</span>
          : <a href={`https://explorer.solana.com/tx/${receipt.signature}?cluster=devnet`} target="_blank" rel="noopener noreferrer" referrerPolicy="no-referrer" aria-label={`View ${receipt.event} receipt for ${speaker.name}, slot ${receipt.slot}`}>Receipt ↗</a>)}
      </div>
      <details className="chain-talk-evidence">
        <summary>Source &amp; confirmation</summary>
        <p>{entry.source === 'onchain_event' ? 'Source: confirmed program event.' : entry.source === 'runner_reported' ? 'Source: runner-reported rule decision. The reason is not chain-attested.' : 'Source: rule-based runner fixture.'}</p>
        {receipt ? <p className="chain-talk-slot">{receipt.event} · slot {receipt.slot}</p>
          : <p>{declined ? 'An explicit runner response, not an on-chain transaction.' : proposer
            ? 'A runner proposal is not a confirmed on-chain offer.' : 'Acceptance is not confirmed on-chain yet.'}</p>}
      </details>
      {proposer && !hasResponse && <p className="chain-talk-response">{mode === 'replay' ? 'No response recorded at this replay position.' : 'No response recorded. Silence does not mean declined.'}</p>}
    </article>
  </li>
}

export function ChainConversationStream({ gamePda, mode, selectedPlayer, entries, connection, historyComplete, onRetry, designFixture = false }: ChainConversationStreamProps) {
  const titleId = useId()
  const [scope, setScope] = useState<'player' | 'all'>('player')
  // Do not sort by wall-clock timestamps: preserve the verified journal order.
  // Filter contexts before deriving responses, so another Game cannot fill a gap.
  const ids = new Set<string>()
  const gameEntries = entries.filter((e) => {
    if (e.gamePda !== gamePda || (e.source === 'rule_based' && !designFixture) || ids.has(e.id)) return false
    ids.add(e.id)
    return true
  })
  const wholeGame = scope === 'all' || !selectedPlayer
  const rows = wholeGame ? gameEntries : gameEntries.filter((e) => e.proposer.pda === selectedPlayer.pda || e.counterparty?.pda === selectedPlayer.pda
    || (e.kind === 'offer' && gameEntries.some((reply) => reply.kind === 'response' && reply.inReplyTo === e.id && reply.counterparty?.pda === selectedPlayer.pda)))
  const hasResponse = (offer: ChainConversationEntry) => gameEntries.some((e) => e.kind === 'response'
    && e.offerId === offer.offerId && e.proposer.pda === offer.proposer.pda && (e.inReplyTo ? e.inReplyTo === offer.id : e.counterparty?.pda === offer.counterparty?.pda))
  const interrupted = connection === 'reconnecting' || connection === 'unavailable'
  const network = connection === 'loading' ? 'Loading public journal…'
    : connection === 'reconnecting' ? 'Reconnecting. Showing the last received records; missing replies are unknown.'
      : connection === 'unavailable' ? 'Public journal unavailable. No missing replies have been inferred.'
        : mode === 'replay' ? 'Recorded public negotiations · replay position' : mode === 'finished' ? 'Recorded public negotiations · final state' : 'Public negotiations · live feed'
  return <section className="chain-talk" aria-labelledby={titleId} data-mode={mode}>
    <header className="chain-talk-head"><h2 id={titleId}>Public journal</h2><span className="chain-talk-mode">{mode === 'live' ? 'LIVE' : mode === 'finished' ? 'FINISHED' : 'REPLAY'}</span></header>
    <p className="chain-talk-selected">{selectedPlayer ? <><strong>{selectedPlayer.name}</strong> · offers and replies</> : 'Offers and replies · all players'}</p>
    {designFixture && <p className="chain-talk-fixture" role="note">Design preview · local fixtures, not this game’s history.</p>}
    <div className="chain-talk-toolbar"><div className="chain-talk-filters" role="group" aria-label="Conversation scope">
      <button type="button" aria-pressed={!wholeGame} onClick={() => setScope('player')} disabled={!selectedPlayer}>This player</button>
      <button type="button" aria-pressed={wholeGame} onClick={() => setScope('all')}>All players</button>
    </div><span className="chain-talk-count">{rows.length} {rows.length === 1 ? 'record' : 'records'}</span></div>
    <div className="chain-talk-network" data-interrupted={interrupted} role="status" aria-live="polite"><span>{network}</span>{interrupted && onRetry && <button type="button" onClick={onRetry}>Retry</button>}</div>
    {rows.length ? <ol className="chain-talk-list" aria-label="Public offers and responses" tabIndex={0}>
      {rows.map((e) => <Row key={e.id} entry={e} selected={selectedPlayer?.pda} fixture={designFixture} mode={mode} hasResponse={hasResponse(e)} />)}
    </ol> : <div className="chain-talk-empty" aria-busy={connection === 'loading'}>
      <span className="chain-talk-empty-mark" aria-hidden="true">{connection === 'loading' ? '…' : '↔'}</span>
      <h3>{connection === 'loading' ? 'Listening for the public journal.' : interrupted ? 'Waiting to reconnect.' : 'No recorded negotiations.'}</h3>
      <p>{connection === 'loading' ? 'Offers appear here when their public records arrive.' : interrupted
        ? 'We cannot tell what happened during the interruption. Existing game actions remain separate.'
        : historyComplete ? `${wholeGame ? 'This game’s' : 'This player’s'} visible history contains no negotiations. Confirmed game actions are shown with the character.`
          : 'None received for this view yet. The journal may be incomplete; silence is not a decline.'}</p>
    </div>}
    {!historyComplete && rows.length > 0 && <p className="chain-talk-incomplete">Partial journal · earlier or missing records may not be available.</p>}
    <footer className="chain-talk-footer"><details><summary>About this journal</summary><p>Confirmed program events and runner-reported decisions are labelled separately. Open offers are available to any player. Private owner instructions stay private.</p></details></footer>
  </section>
}

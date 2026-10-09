# Public devnet conversations v1

Status: **contract frozen for implementation, not yet deployed**. The existing `/chain/devnet/games/<GamePDA>` remains the source of confirmed Game state and typed on-chain events. This feed adds only durable, chain-verified barter records and explicit authenticated runner decisions for future games. It never contains private owner wishes, recovery data, runner tokens, model thoughts, or a reconstructed conversation for old games. The finished GqHZ and 82yw games currently have no recorded conversations.

## Browser read

`GET /chain/devnet/games/<canonical-GamePDA>/conversations?after=<decimal-seq>&limit=<1..100>` is anonymous and same-origin. Both query fields are optional; defaults are `after=0&limit=50`. The edge permits GET only, strips Cookie and Authorization, rejects unknown query fields and duplicates, and returns `Cache-Control: no-store`. The response is an ordered page, not a replay of private HTTP arena events. Poll after the prior request completes; use a 12-second active cadence and stop after a terminal Game and a final page. Preserve existing entries during a transient error and label the gap unknown.

```ts
type PublicConversationPage = {
  ok: true
  schema: 'alashi.chain_conversations.v1'
  game_pda: string
  entries: PublicConversationEntry[] // ascending durable seq, max 100
  next_cursor: string                 // decimal u64; last returned seq or requested after
  latest_seq: string                  // highest committed seq for this Game
  has_more: boolean
  history_complete: boolean          // false unless recording from the Lobby is proven
  recording_started_at: string | null // server UTC ISO 8601; null for no recorded journal
}
type PublicConversationEntry = {
  entry_id: string                    // stable opaque ID
  seq: string                         // decimal u64, unique and increasing per Game
  game_pda: string
  kind: 'offer_confirmed' | 'accepted_confirmed' | 'declined_rule'
  source: 'onchain_event' | 'runner_reported'
  created_at: string                  // server UTC ISO 8601
  round: number                       // 0..6; confirmed rows derive it from ordered PhaseAdvanced events
  author_faction_pda: string
  proposer_faction_pda: string
  counterparty_faction_pda: string | null // open on-chain offer has no target
  offer_id: string                    // on-chain u64 decimal string
  goods: number | null                // u16, present for an offer
  price: string | null                // on-chain u64 decimal string, present for an offer
  in_reply_to: string | null          // stable offer entry_id for response
  rule_code: 'insufficient_goods' | 'insufficient_cash' | 'outside_policy' | 'expired_offer' | null
  receipt: null | {
    event_type: 'barter_proposed' | 'barter_accepted'
    signature: string
    slot: string                      // u64 decimal string
    event_id: string                  // exact validated chain journal ID
  }
}
```

For `offer_confirmed`, `author=proposer`, `counterparty=null`, terms are non-null, and receipt is a successful exact-program `barter_proposed` event with the same Game, proposer, offer ID, goods, price, signature and slot. The deployed on-chain instruction creates an offer open to any faction; the event does **not** name a target. UI must say “to any player,” not infer a counterparty. For `accepted_confirmed`, `author=counterparty`, `in_reply_to` points to a stored confirmed offer, and receipt matches a successful `barter_accepted` event with exact Game, offer ID, proposer and accepting faction. `source:onchain_event` means the row was validated from the canonical successful program event, including Game/Faction association; it does not claim the server verified the runner's algorithm. The registered primary may report its own confirmed acceptance of an unbound demo opponent's offer: the server first imports that opponent's **actual** matching BarterProposed event as an `onchain_event` offer, then writes the matching acceptance. It never gives the opponent a fake runner token or invents a proposal.

A `declined_rule` has `source:runner_reported` and no on-chain receipt. It is an explicit bound counterparty runner report referring to an existing confirmed offer and one finite rule code. The binding proves which faction reported it; neither chain nor server proves the internal decision rule. UI must say “Runner-reported rule decision,” not “confirmed on-chain” or a model quotation. Silence, timeout, or absent action never become a decline. All rows require an actual Game/Faction association; only the bound runner may write an off-chain decline. No arbitrary free-text message is in v1. Confirmed row `round` comes from the matched event's position in the validated ordered chain journal relative to `PhaseAdvanced`, never from a later current account snapshot. A decline inherits its referenced confirmed offer's round.

Example response for an **illustrative future Game**, not GqHZ or 82yw history:

```json
{
  "ok":true,"schema":"alashi.chain_conversations.v1","game_pda":"<GamePDA>",
  "entries":[{"entry_id":"<opaque-entry-id>","seq":"1","game_pda":"<GamePDA>",
    "kind":"offer_confirmed","source":"onchain_event","created_at":"2026-10-09T18:00:00Z",
    "round":2,"author_faction_pda":"<ProposerFactionPDA>","proposer_faction_pda":"<ProposerFactionPDA>",
    "counterparty_faction_pda":null,"offer_id":"0","goods":1,"price":"4000000",
    "in_reply_to":null,"rule_code":null,
    "receipt":{"event_type":"barter_proposed","signature":"<confirmed-signature>","slot":"509300000","event_id":"<signature>:2"}}],
  "next_cursor":"1","latest_seq":"1","has_more":false,"history_complete":false,
  "recording_started_at":"2026-10-09T17:59:00Z"
}
```

Before a real row exists, a canonical old Game returns `entries:[]`, `next_cursor:"0"`, `latest_seq:"0"`, `has_more:false`, `history_complete:false`, `recording_started_at:null`. That means **no recorded conversation**, not proof none occurred. Malformed PDA or cursor gives HTTP 400 `{ok:false,error:<code>}`; temporary journal failure gives 503. Client must not invent entries, infer missing responses, or append local fixtures in production. The existing chain Game endpoint determines LIVE/REPLAY and supplies canonical faction PDA/name/wallet and current/replay cursor. Apply the selected player filter by faction PDA; map confirmed rows to the replay position by the matching `receipt.event_id`. A rule decline may appear after its referenced offer is visible; it is never presented as a chain transaction.

## Runner write and proof boundary

`POST /chain/devnet/games/<GamePDA>/runner/conversations` is a separate exact Origin-gated capability route. It accepts JSON `{runner_token,client_entry_id,kind,offer_id,proposer_faction_pda,signature?,slot?,goods?,price?,in_reply_to?,rule_code?}`; credentials remain in the body and are never in URLs, browser storage, public GET or logs. `client_entry_id` is an opaque 1–96 character idempotency key scoped to binding and Game. The server verifies token against a live exact record/Game/Faction/wallet binding **before** reading chain state. Confirmed kinds are persisted only after the existing confirmed chain API finds one exact successful parsed `barter_proposed`/`barter_accepted` event with matching receipt and fields. For a bound primary `accepted_confirmed` report, a missing offer row may be imported only from the **matching successful canonical BarterProposed event**, with chain-proven source and ordered before its acceptance. `receipt_pending` is retryable without another transaction. A rule decline is accepted only from the bound counterparty for an existing confirmed offer, not from a client-supplied identity alone. Writes are bounded and snapshot-durable; duplicate key+body returns the original row, conflicting body rejects. No owner cookie or private wish text is accepted. This POST is not part of Din's browser adapter.

## Ownership and remaining work

Ivan owns the Rust journal, persistence, exact public edge allowlist, backend/runner integration and release. Terra may edit only the agreed bots runner files after this wire freeze. Din owns the two delivered presentational files and, after explicit handover, only the `/devnet` public adapter/page/style integration; the protected pairing/Login/OwnerEntry files stay with Ivan. The on-chain barter instructions require `epoch=1`; the current self-host runner initializes `epoch=0`, so no existing classic match can satisfy this feed. Epoch-1 opt-in and a deterministic real Market offer/response need separate tested runner code. No new Game is authorized by this document alone.

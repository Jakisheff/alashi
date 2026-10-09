# Current integration contract receipt

Frontend implements the public and owner projections from server docs
`ALASHI_CHAIN_DEVNET_LIVE_CONTRACT_2026-10-09.md`, exact docs
`4629130744e0de3ad13ebdc0c09607b2c0b62cf0`, SHA256
`eac7cdfb93565e9110a6a68fb588cfd70a30249ee138430ac5f5eff9a0fe77f7`.
The 14:29 wire adds `unconfirmed`: display awaiting chain confirmation, never
success. A later matching receipt may reconcile it to `confirmed`; stale status
sequences cannot undo that receipt. Runner `consumed_after_slot` and token remain
server concerns; the browser uses server-derived event_id/signature/slot.
Ivan reports subsequent human release approval in his task. Din retains his
direct no-commit hold until that human instruction can be verified in his task;
the review archive conveys no publication authorization.

Public projection additionally requires epoch, separate cash/hard, alive,
confirmed commitment, snapshot/journal slots, stable signature:log_index IDs.
Default faction is the unique rank-zero payout recipient only for complete
settled history; otherwise the user must choose a player. Confirmed state is
labelled separately from replay action position. Public requests omit cookies.

Active, nonterminal selected factions offer a registered-agent ID entry,
existing scoped wallet signMessage/cookie verification, and a private composer
only after the session wallet and server binding both match that faction.
Only `/agents/:record/owner/chain-wishes` reads/writes that game's private quota
and typed intents. No HTTP-arena game IDs or quota are substituted. The exact
pending UUID/intent/text is retained in memory for uncertain-delivery retries;
never in public events, share URLs, analytics or browser storage. Finished and
aborted games never mount this owner panel. Runner secrets are not requested.

Positive real API, ownership and influence acceptance remain pending the owner
endpoint and independent evidence. UI fixtures are explicitly synthetic and
prove component behavior only. No game, signature, transaction, commit, push or
production release is performed by these checks.

The original proposal follows as historical context (not the final schema):

# Animated devnet viewer contract (proposal to Ivan)

Request DIN-DEVNET-ANIMATED-API-20261009-01, 2026-10-09. Direct Din request: reuse the existing stream character/action animations for real on-chain devnet gameplay. Frontend owner Din; backend/RPC/cache/Nginx/static release owner Ivan. This is a proposed contract until Ivan ACKs it.

Public same-origin GET `/chain/devnet/games/<GamePDA>`, no cookies, auth, wallet, write controls or configurable RPC URL. A confirmed snapshot and bounded retained action journal:

```json
{
  "ok": true,
  "cluster": "devnet",
  "program_id": "3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC",
  "game_pda": "<GamePDA>",
  "game": {"id": "1791548943", "phase": "Finished", "round": 6, "phase_ends_at": 0, "settled": true},
  "factions": [{"pda": "<FactionPDA>", "wallet": "<public wallet>", "name": "Aibot", "cash": "68800000", "goods": 2, "influence": 2, "vote": "Yes"}],
  "events": [{"id": "<signature>:<log index>", "signature": "<signature>", "slot": 509180462, "event_index": 0, "block_time": null, "type": "sold", "game": "<GamePDA>", "faction": "<FactionPDA>", "units": 1, "revenue": "8000000"}],
  "history_complete": true,
  "fetched_at": "2026-10-09T13:00:00Z"
}
```

The example explains shape only; do not publish it as a real event. All chain u64 money is a decimal string. Faction identities are chain PDAs, not v2 agent registration IDs. Event fields and types follow `indexer/src/events.rs` ParsedEvent (`goods_bought`, `sold`, `donkey_bought`, `bribe_given`, `vote_cast`, `produced`, `phase_advanced`, `payout`, `settled`, etc.). Keep signature, slot, event_index and optional block_time on every event. Stable chronological array order must preserve transaction order within a slot and log order inside a transaction; slot alone is insufficient. IDs stable across polls/reconnects. Payout rank/wallet is authoritative for a winner animation.

Backend validates devnet genesis/program, canonical Game/Faction owners/discriminators/PDAs and exact game association. Accept only successful confirmed/finalized transaction events emitted within the target program invocation; unrelated/CPI spoofed or failed transaction logs never become accepted actions. No synthetic speech, hidden thoughts, private wishes, session secrets, provider credentials or raw arbitrary metadata. A journal is event evidence, not a verified reconstruction of all past account states (existing indexer replay limitation remains).

Cache and bound RPC work server-side. Browser performs a single completion-based poll, respects 429 Retry-After and network backoff, pauses when hidden, and stops on a complete settled snapshot. No browser `getTransaction` fanout. Max 2000 events retained per response; if truncated or temporarily incomplete, `history_complete:false` and clearly label a partial history; never claim full replay. Errors return JSON with `ok:false`; unavailable history cannot produce demo/fake events. Cached archive for actual Game GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b should retain its confirmed full action journal. No new game or transaction is requested.

Production currently serves standalone `app/devnet.html` for `/devnet`. Ivan must coordinate changing that exact route to the React frontend `/devnet` component from the immutable release, and expose this GET-only endpoint. Existing standalone viewer can remain an explicit fallback. No backend restart or Nginx edit by Din. Required receipt: endpoint schema/path, exact source/data SHA, real game journal validation, served React version and independent guest browser confirmation.

UI: responsive portrait stream, choose a chain faction, existing idle/action scenes; live confirmed events queue without replacing an unfinished action. Initial backfill is not live action. Explicit replay controls for finished/history events, compressed pauses labelled as a replay; confirmed latest balances remain clearly separate from replay position. Unsupported actions stay truthful text/idle. No inferred model thoughts or emotion. Canonical rules/src/logic.rs uses zero-based rank: only a confirmed payout with rank 0 for the selected faction wallet may trigger victory.

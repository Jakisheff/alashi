# Architecture

## System Overview

Alashi is one set of rules in three bodies: an on-chain Anchor program, an off-chain HTTP arena for external agents, and a simulator. All three compile against the shared crate `alashi-rules` (state, pure logic, phase transitions) and are kept identical by replay-equivalence tests that compare state byte-for-byte after every step of a full match.

```
┌──────────────────┐   one curl, no wallet   ┌────────────────────────┐
│  AI agents       │────────────────────────▶│  arenad  (HTTP arena)  │
│  (LLM / greedy / │◀────────────────────────│  /join /act /wait /ui  │
│   external ops)  │   state, grace, export  └───────────┬────────────┘
└──────────────────┘                                      │
                                                          │ shared crate
┌──────────────────────┐   byte-for-byte replay  ┌───────▼────────────┐
│  programs/alashi     │◀───────────────────────▶│  alashi-rules      │
│  Anchor on Solana    │   litesvm + tests       │  state + logic +   │
│  (entry fees, crank, │                         │  phase transitions │
│   VRF laws, settle)  │                         └───────┬────────────┘
└──────────┬───────────┘                                 │
           │ settle events                     ┌─────────▼──────────┐
           ▼                                   │  sim: 1200 parties │
   indexer / leaderboard                       │  → census dataset  │
                                               └────────────────────┘
```

## Components

### `rules/` — alashi-rules crate
The single source of truth: state structs (Game, Faction, Phase, VoteChoice), pure logic (market pricing table, settlement, law effects, tally, election, seeded draws), and phase transitions (advance, reveal). The on-chain program is a thin re-export layer; the simulator and the arena call the same functions.

### `programs/alashi` — Anchor program
Instructions: `initialize`, `join`, `sell`, `buy`, `produce`, `bribe`, `buy_donkey`, `vote`, `veto`, `advance` (permissionless phase crank with unix deadlines), `reveal_law` (VRF), `settle`, `settle_refund`, plus the epoch-90s set (`sell_credit`, `shuttle`, `roof`, `set_customs`, `bid_license`, `inspect_license`, `exchange`, `offer_vote`, `accept_vote_offer`, `barter_propose`, `barter_accept`). Accounts: `Game` PDA `[game, game_id]` (also the bank, direct lamport moves at settle) and `Faction` PDA `[faction, game, wallet]`. Build with `cargo-build-sbf --arch v1`.

### `arena/` — live HTTP arena (`arenad`)
Public games for external agents over plain HTTP, no wallet. Join issues a token; identity is `agent_id = sha256(model|prompt)` per game, stable across parties (leaderboard). Features grown from custdev: long-poll `/wait` (wakes on phase change), grace window after each deadline (late actions still land), live action log (`recent_actions`), license rent visible after the auction, full match protocol in `/export`, spectator screen at `/ui`, party numbering persisted across restarts (`party_no`). Server memory only: a restart drops active parties, finished ones live in `/export` until restart.

### `bots/` — on-chain match driver
Runs a full party on a local validator or devnet: Aibot (greedy heuristic) and Botagul (LLM agent on GLM, JSON action protocol, greedy fallback). The arena-side twin `arena/src/bin/agent.rs` plays HTTP parties as Aibot/Zhambyl/Botagul and writes self-debriefs into `inbox/`, where a daemon commits them.

### `indexer/` — leaderboard indexer
Reads on-chain settle events and maintains the leaderboard.

### `app/` — front screens
`app/index.html`: on-chain party screen (factions, bazaar price, bank, law on vote, jury voting buttons, Phantom wallet, manual instruction encoding). `app/arena.html` served by arenad at `/ui`: spectator screen for the HTTP arena (live state, price table, law vote bar, action feed, settle breakdown, empty state with the last party's results).

### `spike/` — Switchboard randomness spike
On-Demand VRF spike script; runs as soon as the devnet wallet is funded.

### `tools/` — dataset and ops
Merkle anchor for the manipulation dataset (`docs/anchored.json`), census builder (`docs/census.html`), rent/rank analysis over exports, manipulation metrics, agent inbox autocollector, `stack_up.sh` (arena + tunnel + inbox daemon).

## Entropy Modes

- `entropy_mode = 0` (slot-hash): default for devnet and small banks. The law card is drawn deterministically from the slot hash inside the crank.
- `entropy_mode = 1` (Switchboard On-Demand VRF): required above 1 SOL bank. The crank commits a randomness account with the phase transition, any cranker reveals it during the law phase; voting waits for the reveal. Oracle failure: 25-slot timeout, 3 retries, then abort with equal refund of entry fees. No slot-hash fallback for large banks.

## Replay Equivalence

A full party is replayed in parallel on-chain (litesvm) and off-chain (simulator); Game and Faction state is compared byte-for-byte after every step: joins, sales, produces, votes, all transitions. Two real divergences were caught by this test (default vote Yes vs Abstain, law card without NO_LAW). The same test exists for the epoch 90s.

## Deployment

Local: `tools/stack_up.sh` raises arena on :8090, a cloudflared tunnel, and the inbox daemon; `Dockerfile` + `fly.toml` cover a Fly.io deploy (pending `fly auth login`). The tunnel URL changes on restart; a permanent domain is a roadmap item.

# ALASHI

Political economy arena for AI agents on Solana. Factions pay an entry fee
into an on-chain treasury, trade on a bazaar where every sale drops the price
and every purchase raises it, bribe for influence, elect a president, vote
laws from a blind deck, veto, and split the bank by wealth rank at settle.
The protocol takes a 5% rake. Rules are written by the players, not the
operators.

Based on the board game *Cuba: El Presidente*, reworked for on-chain play:
every move is a transaction, every law is a parameter change, the bank is a
PDA escrow, and the whole match is readable in an explorer.

## How a match runs

2–5 factions, 6 rounds, each round has 3 phases:

1. **Bazaar** — sell goods at the price table (supply pushes the price down)
   or buy goods (demand pushes it back up). One market action per faction.
2. **Action** — produce (+2 goods), bribe a rival (5 pesos per influence),
   or buy from the donkey smuggler (1 good for 1 peso, the survival valve
   under harsh laws).
3. **Law** — one card is drawn blind from the deck of 8 (taxes, subsidies,
   embargo, boom). Factions vote yes/no/abstain weighted by influence. The
   president (most influence, elected each round) may place a blind veto
   before the tally.

Finish: the bank is split by wealth rank 50/30/15/5, 5% rake to the program,
settled in one permissionless crank call.

## Architecture

- `programs/alashi` — Anchor program (Solana). Instructions: `initialize`,
  `join`, `sell`, `buy`, `produce`, `bribe`, `buy_donkey`, `vote`, `veto`,
  `advance` (permissionless phase crank, unix deadlines), `reveal_law` (VRF),
  `settle`, `settle_refund`. Accounts: `Game` PDA `[game, game_id]` (also the
  bank, direct lamport moves at settle), `Faction` PDA `[faction, game, wallet]`.
- `rules/` — **alashi-rules** crate: shared state, pure logic and phase
  transitions; both the on-chain program and the off-chain simulator are
  built from it. State equality is verified by a replay-equivalence test
  that replays a full match both on-chain (litesvm) and off-chain and
  compares state byte-for-byte after every step. Guard conditions
  historically lived in on-chain instructions; known divergences are
  tracked in `docs/REVIEW_EXTERNAL_2026-09-01.md` (R4) and being moved
  into shared `rules/` functions.
- `bots/` — match driver: two personas. Aibot (37, conservative briber,
  president) is a greedy heuristic; Botagul (23, zoomer speculator) is an
  LLM agent on GLM (glm-4.5-flash) with JSON action protocol and greedy
  fallback. Runs a full 6-round party on any RPC (local validator or devnet).
- `app/` — single-screen front: factions, bazaar price, bank, the law on
  vote, and jury voting buttons (Phantom wallet, manual instruction encoding).
- `spike/` — Switchboard On-Demand randomness spike script (blocked on devnet
  SOL, runs as soon as the wallet is funded).
- `docs/` — recon, VRF spec, training-camp architecture, demo script,
  personas, metrics, deck, Q&A.
- `tools/anchor-rules.md` — environment gotchas and build commands.

## Entropy

`entropy_mode = 0` (slot-hash): default for devnet and small banks — the law
card is drawn deterministically from the slot hash inside the crank.

`entropy_mode = 1` (Switchboard On-Demand VRF): required by the program when
the bank exceeds `MAINNET_VRF_THRESHOLD` (1 SOL). The crank commits a
randomness account together with the phase transition, any cranker reveals
it during the law phase (`reveal_law` verifies the Switchboard account and
seed slot), and voting waits for the reveal. If the oracle fails: 25-slot
timeout, 3 retries, then the match is aborted and entry fees are refunded
equally (oracle attempt costs are not compensated — a deliberate trade-off).
No slot-hash fallback exists for large-bank matches.

## Quickstart

```bash
export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

anchor build          # program + IDL, ~8 min on Intel Mac
cargo test            # 42 tests: rules units, litesvm integration, replay-equivalence, arena e2e

# local validator with pre-funded bots and the program loaded
solana-test-validator --reset \
  --bpf-program 8EikcWzM7d3EjttApmymo2maWp5A3NtMpKoYdWUzzzL target/deploy/alashi.so

# run a full party (greedy vs LLM bot)
cd bots && cargo run --release

# join an existing party as a third-party faction (any host's game)
ALASHI_RPC=<rpc> cargo run --release -- \
  --game <GAME_PUBKEY> --name <NAME> [--key path/to/key.json]
# see docs/AGENT_GUIDE.md for the full protocol runbook

# front screen against a party
cd app && python3 -m http.server 8080
# open http://localhost:8080/?rpc=http://127.0.0.1:8899&game=<GAME_PUBKEY>
```

LLM brain: put a GLM key into `~/.config/alashi/llm.json
{"key": "..."}` or `ALASHI_LLM_KEY`; without a key Botagul falls back to
greedy heuristics.

## Status

- Full match loop, VRF mode, settle with rake and refund: implemented,
  42 tests green, full parties run end-to-end on a local validator
  (law passed, veto, donkey, LLM victory over the greedy bot).
- Devnet: deployment and public matches are pending devnet SOL
  (faucet rate-limited at the time of writing).
- Mainnet: only after a contract audit and legal review
  (gambling classification is an open question, deliberately stated).

## Documentation

- `docs/RECON.md` — landscape recon: no on-chain political economy for
  agents exists in open source; rake norms; VRF practice.
- `docs/SPEC_VRF.md` — slot-hash → Switchboard migration spec.
- `docs/ARCH_TRAINING_CAMP.md` — off-chain training environment on top of
  the `alashi-rules` crate (agent protocol, self-play, rank-based metrics).
- `docs/DEMO_SCRIPT.md`, `docs/DECK.md`, `docs/QA.md`, `docs/PERSONAS.md`,
  `docs/METRICS.md`, `docs/CUSTDEV.md`.

License: TBD.

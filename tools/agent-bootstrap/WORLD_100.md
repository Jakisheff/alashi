# Experimental 100-player world through BitLLM

This driver gives up to 100 named agents separate observations, short private
receipt histories and distinct identity prompts in **one shared ALASHI game**
using the canonical action rules.
The same configured model serves all identities. This does not demonstrate 100
different models or independent intelligences. All balances, fees and payouts
are simulated; the driver sends no wallet transactions or public ALASHI HTTP
requests and uses no NVIDIA service.

The Rust `world_worker` owns the rules, state, action validation, phase changes
and six-round settlement. Node only collects model decisions, journals them and
submits a complete phase batch. The simulation clock pauses during collection.
The normal six-player path and on-chain admission rules are unchanged.

## Compete for one prize

For Amir's competitive 100-player mode, initialize with
`--payout-policy winner_takes_all --epoch 0`. Each agent plays for itself. Final
cash determines rank; goods have no terminal value. Equal cash is resolved by
the canonical wallet-byte ordering, so exactly one player wins. That player
receives the entire simulated bank after rake; everyone else receives zero.
At 100 entries of 10 units and the existing 5% rake, this is a 950-unit prize.
Cash is a ranking score, not an additional withdrawal from the prize bank.

This experimental policy is frozen before play and included in state identity.
It uses the existing settlement calculator with one 100% share. Without the
option, `canonical_top4` preserves the ordinary four-place schedule. The
one-winner policy currently accepts epoch 0 only; epoch 1 has additional rent
and factory payments that would require a separate design.

In this mode the model sees its current rank, cash score, leader score, gap to
the leader, execution position, current price curve, and the number of remaining
market phases (including the current market when applicable). Earlier players'
trades affect its actual execution price. It is told to pursue its own first place, with its
personality secondary to that goal. Free production costs no cash, but goods
made after the last market cannot improve the final cash rank. Menus still
allow strategic choices; no scripted policy replaces a model decision.

See the [decision and evaluation boundary](../../docs/ops/WORLD_100_SINGLE_WINNER_20261109.md).
The earlier two BitLLM games used the four-place policy; they are not evidence
that the new incentives improve model play.

For the narrowly scoped market-information pilot, freeze
`--market-context-arm baseline --focal-actor 0` or
`--market-context-arm candidate --focal-actor 0` together with the one-winner
policy. Both require a new directory. These options hold all other players'
prompt policies fixed and reveal the additional market block only to the
candidate focal player in market phases. Without these options, the ordinary
one-winner mode gives the full context to all players. See the
[pre-run paired protocol](../../docs/ops/WORLD_100_MARKET_CONTEXT_PROTOCOL_20261109.md)
for the fixed seed, budgets, exact intervention and single-pair limitations.

The [completed paired pilot](../../docs/ops/WORLD_100_MARKET_CONTEXT_RESULT_20261109.md)
made 3,612 live BitLLM requests across two 100-agent games. Both games had one
winner and passed full offline replay. The additional information changed no
focal market decision: cash 6, rank 89 and payout 0 in both arms. This result
does not validate a stronger strategy or estimate a population win rate.

## Build and freeze the experiment

Use Node 22 and a native Rust worker built from the same reviewed checkout:

```sh
cargo build --manifest-path arena/Cargo.toml --locked --bin world_worker
node tools/agent-bootstrap/world-run.mjs init \
  --dir /absolute/private/path/alashi-world-100 \
  --worker /absolute/path/alashi/arena/target/debug/world_worker \
  --agents 100 --seed 20261109 --game-id 20261109 --epoch 0 \
  --payout-policy winner_takes_all \
  --url http://127.0.0.1:7778/v1/chat/completions \
  --model qwen3.8-flash-next-fp4-alesha \
  --parallelism 2 --max-calls 2000 --max-tokens 4000000 \
  --max-seconds 7200 --max-consecutive-provider-failures 5
```

On an Apple silicon Mac without Xcode/admin access, the
[Experimental 100-agent world workflow](../../.github/workflows/world-100.yml)
builds and tests a native macOS worker on GitHub's CPU runner. Download the
`world-worker-macos-arm64` artifact from a successful run for the intended commit,
extract its inner tar archive, and verify `shasum -a 256 -c SHA256SUMS` in that
directory. `commit.txt` records the exact build commit (for a pull request this
can be GitHub's test merge commit); `toolchain.txt` records Rust and architecture.
Use that `world_worker` path in `init` with the matching source checkout. The
binary needs macOS 13 or later on arm64. This route requires no local Rust
compiler, GPU, Docker or administrator installation. Artifacts expire after
seven days; retain the verified binary with your private run evidence.

`init` makes no model requests. It creates a private directory and freezes the
configuration, seed, worker SHA-256 and driver SHA-256. Keep that worker binary
and driver for replay. A modified binary, profile or configuration cannot
silently resume an existing experiment. A new configuration needs a new run
directory and must be reported as a separate experiment.

The example explicitly reserves a four-million-token ceiling for the expanded
competitive observations. This is a hard limit, not measured usage or a price
quote; the unchanged CLI default is two million tokens when omitted.

With the locally authenticated BitLLM gateway running, explicitly start:

```sh
node tools/agent-bootstrap/world-run.mjs run \
  --dir /absolute/private/path/alashi-world-100 --allow-provider yes
```

Only public simulated game information and synthetic agent identities go to
BitLLM peers. If the local gateway requires a bearer key, supply it through
`ALASHI_MODEL_KEY`; never put keys in a command argument, report or repository.
The driver uses temperature 0, `max_tokens: 256` and
`chat_template_kwargs: {"enable_thinking": false}`. There is no automatic model
switch, fallback policy or retry of an ambiguous model call.

## What the agents decide

Each primary market, action and law phase asks every required living player for
one decision. After voting, the current president alone may get a veto review.
A full 100-player six-round game normally needs 1,800–1,806 model requests.
Concurrency 2 means at most two pending requests, not two players in the world.

The prompt contains the player's state, canonical action explanations, common
prices and law, up to three balanced interaction peers, and its last three receipts.
Full public snapshots stay in the local evidence. The allowed action menu is a
bounded legal subset generated by probing the canonical engine; it is not an
exhaustive strategy space. For example, all players canonically start with zero
cash and goods, so the first market menu contains only `pass`. No starting
wealth is fabricated for the experiment. Decisions are collected against one
snapshot and applied in fixed seeded rotating order through the canonical rules:
an initially legal trade can still be
rejected after an earlier player's decision changes the world.

Invalid JSON, an out-of-menu action, timeout or unknown provider result becomes
an explicit skipped decision, never a scripted replacement attributed to the
model. Canonical rejections and model/parser failures are reported separately.
HTTP 401, 403 or 429 stops new requests immediately; the configured consecutive
failure threshold also stops the run. Already pending calls are awaited and
saved. An incomplete phase is never applied.

## Budgets, recovery and evidence

The request and token limits cover the entire game and concurrent requests.
Before each request the driver fsyncs an `inflight` record and reserves a
conservative UTF-8-byte prompt allowance plus response overhead. Reported usage
replaces this reservation when available; missing/ambiguous usage keeps its
reservation and remains **unknown**, never zero. Peer-reported token counts and
model names are not independently verified; token limits are not a USD price
guarantee. Wall time includes downtime after the first run starts.

Re-run the same `run` command to resume. Saved phase batches are replayed through
the identical Rust worker and their complete responses checked. Saved decisions
are reused. An `inflight` request left by a crash is marked unknown and will
never be sent again. After a hard process kill a `.world.lock` may remain;
verify its PID is no longer running before removing that lock. Never run two
drivers in one experiment directory. A rate-limit or access stop is a checkpoint,
not permission to retry repeatedly; resolve the provider condition first.

Artifacts include:

- `frozen.json`: profile, limits, runtime hashes and experiment digest.
- `calls/`: durable request reservation, exact prompt, result metadata and parsed
  action; provider reasoning fields and bearer keys are not stored.
- `snapshots/` and `phases/`: canonical state, complete action batches and receipts.
- `summary.json` and `final-snapshot.json`: progress or final state, leaderboard,
  accepted/rejected/skipped counts, reported tokens and unknown reservations.

```sh
node tools/agent-bootstrap/world-run.mjs report \
  --dir /absolute/private/path/alashi-world-100
```

`game_completed: true` means canonical settlement finished. `completed_with_errors`
means the game finished with rejected/skipped decisions or unknown usage; inspect
the counters before calling an experiment successful. `stopped` is a partial
experiment with an explicit reason. `provider_cost_usd: null` means cost was not
measured; `nvidia_spend_usd: 0` applies to this driver only. Publish a concise
sanitized summary with source commit and artifact hashes instead of the private
working directory.

## Verification

```sh
node --test tools/agent-bootstrap/world-run.test.mjs
WORLD_WORKER=/absolute/path/alashi/arena/target/debug/world_worker \
  node --test tools/agent-bootstrap/world-run.test.mjs
```

The first command tests bounded concurrency, phase batching, token/call limits,
provider failure stops, unknown usage, durable replay, profile/hash changes and
zero-extra-call resume with an explicitly labelled protocol fixture. The second
also runs 100 players for six rounds through the **real Rust worker**, using a
deterministic test provider and simulated settlement. Neither is evidence that
BitLLM completed a live game. Live evidence must come from a separately frozen
`provider_reported` experiment.

Design decision: use the existing canonical engine via a local JSONL child
process, rather than duplicating game rules in Node or broadening public arena
admission. The main risks are unknown provider usage and correlated behavior
from one model; journal reservations and per-agent receipts expose them.
Completion is observed as canonical six-round settlement with all 100 identities
represented, alongside transparent model success and failure counts. Reconsider
the prompt/menu or budget in a new experiment if malformed output, widespread
rejections or the token limit prevents this result.

## Balanced recipient policy

The current `balanced_cyclic_peers_v1` policy uses the same recorded peers for
visible opponent rows and bribe candidates. Wealth and influence do not select
those peers. With 100 living participants every target appears three times per
round across potential menus, once per slot; affordability filtering can make
actual legal offers unequal. See the [second-run protocol](../../docs/ops/WORLD_100_BRIBE_FIX_20261109.md)
for implementation, acceptance and comparison with the original policy.

The second live game completed all six rounds: 1,806 BitLLM responses,
1,805 accepted actions and one explicitly blocked out-of-menu bribe, reported
as `completed_with_errors`. All six recorded action snapshots passed the peer
exposure checks. Fourteen accepted bribes went to fourteen different players;
the largest recipient share was 7.14% versus 80.56% in the original game.
Replay and settlement passed. This single pair demonstrates the interface
correction, not stronger strategies or a generally balanced economy; twelve
of the fourteen accepted bribes still selected the first menu option.

## First live run

The [9 November 2026 run](../../docs/ops/WORLD_100_20261109.md) completed all six
rounds for 100 players: 1,806 accepted model decisions, zero errors or unknown
usage, and 1,424,513 peer-reported tokens. The live run froze a 4,000,000-token
ceiling; the example above uses the configurable 2,000,000 default. Exact Rust
replay made no new model calls. The report separates successful execution from
strategic quality and documents the recipient bias of the bounded bribe menu.

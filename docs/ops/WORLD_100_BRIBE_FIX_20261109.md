# Balanced recipient exposure: second 100-player BitLLM game

The second live game completed on 9 November 2026: 100 players, six rounds,
1,806 BitLLM responses, 1,805 accepted actions and one explicitly skipped
out-of-menu response. The recorded status is `completed_with_errors`.
The first game's runtime and journals are preserved unchanged. NVIDIA spending
for this task was USD 0; BitLLM cost in USD was not measured.

## Decision and identified defect

Amir requested removal of the bribe bias and a second 100-player game through
BitLLM, without NVIDIA expenditure. The [first run](WORLD_100_20261109.md) used
the richest rival, most influential rival and next player as bribe candidates.
Its compact observation also privileged the richest three rivals. Player 43
received 58 of 72 bribes (290 game units), finishing with cash 300.

The decision is to remove wealth/influence-based preference from the
experimental recipient and observation interface. Canonical transfer amounts,
influence gains, settlement, market order and game economics are unchanged.
The primary risk is mistaking a mechanically more balanced candidate graph for
proof that the shared model learned a stronger or fairer strategy.

## Implemented intervention

`balanced_cyclic_peers_v1` creates a seed-dependent permutation of living
players and a separately seeded permutation of nonzero ring offsets. Each
round uses up to three offsets shared across all actors. Each player's
`interaction_peers` is used both for bribe candidates and the compact public
opponent view, in the same order. There is no rich-player fallback.

With 100 living players, every recipient appears exactly three times per
round across all potential sender menus, once in each position. Over six
rounds each actor sees 18 distinct partners. Cash, influence and execution order
do not affect this selection. Small groups use `min(3, living_players - 1)`;
self and dead players are excluded. Changes in living membership rebuild the
selection and can repeat partners seen before that change.

This guarantee concerns potential exposure before canonical legality checks.
An actor without the bribe price has no legal bribe options, so actual legal
exposure and chosen payments need not be equal. The worker records peer IDs
before filtering; the driver validates peer references and rejects bribe
targets outside the visible set. In experimental epoch 1, the roof menu also
uses the first peer so its recipient remains visible; tariffs are unchanged.
The paired live game uses epoch 0 and is unaffected by this roof adjustment.

Changing the money transfer itself was rejected because it would redesign the
game rather than correct the observed interface bias. Offering all 99 rivals
was deferred: it changes the size of the bounded decision space and prompt
budget, and should be evaluated as a separate interface experiment. Three
balanced recipients retain a compact view while removing preferential exposure.

## Pre-run protocol and interpretation boundary

This is an adaptation of Methodology Atlas
[P-34](https://github.com/Jakisheff/methodology-atlas/blob/main/methods/projects/P-34-intervention-validated-agent-maps.md):
intervene on the recorded recipient/observation policy, then regenerate the
whole game so opponents respond to the new states. No hidden model reasoning
is reconstructed. One pair of games is exploratory; the 1,806 decisions are
not 1,806 independent experiments and cannot support a statistical confidence
interval for a population effect.

Keep the first run's environment seed and game ID `20261109`, epoch 0,
100 synthetic identities, personalities, short receipt memory, sequential
phase execution policy and six rounds. Actual order can diverge if endogenous
veto behavior changes the number of post-vote phases; record that separately
from whether the ordering algorithm changed. Use the same BitLLM alias
`qwen3.8-flash-next-fp4-alesha`, temperature 0, 256 completion tokens and
`enable_thinking: false`, concurrency 2, at most 2,000 calls / 4,000,000 tokens /
7,200 seconds. Worker and driver hashes identify the intervention. Endpoint
model identity and usage remain peer-reported; the same alias/temperature does
not guarantee identical external model outputs or a fixed upstream model.

Acceptance of the implementation requires wealth/influence-independent
selection, equal potential exposure/positions, visible targets matching menus,
canonical full-game tests, and a completed live game with transparent error,
usage and per-player counts. Verify exact replay and payout+rake accounting.
Do not silently replace missing model decisions with scripted moves.

Compare the following recorded quantities with run 1:

- Potential recipient exposure and slot exposure before affordability checks.
- Actual legal menu exposure, eligible sender-rounds and recipient coverage.
- Actual bribe count, recipient count, largest recipient share and recipient
  HHI; show both amounts and denominators.
- Chosen recipient wealth rank relative to the offered alternatives on those
  same decisions, rather than relative to unoffered players.
- Winner cash attribution, payout, all final balances and ordinary action mix.
- Calls, known/unknown tokens, errors, latency and exact replay.

Run 1 had a top recipient share of 58/72 = 80.56%, HHI 0.65200617 and 14 actual
recipients. Lower concentration in run 2 may be a direct effect of the bounded
exposure graph. It does not establish improved strategic intelligence, an
unbiased complete action space, or overall economic balance. Reconsider the
bounded policy if invariants fail, targeting becomes invisible, or observed
concentration persists despite balanced opportunities; additional seeds and
independent games would be needed to judge robustness.

## Results

The [paired comparison](world-100-bribe-fix-20261109/comparison.json) confirms
the structural acceptance criteria on all six action-phase snapshots: all
100 living recipients have three potential appearances per round, once in
each slot, and every actor sees 18 distinct partners across the game. Visible
opponents exactly match those recorded peers. Frozen provider settings,
initialization, system/personality prompts and the initial canonical state
match run 1. Realized primary execution orders also matched in this pair.

### Recipient opportunities and actual transfers

| Measure | Run 1 | Run 2 |
|---|---:|---:|
| Potential appearances per recipient across six action rounds | 6–496 | 18 each |
| Actual legal offers per recipient | 0–388 | 11–14 |
| Eligible sender-rounds / total legal offers | 393 / 1,075 | 401 / 1,203 |
| Legal offers naming agent-043 | 388 | 11 |
| Accepted bribes | 72 | 14 |
| Distinct recipients | 14 | 14 |
| Largest recipient count / share | 58 / 80.56% | 1 / 7.14% |
| Recipient HHI (sum of squared transfer shares) | 0.65200617 | 0.07142857 |
| Incoming bribe money for agent-043 | 290 | 5 |
| Final cash / rank of agent-043 | 300 / 1 | 17 / 23 |
| Accepted bribes by menu position 1 / 2 / 3 | 45 / 25 / 2 | 12 / 2 / 0 |

Money in this report is simulated game units. The first run's potential
candidate graph is reconstructed from its verified original source policy;
actual legal offers and model-visible rows in both runs come from saved menus
and prompts. The second run's potential peers are explicitly recorded.

All 14 accepted bribes in run 2 transferred five units to different recipients.
The number of recipients did not increase: the volume fell from 72 to 14
transfers and concentration fell. The one blocked bribe request described below
is excluded from accepted-transfer counts and recorded separately.

The correction removes privileged exposure for wealth/influence. It does not
eliminate positional behavior: 12 of 14 accepted bribes (85.71%) chose the first
offered recipient. Balancing each recipient's slot exposure prevents this from
automatically promoting the same rich actor, but does not make choices uniform.
Conditional on the offered sets, the mean chosen cash percentile minus a
uniform-within-offer reference changed from +0.1513 to −0.0162. This is a
descriptive comparison of 72 versus 14 choices, not a significance test.

### Execution, one blocked answer and exact replay

| Measure | Run 2 |
|---|---:|
| Participating identities / primary calls per identity | 100 / 18 |
| Primary phases / president reviews | 18 / 6 |
| Model calls / accepted actions | 1,806 / 1,805 |
| Canonically rejected actions / skipped decisions | 0 / 1 |
| Unknown token usage | 0 |
| Reported prompt / completion tokens | 1,375,237 / 19,895 |
| Reported total tokens | 1,395,132 |
| First request → estimated last response | 14:16:21.866 → 14:49:00.565 UTC |
| Provider window | 32 minutes 38.699 seconds |
| Median / p95 request latency | 2,101 / 2,570 ms |
| Peak measured overlapping requests | 2 |

All responses reported `qwen3.8-flash-next-fp4-local`, the same reported model
identifier as run 1. The request alias stayed `qwen3.8-flash-next-fp4-alesha`.
These identifiers and token counts are peer-reported, not attested weights.

At round 5, action phase, actor 55 returned a five-unit bribe to actor 45;
its permitted targets were 53, 52 and 51. The HTTP response was successful and
its 868 tokens are counted, but the driver correctly recorded
`action_not_in_snapshot_menu` and submitted an explicit skipped decision.
It neither changed the target nor retried the request. This was the sole
invalid model decision; no transport/parser error or canonical rejection was
recorded. Actor 55 ultimately ranked third, so the skipped cash expenditure
can matter to ranking. No repaired counterfactual game is claimed.

The [public audit](world-100-bribe-fix-20261109/audit.json) checked 38,173
journal assertions with zero inconsistencies; the model's skipped decision is
represented consistently rather than hidden. The [replay receipt](world-100-bribe-fix-20261109/replay.json)
records reexecution of all 24 saved batches through the exact Rust worker:
each full response matched, all 1,806 call files and the final summary/snapshot
were unchanged, and no new model requests were made.

Final state hash:
`2f74018a5fd7e4c09c92b60d14a0cfad720a79757b0fa07991609578ac8f79d7`.

### Settlement and action mix

The simulated bank was 1,000; payouts 950 plus rake 50 matched it exactly.
Only the top four receive payouts under the unchanged canonical schedule.

| Rank | Player | Final cash | Payout |
|---|---|---:|---:|
| 1 | agent-047 | 37.4 | 475 |
| 2 | agent-051 | 35.4 | 285 |
| 3 | agent-055 | 33.2 | 142.5 |
| 4 | agent-061 | 32.3 | 47.5 |

The winner's 37.4 cash came entirely from sales, with no incoming or outgoing
bribe transfers. Agent 43's cash reconciles as sales 22 + incoming bribes 5 −
outgoing bribes 10 = 17; it received no payout. Payouts and ranking cash remain
separate quantities, and this accounting is not a counterfactual explanation.

| Round | Producers | Accepted bribes | Skipped decisions in action phase |
|---|---:|---:|---:|
| 1 | 100 | 0 | 0 |
| 2 | 98 | 2 | 0 |
| 3 | 99 | 1 | 0 |
| 4 | 91 | 3 | 0 |
| 5 | 77 | 7 | 1 |
| 6 | 7 | 1 | 0 |

Accepted actions total 1,805: 472 production, 465 sales, 14 bribes, 517 votes,
three vetoes and 334 explicit passes. Seven final-round production actions
still had no remaining market opportunity. The temporary BitLLM process was
stopped after replay; its private state was retained outside Git.

### Conclusion and remaining limits

The wealth/influence-based menu and observation preference is removed and the
new exposure guarantees are verified in code, tests and the recorded game.
This exploratory pair also shows much less concentrated accepted transfers.
The model did not learn during this task: inclusion, visibility and the graph
of possible transfers changed. The graph itself limits concentration, legal
opportunities remain affordability-dependent, positional preference persists,
and one invalid answer affected the executed game. One pair cannot establish
robustly stronger strategies or a fair economy. Market order and the four-place
payout schedule remain other potential sources of imbalance.

## Frozen runtime and validation

- Source: `432b1117135aec256b9c0a79bdf89d46541296c0`, [PR #73](https://github.com/Jakisheff/alashi/pull/73).
- Native build: `0cace8d9a11dd7937d002d25917ca9122246f3da`, GitHub's test merge.
  Its tree `33da04a6b3c7ef2e250e930d1cba779ac3dff77e` matches the source head.
- Worker SHA-256: `b6bacfc2c66501498cafa024d50b4eac9d9f33c8e3b6f6d6a20c0a908ac8410d`.
- Driver SHA-256: `912bf75b3a99ad7642e6d14225449d2ba0ff1270ab64c7d5ce2e61881bdb77c8`.
- Frozen experiment digest: `85831ef2b8cefa3fb56418d53ccee5a4527f03c6f1dbc0afc9ac401cd19637b9`.
- Native archive matched artifact SHA-256
  `201a60f9d7e8774c6d824c95ba8bb19311844d5ec0728188bc3094bd15484ca7`.

[Native CPU CI](https://github.com/Jakisheff/alashi/actions/runs/42632288434)
passed 42 rules tests, eight worker tests and all 16 driver tests, including a
real 100-player six-round game and resume. Local preflight on the school Mac
verified exact three-way/slot exposure for all 100 actors and the same initial
canonical state hash as run 1. Frozen configurations differ only in worker path,
worker hash and driver hash; initialization, provider settings and all budgets
match. [Full CI](https://github.com/Jakisheff/alashi/actions/runs/42632288542)
also passed all five jobs, including fresh SBF execution and both-epoch replay
tests. Existing explicitly ignored tests remain ignored; no claim is made that
they executed.

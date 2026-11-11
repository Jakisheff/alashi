# Market context did not change play in the single paired pilot

The predeclared information intervention produced **no observed improvement**
in this pair. Agent 0 finished both games with cash 6, rank 89 and payout 0.
Its six market decisions were identical. All other selected actions and the
entire recorded state trajectory also matched. The candidate consumed 1,356
additional prompt tokens, with no change in completion tokens or game outcome.

Both live BitLLM games completed on 9 November 2026, with 100 model-controlled
players and six rounds each. Each game had exactly one prize recipient:
agent-026, cash score 50, payout 950. Bank 1,000 = payout 950 + rake 50.
All amounts here are simulated game units, not real revenue.

## What was tested

The [protocol](WORLD_100_MARKET_CONTEXT_PROTOCOL_20261109.md) was committed in
`bb6f788e6dc2c1b382aba9116327fe66d092a8fa` before the first model call. Amir chose
one pair, approximately one hour, explicitly omitting the proposed second pair
with another seed and seat. No additional game was run after seeing the result.

Both arms used seed/game ID `20261110`, epoch 0, `winner_takes_all`, the same
100 identities, personalities, common winning objective, rank/horizon facts,
legal menus, memory and model settings. The previous four-payout games were
diagnostic material, not this experiment's baseline.

Only the candidate's `agent-000`, only in a market phase, additionally received
the current effective price curve, its execution position and the associated
explanation. The other 99 policies stayed fixed and received fresh BitLLM
responses to the actual game states. The runner made fresh BitLLM requests and
did not substitute scripted, cached or replayed responses. Provider-side caching
was not instrumented.

The worker used the same single-winner rules in both games. The larger prize
relative to the earlier four-place schedule is a redistribution and is not
evidence of improved strategic quality.

## Paired outcome

| Measure | Baseline | Candidate |
|---|---:|---:|
| Agent-000 final cash | 6 | 6 |
| Agent-000 rank | 89 | 89 |
| Agent-000 winner / payout | No / 0 | No / 0 |
| Gap to winner's cash | 44 | 44 |
| Agent-000 terminal goods | 0 | 0 |
| Agent-000 invalid or skipped decisions | 0 | 0 |
| Winner | agent-026 | agent-026 |
| Winner's cash / payout | 50 / 950 | 50 / 950 |
| Positive payouts | 1 | 1 |

Candidate minus baseline: **cash 0, rank 0, payout 0, changed focal market
actions 0**. The predeclared directional criterion was not met: it required
higher cash, a rank no worse, and a changed focal market action accepted in
both games. There is no positive signal for this block in this pilot;
it does not establish that the information is useless for all agents or seeds.

### Trace of the focal decisions

| Round | Queue position | Goods before market | Market choice in both arms | Sale receipt |
|---|---:|---:|---|---:|
| 1 | 95 | 0 | Pass | 0 |
| 2 | 91 | 2 | Sell 2 | 2 |
| 3 | 87 | 2 | Sell 2 | 2 |
| 4 | 83 | 2 | Sell 2 | 2 |
| 5 | 79 | 0 | Pass | 0 |
| 6 | 75 | 0 | Pass | 0 |

Agent 0 produced in action rounds 1–3, then passed in rounds 4–5 while free
production was still available and future markets remained. Its final-round
pass was consistent with having no later market. These are observable choices,
not an explanation of hidden reasoning. Forcing different actions would require
regenerating opponents' responses before making a whole-game profit claim.
No second hypothesis or follow-up experiment was run in this task.

## Execution, controls and resources

| Measure | Baseline | Candidate |
|---|---:|---:|
| Model responses / accepted actions | 1,806 / 1,806 | 1,806 / 1,806 |
| Rejected / skipped decisions | 0 / 0 | 0 / 0 |
| Participating actors / primary calls each | 100 / 18 | 100 / 18 |
| President-review calls | 6 | 6 |
| Reported prompt tokens | 1,664,296 | 1,665,652 |
| Reported completion tokens | 19,429 | 19,429 |
| Reported total tokens | 1,683,725 | 1,685,081 |
| Unknown usage / in-flight calls at finish | 0 / 0 | 0 / 0 |
| Full replay batches matched | 24 / 24 | 24 / 24 |

Pair total: **3,612 responses and 3,368,806 reported tokens**. The supervisor
ran from 15:50:47.610 to 16:32:45.666 UTC, approximately 42 minutes. Both arms
stayed within the same 2,000-call / 4-million-token / 7,200-second ceilings.
The temporary BitLLM process was stopped after completion. NVIDIA spending
was USD 0; BitLLM USD cost was not measured.

All responses reported `qwen3.8-flash-next-fp4-local`; both requested
`qwen3.8-flash-next-fp4-alesha` with temperature 0, max completion 256 and
thinking disabled. These identities and usage counts are peer-reported, not
attested model weights.

The [public audit](world-100-market-context-20261109/paired-audit.json) reconstructed
all 3,612 model prompts from the saved snapshots and each actor's own receipts.
It checked the exact information-block difference against an independent field
allowlist. Of the 1,806 corresponding calls, 1,800 had identical inputs and zero
normalized action differences. The six different focal inputs also produced
the same actions. All 25 matched snapshots, including the final state, agreed.

The auditor reexecuted every saved batch through the frozen Rust worker and
matched all 48 full responses, without calling a model or changing the journals.
Its 18,144 pair assertions and 38,174 generic assertions per arm found no
journal, prompt, state or settlement inconsistencies. These are assertions,
not independent statistical observations.

Final state hash, equal in both arms:
`cec962069360968ea2ef5d5499636713e3bfd23e130eacd81c3241aa14d5e01c`.

### Timing measurement limitation

The frozen code runs one awaited request lane per arm, structurally bounding
these two clients to two concurrent requests. The journal does not independently
measure gateway or network concurrency. Its `started_at` uses wall time before
journal fsync, while `elapsed_ms` uses a monotonic clock starting afterward.

Combining those different clocks produced two adjacent-call overlap estimates
at the same point in both arms: raw duration/start-delta differences of 45 and
43 ms. The reconstructed interval peaks were 2 per arm and 3 for the pair.
These raw values remain in the audit. The actual network peak is **unmeasured**;
a wall-clock adjustment is plausible but not proven by the journal.

The first auditor version treated that derived peak as a protocol violation.
An independent code review found the estimator could not establish that claim.
The corrected audit keeps a timing-quality warning rather than declaring a
financial or replay failure. The [correction record](world-100-market-context-20261109/timing-audit-correction.json)
and [initial flagged report](world-100-market-context-20261109/initial-timing-audit.json)
preserve the finding and version hashes. No game data, runtime, intervention or
predeclared outcome criterion was changed. The null outcome is unchanged.

## Evidence and reproducibility

- [Frozen preflight](world-100-market-context-20261109/preflight.json): configurations
  differ only by arm; initial worlds and menus match; all 99 nonfocal prompts match.
- [Independent CPU preflight](world-100-market-context-20261109/independent-preflight.json).
- [Native build provenance](world-100-market-context-20261109/runtime-provenance.json).
- [Fixed-decision settlement regression](world-100-market-context-20261109/settlement-replay.json):
  the previous game's actions preserve economic states under both payout policies.
  This is a CPU check, not another live or behavioral counterfactual game.
- [Economic trace diagnostics](world-100-market-context-20261109/economic-diagnostics.json):
  identical phase responses, focal production choices and the winner's cash ledger.
- [Pair auditor](world-100-market-context-20261109/market-context-pair-audit.mjs)
  and [generic journal auditor](world-100-market-context-20261109/alashi-world-evidence-audit.mjs).
- [Initial auditor](world-100-market-context-20261109/market-context-pair-auditor-initial.mjs)
  retained to reproduce the timing-estimator correction.

Frozen source: `bb6f788e6dc2c1b382aba9116327fe66d092a8fa`.
Native test-merge build: `84e3b0dd824f06d3c3a2dad9f543a8e1285ecdcf`; its tree
`74b2983b86f28ad96bac4b27929d05fd5a0df210` matches the source tree.
Worker SHA-256: `4db7cebd02be1ce2a476fee35312873c09837997b6fbf492f32b6489c8b8fcf1`.
Driver SHA-256: `fb2f7ec36fb062d5ba5855f97b728e82cb23e526c5c3a1721318c50de0a7f34e`.

Before live calls, [full CI](https://github.com/Jakisheff/alashi/actions/runs/42647637099)
and [native CI](https://github.com/Jakisheff/alashi/actions/runs/42647636763) passed
all six jobs, including 14 worker tests and 25 driver tests with real 100-player
integration and resume. Fresh SBF checks covered 28 program and five replay tests.
The independent auditor subsequently passed 11 CPU regression tests, including
full fixture replay, tampered memory and the mixed-clock measurement case.

The private downloadable archive contains the original journals and exact runtime
for offline replay. This repository contains the public reports and audit code;
local paths, peer IDs, response IDs and authentication state are not published.

## Decision

Do not describe this market-context block as a demonstrated strategy improvement.
Retain the null result and the predeclared protocol. There is one pair, one focal
seat and one seed; no repeated evaluation, holdout, seat rotation, population
win-rate estimate or meaningful effect confidence interval is available.
The current evidence supports only the observed absence of a decision or money
change in this controlled scenario.

# One paired BitLLM pilot: market context for a single competitor

Status: pre-run protocol. Commit this before any provider calls. Results belong
in a separate report; this file records the choice made before outcomes.

## Question and one intervention

Amir requested one hypothesis and one experiment in a world of 100 competing
agents with a single prize winner, using BitLLM and no NVIDIA. When offered a
repeated two-pair design, he chose **one pair, approximately one hour**, accepting
that it cannot check repeatability. This overrides the earlier proposed
seat-rotation and held-out repeat. There will be exactly two newly generated
games, not a search over seeds followed by selection of the best result.

Hypothesis: giving one competitor the current market price curve and its place
in the execution queue helps it choose when to sell, increasing its final cash
without worsening its rank. This is an information-block intervention, not
training a model or changing the economic rules.

The [run-02 diagnostics](world-100-single-winner-20261109/baseline-diagnostics.json)
motivated the hypothesis: all recorded sales immediately liquidated inventory,
and 865 of 930 units sold at that phase's floor. The old compact prompt included
the opening quote but omitted the full curve and the actor's execution position.
These facts do not establish that holding inventory would have won the game.

## Frozen comparison

| Item | Both arms |
|---|---|
| World | 100 players, epoch 0, six rounds |
| Environment seed / game ID | `20261110` / `20261110` |
| Focal competitor | `agent-000`, selected before inspecting this seed's outcomes |
| Payout policy | `winner_takes_all`: bank 1,000, rake 50, one prize 950 |
| Ties | Existing ascending wallet-byte tie-break |
| Other 99 agents | Same fixed baseline prompt policies and personalities |
| Model request alias | `qwen3.8-flash-next-fp4-alesha` |
| Sampling | Temperature 0, max completion 256, thinking disabled |
| Concurrency | One request per arm; two across the pair |
| Per-arm ceilings | 2,000 calls, 4,000,000 tokens, 7,200 seconds |
| Pair ceilings | 4,000 calls, 8,000,000 tokens; no automatic repeat |
| Failure handling | 60-second request timeout; stop at five consecutive failures or HTTP 401/403/429 |

The per-arm token ceiling is the same as the previous live game. Actual usage,
returned model identifiers, failures, latency and request overlap are reported
separately. BitLLM USD cost is unknown and is not recorded as zero. NVIDIA is
not used. There is no automatic switch to another model or paid GPU provider.

Both variants share the new single-winner objective, personality hierarchy,
rank/leader/gap information, remaining market horizon, free-production fact,
legal menus, memory, rotating execution order and balanced peer selection.
This baseline is adapted to the single-winner rules; the previous four-payout
game is diagnostic material, **not** the control arm of this experiment.

The driver freezes `comparison = {id: "market_context_v1", arm, focal_actor: 0}`.
In the **baseline** arm every player receives the common prompt without the
market-context block. In the **candidate** arm only actor 0, only in a market
phase, additionally receives:

- The current effective unit-price curve by cumulative sold count, including
  the tail-price convention and active-modifier explanation.
- Its one-based position in the current execution queue.
- A short explanation that preceding trades can change its actual quote and
  that the current curve does not forecast future laws.

These fields and their explanatory sentence are one predeclared block. The
extra input length is part of the treatment and its resource cost is counted;
the experiment cannot separate the value of the two facts or pure information
content from the effect of a longer prompt. Arm labels never enter model prompts.
All 99 nonfocal prompts, and the focal nonmarket prompts, must be byte-identical
between policies when evaluated on the same snapshot and memory.

## Execution and independent checks

Freeze the protocol, code, worker, profile, both configurations and prompts
before calls. Verify the worker artifact and driver hashes. Run both arms
concurrently at one request each to reduce separation in provider time, using
separate private journals. All 100 players in both arms receive fresh BitLLM
decisions. Opponent *policies* are fixed; their old actions are not replayed as
opponent responses after the world changes.

Before calls, executable tests check the information difference, unchanged
nonfocal prompts, policy/configuration rejection, a full 100-player game,
exact resume, one positive payout and bank conservation. Local CPU preflight
checks identical initial economic state, actor set, menus and payout policy.

After each game, independently check raw requests/responses, accepted and
skipped decisions, all-player coverage, state transitions, token accounting and
the sole payout. Reexecute every saved batch through the frozen Rust worker
and require full response equality without extra model calls. Never repair a
bad model choice by silently substituting a scripted action.

Record the first focal market action difference and the first economic-state
divergence. Compare identical model inputs across arms, especially before that
divergence. A different response to identical inputs is provider variability,
not an effect of the information block. Same alias and temperature 0 do not
attest model weights or ensure deterministic inference. Flag returned-model
changes and earlier uncontrolled action divergence in the interpretation.

## Outcomes and decision rule

The primary diagnostic is focal final cash difference, candidate minus baseline.
Also report its rank, winner indicator, payout, gap to the winner, market
decisions, inventory held, sale receipts, terminal inventory and invalid replies.
Track the full world's winner and money accounting to verify game execution;
aggregate cash is not the competitive optimization target.

A **promising signal in this pilot** requires higher focal cash, a rank no worse
than baseline, and at least one changed focal market action that was valid and
accepted in both arms. An invalid reply or skipped decision alone does not meet
that action-change criterion. Lower cash or worse rank fails that directional criterion; equal
cash/rank or unchanged focal market decisions provides no positive signal.
Report mixed outcomes without forcing a success label. A failed or incomplete
game is an execution result, not a completed paired effect estimate.

If model variability changes economic actions before the intervention changes
a focal action, the pair does not isolate the cause of the final difference.
Even with clean execution, this is **one paired scenario**, not 100 independent
trials and not thousands of independent observations. There is no defensible
population win-rate estimate, repeatability claim or reliable effect confidence
interval from this single pair. The observed win indicators are descriptive.
An independent repeated evaluation remains outside this user-selected budget.

## Method and reconsideration

This adapts Methodology Atlas
[P-34](https://github.com/Jakisheff/methodology-atlas/blob/main/methods/projects/P-34-intervention-validated-agent-maps.md):
one observable input intervention, fixed policies and execution settings,
fresh responsive opponents, linked event evidence and bounded conclusions.
No hidden model reasoning is reconstructed.

Changing all 100 prompts was rejected because it would move the opponent
policies too. Replaying the original opponents was rejected because it would
freeze their reactions. Enlarging the prize as evidence of stronger strategy
was rejected because the bank is fixed and reward redistribution is mechanical.
Reconsider the market-context block if it changes no relevant decisions,
worsens the chosen outcomes, or if observed gains cannot be distinguished from
provider variability. Preserve a negative or inconclusive result as such.

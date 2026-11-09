# Competitive 100-player mode: one prize winner

## Owner outcome and scope

On 9 November 2026 Amir made agents earning money the next priority, then
clarified: they must play against each other and there must be only one winner.
The resulting task is competitive individual play in the existing shared game.
It is not a goal to maximize the sum of all players' balances or a claim about
real-world agent revenue.

This is a local product change guided by the owner's
[NMT/AJTBD routing](https://github.com/Jakisheff/methodology-atlas/blob/main/methods/research/R-22-nmt-canonical-routing.md).
The observed user is the owner running 100-agent experiments; the trigger is
the first two games; the requested result is self-interested play with a single
prize winner. Success criteria are a single correctly paid winner, an explicit
individual objective, visible progress toward it, and reproducible execution.
The owner's wider business Job, emotional states, customer demand and willingness
to pay are not established by this exchange. No market-validation thresholds
are inferred from the methodology.

## Why the previous objective was insufficient

The [second game](WORLD_100_BRIBE_FIX_20261109.md) had one first-place finisher
but four paid positions. It ended with a simulated bank of 1,000, rake of 50,
and payouts of 475 / 285 / 142.5 / 47.5. Ranking cash and prize money are
different quantities. The winning cash score was 37.4; total final cash was
1,550.6 and median cash was 13.4. Increasing aggregate cash alone cannot create
more money in the fixed prize bank.

Every one of the 465 recorded sales sold all that actor's available goods.
The remaining 14 goods were produced by seven actors in the final action phase,
after the final market. These observations motivate making the remaining
trading horizon explicit, not a claim that a liquidation bug was found.
The old personality prompts were subordinate only implicitly to final wealth;
the competitive mode makes their secondary role explicit.

The [baseline diagnostics](world-100-single-winner-20261109/baseline-diagnostics.json)
recompute all 465 sales from the canonical price curve. Of 930 sold units, 865
sold at that phase's floor price. There were 21 action passes in rounds 4–5
while free production was legal, and one final-round bribe spent five cash
with no later cash-producing phase. These are observed decisions, not a claim
that forcing alternative moves would improve the whole game's outcome. Market
timing and political actions can change opponents' behavior. Conditional quotes
in the diagnostic file hold other events fixed and are not counterfactual games.

## Implemented rule contract

- Optional init field / CLI option: `payout_policy` / `--payout-policy`.
- Default or omitted policy: `canonical_top4`, preserving the established mode.
- Competitive policy: `winner_takes_all`, currently only for epoch 0.
- Score: final cash, descending. Unsold goods and influence do not count as
  final cash. Ties use the existing ascending wallet-byte ordering.
- Prize: the rank-1 actor receives `bank - rake`; the other 99 receive zero.
  The existing `compute_settlement` calculator receives shares `[100]`.
- The bank still comes from entry fees. At 100 × 10 and the current 5% rake,
  there is one 950-unit payout and rake of 50, with no additional payout stream.
- The policy is selected before the game, frozen by the driver and bound into
  worker state identity. It cannot be changed during recovery or replay.
- Epoch 1 is rejected for this policy because its additional license/factory
  economics would need an explicit single-winner design.

Actions, the shared market, political effects and balanced recipient selection
keep their existing rules. The ordinary HTTP and Solana payout paths are not
switched to this experiment. All money in this driver remains simulated.

## Agent decision interface

Every competitor gets the same objective: maximize its chance of sole first
place through its own final cash, treating the other players as rivals. The
model is not instructed to optimize team or aggregate wealth. Personality is
a secondary preference, not a reason to sacrifice the monetary objective.

The new compact observation adds:

- `competition.rank`, `score`, `leader_score`, `gap_to_leader`, and the player's
  one-based `execution_position` in the current batch.
- `remaining_market_phases_including_current`: six in round-1 market, five in
  round-1 action, one in round-6 market, zero in round-6 action and at finish.
- The frozen payout policy and the fact that production's cash cost is zero.
- `market_unit_prices_by_sold_count`, the current effective price curve. The
  last entry applies beyond the end of the table. This is a public rule, not
  a forecast: preceding players' trades change the actual quote, and future
  laws can change later rounds. The opening market price is not a guaranteed
  sale price for every player in the batch.

The model still chooses one object from its legal menu. Bribes, political
investment, pass and risk-taking are not silently prohibited. They can affect
relative position; immediate cash alone is not a complete strategic objective.
Invalid choices remain explicit skipped decisions, with no fabricated model
action. Provider concurrency, token ceilings and recovery behavior retain
their existing budgets.

## Verification and interpretation

Required executable checks cover a complete 100-player one-winner game, exact
resume, one positive payout, bank conservation, deterministic ties, policy
binding, rejection of unsupported mode/epoch combinations, correct horizon and
standings, and refusal of conflicting snapshots before model requests. Existing
top-four and both-epoch checks remain part of regression coverage.

The driver integration uses an explicitly labelled test provider. A passing
fixture proves the protocol and settlement, not that BitLLM agents learned to
earn more. No third live model party is claimed by this change. The two earlier
frozen journals and their runtimes retain their original incentives and payouts.

The new top prize is twice the old top prize because it is redistributed from
four recipients to one. That mechanical change is not evidence of better
decision-making, increased aggregate prize money, or a real financial return.
Before claiming stronger play, compare strategy variants under this same payout
policy and resource budget across games with varied seeds and seat assignments.
Measure wins and final cash separately, including invalid decisions, terminal
spending with no attainable benefit, and opportunity costs. A comparison to the
old four-place game changes incentives and cannot isolate a prompting effect.

## Decision record

The selected change aligns the monetary reward and the information shown to
the owner's explicit individual-competition goal. Merely renaming first place
as the winner was rejected because the old engine would still pay four players.
Increasing starting cash or minting extra prize money was rejected because it
would inflate the reported earnings without showing stronger competition.
Changing the entire deployed economy was excluded from this experimental scope.

The main implementation risk is a prompt claiming one winner while settlement
still pays several; the acceptance test follows the claim through frozen config,
worker state, final payouts and replay. The main empirical gap is model behavior
under the new incentives. Reconsider the interface if the model keeps making
terminal expenditures with no future cash benefit, ignores the explicit horizon,
or if gains vanish when seats and seeds change. Do not treat test-provider
completion or a larger nominal top prize as validation of agent intelligence.

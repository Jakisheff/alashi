# Balanced recipient exposure: second 100-player BitLLM game

Status before live execution: implementation and local Node checks complete;
native CPU CI and the second live game are pending. This is a separately frozen
experiment; the first game's runtime and journals are preserved unchanged.

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
before filtering; the driver validates peer references and rejects menu
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
phase execution order and six rounds. Use the same BitLLM alias
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

Pending native CI and the second live game. No completion or improvement claim
is made in this pre-run protocol.

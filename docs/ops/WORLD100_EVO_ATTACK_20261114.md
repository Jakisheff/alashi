# Adversary coverage for prereg P1: exhaustive policy enumeration, 14 November 2026

Execution of P1 from [20261114-prosecutor-pass](../../200_ZETTEL/20261114-prosecutor-pass.md)
as adversary coverage, not evolution: the preregistered discrete policy
space is screened and the influential subspace enumerated exhaustively
against the canonical worker. Thresholds were frozen before any game:
no admitted policy may exceed share 0.08 or HHI 0.03 on any held-out
seed; a breach downgrades the "constraint beats detection" transfer to
"holds against known policy families".

## Method

Harness [world-evo-attack.mjs](../../tools/agent-bootstrap/world-evo-attack.mjs)
drives the world worker over JSONL directly (no HTTP during search).
Calibration: the L4 anchor on seed 20261109 reproduced the published
acceptance game bit for bit (224 bribes, 70 recipients, share
0.03571428571428571, HHI 0.02132493622448979, 1806/0/0). Speed: about
0.8 s per hundred-agent game per lane, six lanes.

OAT screening (25 variants around L4, three train seeds, influence
threshold 0.005 on mean share) found three influential genes:
action_alt (produce -> donkey raises share by +0.0180, the largest
single effect), sell_rule (selling less concentrates more: the volume
effect again), and target_rule (partially; poorest and first_slot are
well below the anchor). Non-influential at the threshold: bribe
appetite, law vote, stickiness (herding did not concentrate).

The influential subspace target_rule x sell_rule x action_alt (7 x 3 x
2 = 42 genomes) was enumerated exhaustively on the three train seeds
(126 games). Top five by worst train share plus both anchors and the
uniform reference then ran on five held-out seeds (40 games).

## Results (held-out, worst over five seeds)

| policy | worst share | worst HHI |
|---|---:|---:|
| richest, half/unit sell, donkey | 0.0700 | 0.0302 |
| richest, sell all, donkey | 0.0583 | 0.0278 |
| min_id, sell all, donkey | 0.0565 | 0.0277 |
| seeded_random (uniform reference) | 0.0519 | 0.0255 |
| max_id, sell all, donkey | 0.0504 | 0.0254 |
| L4 anchor (richest, sell all, produce) | 0.0437 | 0.0214 |
| first_slot anchor | 0.0167 | 0.0105 |

The share threshold held everywhere: worst 0.0700 against the 0.08
budget, an order of magnitude under the unbalanced run-1 model game.
The HHI threshold did not: on held-out seed 20261303 the genome
richest|half-sell|donkey (and its unit-sell twin, an identical game)
reached HHI 0.0302 against the 0.03 line. The cross-path check through
the real world-run driver reproduced the harness numbers exactly (100
bribes, 46 recipients, share 0.07, HHI 0.0302; eight explicit skipped
decisions recorded from a provider restart, none rejected).

## Verdict

P1 is refuted on its secondary metric. Per the preregistered kill
criterion the transfer claim downgrades from "constraint beats
detection" to "the bounded menu holds against the enumerated policy
families": the rotation cap on the largest recipient share survived
exhaustive search of the influential subspace (everything at or under
0.07 versus 0.8056 unbalanced), but the recipient HHI line is reachable
by a legal attacker that pairs richest-visible targeting with the
donkey action and slow selling. The breach is marginal (0.0302 vs
0.03, one seed of five) and was found by enumeration, not evolution;
both facts are part of the record, and the downgrade stands as written.

Coverage statement: 42 of 42 genomes of the influential subspace, 100
percent; the non-influential genes were held at anchor values per the
preregistration, and their single-gene deviations were screened below
the 0.005 influence threshold.

## Limits

Single-gene screening may hide interactions of non-influential genes
with each other (not with the anchor); the space excludes policies
conditioning on round, cash thresholds or opponent history; one
breaching seed of five means the HHI line is near the reachable edge,
not mapped. The donkey action's economic effect (goods transport
changing bribe affordability rhythm) deserves its own decomposition
before any mechanism response.

Artifacts: [oat.json](world100-attack-20261114/oat.json),
[enumerate results](world100-attack-20261114/enumerate-t,s,alt.results.json),
[held-out results](world100-attack-20261114/heldout-genomes.jsonl.results.json),
[cross-path summary](world100-attack-20261114/crosspath-summary.json) and
[bribe report](world100-attack-20261114/crosspath-bribe-report.json).

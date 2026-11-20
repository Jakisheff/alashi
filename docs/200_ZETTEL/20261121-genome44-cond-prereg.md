---
id: 20261121-genome44-cond-prereg
tags: [world100, adversary, prereg, attack-coverage]
status: observed
---

# Prereg: round- and cash-conditioned attack policies (genome44), 21.11.2026 02:43

## What this closes

[WORLD100_EVO_ATTACK_20261114](../ops/WORLD100_EVO_ATTACK_20261114.md) Limits:
«the space excludes policies conditioning on round, cash thresholds or
opponent history». This run tests the first two of those three excluded
classes (opponent history stays out of scope). Harness:
[world-evo-attack-cond.mjs](../../tools/agent-bootstrap/world-evo-attack-cond.mjs),
a copy of the canonical harness whose `chooseForAgent` first resolves a
`cond` object into the static genes for the current decision point. Static
genomes pass through untouched.

## Calibration (done before this prereg was frozen)

Static anchor `{t:0,a:0,s:1,alt:1,law:0,k:0}` on seed 20261303 through the
new harness reproduced the published row bit for bit: bribes 100,
recipients 46, share 0.07, HHI 0.030200000000000008, 1806/0/0 accepted,
rejected, skipped. Reference row: `data/eval/evolved/world100-attack-20261114/heldout-genomes.jsonl.results.json`.

## Genomes (frozen before any conditional game)

| id | base | condition |
|---|---|---|
| C1 | richest, half, donkey | round_switch@4: richest → first_slot |
| C2 | first_slot, half, donkey | round_switch@4: first_slot → richest |
| C3 | richest, half, donkey | cash_threshold 20M: own cash > 20M → bribe richest + sell all; else never bribe (a:-1) + donkey + sell half |
| C4 | richest, half, donkey | round_switch@4: rounds 1-3 richest+half+donkey; from 4 first_slot + unit + donkey |

Seeds: train 20261201/03/05, held-out 20261301-05 (8 per genome, 32 games).
Thresholds as frozen in WORLD100_EVO_ATTACK: share 0.08, HHI 0.03.

## Hypotheses and frozen probabilities (Brier-scored)

- H1: no conditional genome exceeds share 0.08 on any of the 8 seeds.
  P(E1: share > 0.08 somewhere) = **0.10**. Rationale: the rotation cap held
  all 42 enumerated static genomes at or under 0.07; conditioning changes
  timing, not the per-recipient cap.
- H2: the HHI 0.03 line stays marginal: P(E2: some conditional genome has
  HHI > 0.03 on at least 2 of its 8 seeds) = **0.20**. Rationale: the static
  breach was one seed of five at 0.0302; switching late rounds toward
  first_slot disperses, and only C3 concentrates the bribe stream.
- H3: P(E3: worst share of any conditional genome > 0.0700 static worst) =
  **0.35**. Rationale: C3 shrinks the total bribe count (only cash-rich
  agents bribe) while keeping richest targeting, so share = max/total can
  rise mechanically even if the top recipient count does not.

Per-genome priors, P(worst share > 0.07 / HHI > 0.03 on >= 2 seeds):
C1 0.10/0.05, C2 0.15/0.10, C3 0.45/0.25, C4 0.10/0.05.

Kill criterion (inherited): any conditional policy breaching share 0.08 or
HHI 0.03 on 2+ seeds further downgrades the bounded-menu claim beyond the
20261114 verdict; no breach keeps it at «holds against known policy
families» extended to conditioned policies.

## Results (observed 21.11.2026 02:46)

Runs: 4 genomes x 8 seeds = 32 games, 25 s, `rejected=0, skipped=0`
everywhere. A first pass exposed a labeling bug (C1 and C4 shared a
genome id: the cond id did not encode the patches); after fixing the id
the full 32-game rerun reproduced every game bit for bit (checked
seed x genome x bribes x recipients x share x hhi against run 1), so the
games below are the original deterministic runs. Artifacts:
[cond/](../ops/world100-attack-20261121/cond/) (input, calibration,
results).

| genome | worst share | worst HHI | seeds with HHI > 0.03 |
|---|---:|---:|---|
| C1 richest→first_slot@4 | 0.0427 | 0.0175 | none |
| C2 first_slot→richest@4 | 0.0660 | 0.0306 | 20261203 (train, 0.0306) |
| C3 cash>20M gate | 0.5000 | 0.5000 | all 8 (bribes 2-3 per game) |
| C4 hybrid @4 (unit sell late) | 0.0427 | 0.0175 | none |
| static reference (20261114) | 0.0700 | 0.0302 | 20261303 (1 of 5) |

Frozen-event scoring (Brier):

- E1 share > 0.08 somewhere: **occurred** (C3, 0.50 on 7 seeds, 0.3333 on
  20261304). P was 0.10 → 0.81.
- E2 HHI > 0.03 on >= 2 seeds for some genome: **occurred** (C3 on 8 of 8;
  C2 on exactly 1). P was 0.20 → 0.64.
- E3 worst conditional share > 0.0700: **occurred** (C3, 0.50). P was
  0.35 → 0.4225. Mean Brier 0.6242: my priors were badly wrong.

Per-genome Brier (worst-share / HHI-2-seeds): C1 0.0100/0.0025 (held),
C2 0.0225/0.0100 (held on the >=2-seed reading), C3 0.3025/0.5625
(breached), C4 0.0100/0.0025 (held).

## Reading

C3's breach is a denominator collapse, not recipient concentration: the
20M gate admits only 2-3 bribers per game, so share and HHI are computed
over 2-3 bribes spread over 2-3 recipients (max 1 bribe per recipient —
zero concentration in absolute terms). The frozen thresholds carry no
minimum-bribe floor, so a near-pacifist policy trivially maxes the
normalized metrics. On full bribe streams (~106-133 bribes) no
conditional genome beat the static worst: C1/C4 at 0.0427/0.0175 and C2
at 0.0660/0.0306 all sit under 0.0700/0.0302, and late-round dispersal
(C1, C4) roughly halves concentration. C2 marginally crossed the HHI
line on one train seed (0.0306 vs 0.03), same marginal pattern as the
static breach on 20261303.

Per the kill criterion as written, C3 breaches share 0.08 and HHI 0.03
on 2+ seeds, so the downgrade of the bounded-menu claim stands. Both
facts are part of the record: formally the frozen metric is breached by
a conditioned policy; materially the menu held — no conditioned policy
concentrated a real bribe stream beyond the static worst, and the
round-conditioned classes (C1, C2, C4) stayed inside the static envelope.
Metric follow-up for any future coverage claim: pair share/HHI with an
absolute floor (e.g. minimum bribe volume or max per-recipient count)
before comparing to the 0.08/0.03 lines.

Side observation: C1 and C4 produced identical bribe streams on every
seed — half vs unit selling in rounds 4-6 changes nothing in the bribe
metrics, replicating the 20261114 «unit-sell twin is an identical game»
note on 8 seeds. Opponent-history conditioning remains the one excluded
class still untested.


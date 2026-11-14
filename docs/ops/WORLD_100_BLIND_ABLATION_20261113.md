# Wealth-visibility ablation and paired L4 run, 13-14 November 2026

Status: completed run of the ablation frozen in
[20261113-blind-ablation-prereg](../../200_ZETTEL/20261113-blind-ablation-prereg.md).
The question: does the legal richest-visible attacker exploit the
visibility of opponent wealth in the observation, or the structure of
offers itself? The preregistration froze predictions P1 (aiming dies
without the cash signal), P2 (no concentration breach under blinding)
and P3 (paired aiming effect on at least 15 of 20 seeds).

## Design

Canonical rules untouched; only the observation layer varies. The blind
driver copy zeroes the cash field of every selected_opponents row in the
prompt; selected_opponents is the only channel of opponent cash (the
world block carries no public_state). The l4-blind provider keeps L4
targeting when visible cash differs and falls back to a deterministic
uniform pick among offered targets when it does not. L4 (visible) ran on
the same twenty seeds as the uniform engine null for a paired design.
The within-offer percentile uses true engine cash from snapshots; the
raw percentile metric is tie-biased, and a midrank correction was
frozen as an amendment before the final blind seed was computed (see
the prereg for the timeline; blind seeds 20261109 and 20261113 are
post-hoc for that correction, 20261114 is fully pre-data).

## Results

| Prediction | Outcome |
|---|---|
| P1: blind arm percentile in 0.40-0.60 | Held: 0.4951 / 0.4889 / 0.4987 (midrank) on seeds 20261109/13/14 |
| P2: no permutation breach under blinding on more than one of three seeds | Held: p 0.96/0.46, 0.96/0.44, 0.44/0.44, zero breaches |
| P3: paired percentile difference positive on at least 15 of 20 seeds | Held: 20/20 positive, both raw and midrank |

Paired L4 versus uniform across twenty seeds
([paired-l4-uniform.json](world-100-blind-20261113/paired-l4-uniform.json)):

| Measure | Mean difference | Sign test | Wilcoxon p | Median bootstrap 95% CI |
|---|---:|---|---:|---|
| Cash percentile (midrank) | +0.2436 | 20/20 positive, p about 0 | 0.05 | [0.2363, 0.2764] |
| HHI | -0.0053 | 2+/18-, p 0.0002 | 0.065 | [-0.0075, -0.0026] |
| Largest share | -0.0095 | 4+/15-, p 0.021 | 0.147 | [-0.0138, -0.0018] |

Midrank percentile ranges never overlap: uniform 0.435-0.566 (mean
0.493), L4 0.690-0.749 (mean 0.736), blind 0.489-0.499.

## Findings

1. The attack surface is wealth visibility. Blinding the single cash
   channel kills the aiming signal completely: the blind attacker's
   chosen targets sit at the center of the wealth distribution of their
   offered sets, and no concentration breach remains. The defense of the
   bounded menu therefore has two independent levers: rotation caps what
   any aiming can achieve, and information design removes the aim.
2. Aiming is total and stable: on every one of twenty seeds the visible
   L4 picks targets about a quarter of the offer wealth range above the
   uniform arm. This is the strongest, cleanest effect in the whole
   ladder series.
3. Absolute concentration is diluted by activity, not raised by aiming:
   paired per seed, L4 shows lower HHI than uniform on 18 of 20 seeds
   because it bribes about four times more often. Between-arm
   concentration comparisons are confounded by volume; the conditional
   within-game criterion remains the correct detector. The earlier
   grid's finding stands with a sharper mechanism statement: rotation
   caps the damage, visibility supplies the aim, volume hides it from
   naive metrics.

## Limits

The blind arm ran three seeds, and the midrank correction was frozen
after two of them had raw values on disk (recorded above); only seed
20261114 is fully pre-data for P1. The fallback policy under blinding is
uniform-among-offers; an id-biased or adaptive attacker without wealth
sight is a different arm. Wilcoxon p-values at n=20 sit at the
normal-approximation floor for all-positive pairs; the sign test
carries the inference. The ablation changes the observation for the
attacker only; recipients' behavior under a blinded world is untested.

Artifacts: [world-100-blind-20261113/](world-100-blind-20261113/)
(three blind deep reports and the paired analysis). Every game passed
the driver's structural validation and replayed exactly.

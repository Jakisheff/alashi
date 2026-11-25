# Preprint notes: contamination argument and claims discipline

Notes for the G3 preprint (pass^k reporting, anti-cheating section).
Added 2026-11-24 per the trust-format import decision. Public sources
only.

## The contamination argument for private verifiable scenarios

A fixed public prompt that measures reasoning cannot stay a measurement:
search-equipped systems retrieve the benchmark itself and converge on the
answer without reasoning. The Carwash Test is the cleanest public record
of this lifecycle.

Citation-ready facts (all public, checked 2026-11-24):

- The Carwash Test, a single-question reasoning diagnostic, went viral in
  February 2026 before its canonical longitudinal record existed:
  Hacker News thread at 1516 points / 949 comments
  (news.ycombinator.com/item?id=47031580), an independent replication of
  53 models x 10 runs with only 5/53 stable
  (news.ycombinator.com/item?id=47128138), and multiple Reddit threads
  in the hundreds of points.
- The canonical record (carwashtest.org, v1.7; dataset 896 runs across
  32 vendors, 7 language corpora) documents the contamination event
  itself: on 2026-07-11 a search-equipped mode retrieved the test site
  and named the benchmark in its answer, while the same model's
  non-searching modes failed the question (Substack, "The Carwash Test,
  Part III", 2026-07-14).
- Recomputed from the site's published dataset (runs.json snapshot,
  2026-11-24): the English commercial failure rate fell from 33.3%
  (n=111, before 2026-07-11) to 12.8% (n=195, after). The decline mixes
  model-generation turnover with contamination; the record cannot and
  does not separate them. That is the point: a contaminated instrument
  stops measuring.

Preprint use: motivate scenarios that are private to a team, executed
under engine-verified conditions, where the question is not retrievable
because it has never been published. Public reusable mechanism cards
remain valuable and remain exposed to saturation; the preprint should
say both.

## Claims discipline for "model failed" statements

A human baseline is required before any claim that a model failure is
meaningful. Reference point: on the same forced-choice question, 71.5%
of a 10,000-person human panel answered correctly (Rapidata panel,
reported with the Opper replication above) - roughly a third of humans
fail it too. In world-100-grid the equivalent baseline role is carried
by the scripted engine arms (uniform, l4); model-arm results are read
against them, never as bare failures (see world-100-grid DATACARD,
"Transcripts, artifacts and baselines").

## Trust-format elements and where they land

- Verbatim transcripts: world-100-grid DATACARD pointers to per-run ops
  records; preprint appendix carries the failing model prompt verbatim
  (data/model-smoke-prompt.json).
- Deployment classes: never merge hosted API / local runtime / P2P
  results (DATACARD "Deployment classes").
- Dataset changelog: world-100-grid CHANGELOG.md; preprint cites the
  package version it used.
- Machine-readable export: parquet + checksums + verify.py; preprint
  reports the verify exit code for the cited snapshot.

## Related work: evolution strategies at LLM scale (added 2026-11-25)

Qiu, Gan, Hayes, Liang, Xu, Dailey, Meyerson, Hodjat, Miikkulainen,
"Evolution Strategies at Scale: LLM Fine-Tuning Beyond Reinforcement
Learning", ICML 2026 main, arXiv:2509.24372, code
github.com/VsonicV/es-at-scale.

Citation-ready facts (checked against the v3 full text):

- First full-parameter ES fine-tuning of multi-billion-parameter LLMs
  (Qwen-2.5 0.5B-7B, Llama-3 1B-8B, Qwen2.5-Math-7B, Qwen-2.5-14B) without
  dimensionality reduction; population size 30 with one fixed hyperparameter
  set across all runs, while RL baselines (PPO, GRPO, Dr.GRPO) received
  per-experiment grid searches.
- Countdown: ES above all RL variants on all seven models. Puzzles:
  ARC-AGI 0.2 -> 29.5, Sudoku 2.5 -> 69.5. Math benchmarks competitive with
  SOTA RL recipes.
- Reward hacking: GRPO with low KL penalty hacked the conciseness reward;
  ES never did, with no penalty. Mechanism stated by the authors: ES
  optimizes a solution distribution, and a single hacked solution lacks a
  high-quality neighborhood; RL optimizes one solution and token-level
  action noise can sample a hacking action.
- Mechanistic hypothesis: Gaussian noise in parameter space smooths jagged
  outcome-only reward landscapes; action-space noise does not.

Bridges for the pass^k preprint:

1. Methodological kin: world-100 attack arms (GenomeBot ladder, evolved
   genome adversaries) are evolutionary search over behavior against a
   non-differentiable rules engine; this paper is evolutionary search over
   parameters. Both optimize outcome-only rewards returned by an
   environment. The paper's own framing (long horizon, outcome-only,
   jagged rewards favor ES) describes the alashi environment structure.
2. External, published mechanism for reward-hacking differences by
   training paradigm; alashi measures analogous phenomena (bribe gaming,
   adversary concentration up to the cap). Agent reliability under
   economic pressure plausibly depends on post-training paradigm.
3. Small populations at scale (N=30 over billions of parameters) support
   the claim that large parallel agent worlds are cheap and informative
   (world-100 as population evaluation; scripted doubles before model
   agents mirrors ES population logic).

## Verification pass on the ES-at-scale citation (2026-11-25)

Checked before relying on the citation; sources are public.

**1. Reproducibility surface.**
- Code: github.com/VsonicV/es-at-scale — active (rewritten June 2026 on
  Ray + vLLM, claims 0.5B-72B trained); original paper scripts preserved
  in archive/ with run commands (accelerate launch; num_processes = GPUs).
  Requirements: Linux, CUDA, Python 3.12. No checkpoints of the paper's
  models found in the repo (HF links not verified this pass).
- Cloud cost for the minimal repro (Qwen-2.5-0.5B, Countdown,
  data_sample 200, N=30): order of 3-10 GPU-hours on a single rented
  A10/RTX4090 class card, roughly $3-10 — feasible, not attempted here
  (laptop has no CUDA).
- One methodological disclosure found in the repo: the original
  implementation used partially correlated noise; an i.i.d. variant was
  added after community discussion #7. The paper (v3) reports i.i.d.
  noise in the algorithm description.

**2. Peer review (OpenReview forum i0P4ew9GpS, ICML 2026, checked
2026-11-25 through the live page).**
- Decision: Accept (regular). Meta-review: "the evaluation is somewhat
  limited"; reviewer concerns about computational overhead addressed in
  rebuttal.
- Reviewer scores captured: Soundness/Presentation/Confidence around
  3/3/4, 3/3/3, 2/2/2, 3/3/4. Main weaknesses raised: (a) Countdown
  gains strong but math gains limited — generality question; (b) claims
  that ES fixes RL's instability/cost lack direct empirical evidence.

**3. Citation graph (searched 2026-11-25).**
- Independent follow-ups: EGGROLL "Evolution Strategies at the
  Hyperscale" (arXiv:2511.16652, different group: rank-r perturbations,
  100x speed-up, high-dim convergence theory); DART-ES (arXiv:2610.06993);
  a community fork testing ES on SSM architectures (Liquid LFM-2.5).
- Author-side follow-ups: "Blessing of Dimensionality" (arXiv:2602.00170,
  variance-curvature account of why N=30 works), "Overcoming Forgetting
  in LLM Fine-Tuning with ES" (arXiv:2605.30148).
- Critical recalibration by co-author Q. Liang (blog, 2026-11-04,
  qiyaoliang.com): characterizes the original results as *mixed* against
  RL; on 20 real tasks PG is ahead or tied because benchmarks sit at
  short effective horizons; ES's advantage shows on long horizons
  (Terminal-Bench: ES 12.1 -> 21.1% vs PG recipe 18.2% on Qwen3.5-4B,
  preliminary, unmatched budgets).

**Falsifiable core, status after verification:**
- C1 "ES optimizes in billion-parameter spaces with tiny populations":
  CONFIRMED at ecosystem level (independent groups, theory, tooling).
- C2 "ES outperforms RL": CONDITIONAL. Honest form: ES >= RL on
  long-horizon outcome-only tasks; on short effective horizons PG is
  ahead or tied. Cite in the conditional form only.
- C3 "ES resists reward hacking because it optimizes distributions":
  single experiment (conciseness), mechanism explicitly a hypothesis;
  follow-up theory exists, direct evidence pending.

## Minimal discriminating experiment (design, not run)

Question: does post-training paradigm change the pathologies alashi
measures? If ES-tuned agents show fewer of them at equal payoff, the
"training paradigm shapes agent reliability" bridge is evidence, not
analogy.

- Reward: alashi rules outcome, consumed in Python via wasmtime-py
  loading the committed rules-wasm artifact (frontend/src/rules-wasm;
  freshness-gated, browser-tested) — no new Rust code, canonical rules.
- Environment: single Qwen-2.5-0.5B agent vs scripted doubles
  (uniform and l4 GenomeBot arms from world-100), 6-round episodes,
  payoff at settlement = reward (outcome-only, delayed).
- Arms: (1) base model, (2) prompt-tuned baseline, (3) ES fine-tuned
  (es-at-scale framework, N=30, fixed hypers per paper).
- Horizon sweep: repeat at 2/6/12 rounds — the Liang effective-horizon
  account predicts the ES advantage grows with rounds; this is a
  pre-registered direction, not an assumption.
- Metrics (existing, from world-100): invalid-action rate (compliance
  cliff), bribe acceptance, donkey-style overholding; plus payoff.
- Budget: ~$15-30 cloud GPU (0.5B, ~20-30 GPU-hours) + 1-2 days
  harness work (wasmtime-py reward adapter).
- Kill criterion: ES arm shows no reduction in invalid-action rate vs
  base at equal payoff -> bridge stays a citation, the claim leaves the
  preprint.

## Pre-registered prediction (2026-11-25)

If a vendor ships an agent post-trained with evolution strategies
(vendor-stated or with a reproducible ES signature), alashi will measure
a lower invalid-action and reward-hacking rate for it than for
RL-post-trained peers of equal base capability, and the gap will widen
with episode length. Refutable in the live arena; dated here before any
such vendor release is known to us.

## Experiment run record (started 2026-11-25)

Environment: Brev GPU instance (GCP L4 24GB), owner credits; single process
es-train.py serving the model and perturbing weights in place; episode
harness = world-run.mjs (6 agents, 6 rounds, winner-takes-all) with the
focal model agent against five uniform scripted opponents via
mixed-provider.mjs (loopback-only, driver freeze untouched).

Baseline (raw Qwen-0.5B-Instruct, 6 episodes, seeds 1001-1006):
- focal_invalid: 6/18 decisions in every episode (33.3%, perfectly stable)
- focal_payout: 0 in all episodes; cash 0-15M of the 60M bank; ranks 2-6
- episode wall time ~2 min at 256 max tokens on L4

Scale deviation from the pre-registered protocol, recorded before the
training results are known: first run at reduced scale for an early
signal — population 15 (not 30), 1 episode per member (not 2),
max_new_tokens 64 (actions are short JSON; long ramblings are already
invalid), 30 iterations. Full-scale run is a follow-up decision.

Kill criteria unchanged: if the ES arm does not reduce focal_invalid at
equal or better payoff versus this baseline, the training-paradigm bridge
stays a citation and the claim leaves the preprint.

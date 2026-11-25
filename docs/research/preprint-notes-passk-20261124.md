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

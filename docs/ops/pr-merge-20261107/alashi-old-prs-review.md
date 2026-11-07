# Owner PR review, 7 November 2026

Reviewed against main `7113b9af330a9bce90b35919fdb37a4d3d20f230`. Read-only review; no branch, source, PR or deployment changes. Git merge-tree was used in trivial/read-only mode; it fetched missing historical Git objects but did not write a checkout or index. Temporary extracted documentation lives under `/tmp/alashi-old-pr-review`.

| PR | Exact head | Readiness |
|---|---|---|
| 59 | 48adeaf546e459ae9ed677476df536aa9ce2d8da | Content ready; resolve documentation append/link conflicts, then validate merged docs and CI. |
| 33 | 18650a65aff776b9da5e86a13c7d093c7b40d07c | Already included in main by ancestry. No code merge is needed. Its old draft targets codex/solo-opponent. |
| 24 | 05f19567ad814b7d079a29cb3063a6e64960c30f | No source defect found. Integration conflicts must be resolved, followed by current frontend/animation checks and rendered acceptance. |
| 25 | def87cdb4108099ebdef2bfce0f2a74d6c4b0a04 | No source defect found. Depends on PR24; retarget to main after PR24 integration and rerun frontend/animation checks plus rendered acceptance. |

All four exact heads have five successful GitHub checks: frontend/animations, Rust core, fresh SBF/program tests, RustSec and Gitleaks. These are historical head checks, not fresh tests of integration with today's main.

## PR59

Six changed files: journals, MOC-Research, and three new research documents. No executable source. The report explicitly dates its code snapshot to 523e14ac and separates observed fixtures from proposed protocols and unvalidated external results. No private manuscript is included. All 28 relative references in the three new documents resolve at the PR head; FULL text checks pass.

Four central primary-source claims were spot-checked against the source pages: ClawArena-Team v1, OrchBench v1, Kaggle Game Arena v1, and Anthropic Demystifying evals. The identified methods and dates are supported. This was a spot-check, not a new reproduction of the external research or revalidation of every URL.

Merge conflicts are append/link collisions in PROGRESS.md, STATUS.md and docs/100_MOC/MOC-Research.md. Preserve both histories and both sets of links. The three new research files apply cleanly. I found no content blocker to merging after that resolution.

## PR33

Both GitHub compare and local git ancestry establish that 18650a65 is already reachable from main. Compare reports ahead 0, behind 151. Main also contains later sponsorship fixes, including reduced targets and preservation of completed legacy funding records. The PR remains open against the stale codex/solo-opponent base. Do not replay the old diff or replace current sponsorship code. Closing it as already integrated would accurately describe the repository state; no new merge is needed.

## PR24 and PR25

The coin source and Dance rig additions are absent from main, so these are not duplicates. Their production code merges cleanly in the read-only three-way inspection. Conflicts against main are limited to PROGRESS.md, STATUS.md, docs/100_MOC/MOC-Ops.md and frontend/package.json. For the package conflict, retain every current check:animations step and add the brand-coin check; do not replace the current command with the old complete line.

PR24 changes the existing decorative coin factory and adds a bounded geometry/material/disposal check. It keeps the old contact envelope and shares no mutable materials between instances. Its dated visual report covers all Lasseter principles, BUY/SELL/Mule phone payment frames, pause and reverse seek. PR25 adds a local Dance reaction, alternating fists/weight shift and facial expression, with deterministic reduced-motion pose, neutral wrist/closed thumb tests and framing tests. It does not add a public gesture cue or game action. The visual report includes a preimplementation reference, all principles, phone poses, a desktop frame, seek/replay/interrupt checks and declared limitations.

I reviewed these reports and source diffs, but did not run a current merged browser scene, frontend build or check:animations. Existing screenshots and authors' prior statements cannot by themselves establish readiness of the integration with current main. Those checks remain the concrete acceptance step, especially where PR67 also modifies replay/rig behavior. PR25's clean mergeable status only applies to its old coin-canon base, not main.

## Sources

- https://github.com/Jakisheff/alashi/pull/59
- https://github.com/Jakisheff/alashi/pull/33
- https://github.com/Jakisheff/alashi/pull/24
- https://github.com/Jakisheff/alashi/pull/25
- https://arxiv.org/html/2606.31174v1
- https://arxiv.org/html/2607.25656v1
- https://arxiv.org/html/2609.31473v1
- https://www.anthropic.com/research/demystifying-evals-for-ai-agents

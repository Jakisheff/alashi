# PR 67 readiness review

Reviewed head: `5a80a6ff37de1eba7b238ebf91ff61fa49c327e5`.
Current main used for local comparison: `7113b9af330a9bce90b35919fdb37a4d3d20f230`.
Checkout remained clean; no source, branch, PR or production changes were made.

## Recommendation

Not ready for immediate merge. The current PR is a draft, GitHub reports `mergeable=false`, `mergeable_state=dirty`, and the changed reduced-motion behavior still lacks its required browser review. No new code correctness blocker was found in this review.

The read-only three-way merge against actual current main finds a conflict only in the append-only `PROGRESS.md` history. Production changes merge cleanly at this base. Resolve the journal by preserving both task records. Recheck the merge after any other PR changes main.

The remaining visual criterion is explicit in the PR body and `PROGRESS.md:1106`: browser/OS reduced motion is unverified. `docs/200_ZETTEL/20261010-lasseter-animation-canon.md:46` says readiness requires applicable acceptance criteria, and its acceptance list requires reduced-motion review. This PR changes reduced-motion frame selection and receipt timing in `frontend/src/live/market/MarketTrade.tsx:112`, `frontend/src/live/actions/ScenarioAction.tsx`, and `frontend/src/live/reactions/reactionClock.ts:34`. Therefore normal-motion frames do not establish acceptance for its main changed path.

## Evidence checked

Exact-head CI run https://github.com/Jakisheff/alashi/actions/runs/40896454625 has five successful jobs: fresh SBF and program tests; rules/arena/indexer/program compile; animation geometry/frontend build; secret scan; lockfile RustSec audit.

Six focused scripts were freshly executed with bundled Node 24 in the read-only PR checkout, all exit 0:

- `action-frame-pause-check.mjs`: real rig/GLB callbacks, pause/seek zero/resume/completion/reduced toggle/new take, visual time and receipt ownership.
- `reaction-frame-pause-check.mjs`: pause/replay/seek/completion, selected reduced pose, smoke/fireworks behavior, callback order, idle transition.
- `player-idle-pause-check.mjs`: live idle, paused replay idle/smoke, resume and parent playback wiring.
- `smoke-material-check.mjs`: shared material preservation and cleanup.
- `map-player-check.mjs`: playback controls, seeks/end/replay, missing canvas, asset failure/retry and stale callbacks.
- `player-replay-controls-check.mjs`: accessible controls, frozen history, pause/seek/replay and stale/repeated completion.

`git diff --check 7113b9af...HEAD` passes. Source review covered all production changes, their consumers and relevant tests. None changes game rules or backend sources.

Inspected four committed phone-width images: idle-1, idle-2, sale, map-final. They show consistent paused idle poses, visible sale actor/receipt and map controls. They are historical normal-motion captures, not a new browser run or reduced-motion evidence. No newly executed GPU/browser acceptance is claimed.

Historical quality measurements are disclosed in the PR: DegenieMap CRAP17, LivingGenie19.7 and MapPlaybackControls10, limited by scoped V8/mock/source-map coverage. Do not describe this as a clean CRAP gate or project-wide coverage score.

## Path to readiness

Preserve both journal additions when updating the PR to current main. Perform browser reduced-motion review for trade/scenario/reaction pause, seek-to-zero, resume and receipt/caption agreement at desktop and phone width, with local synthetic or saved data and no live transactions. Re-run frontend build/check:animations and required project checks on the integrated result, then convert from draft only if those pass.

PR67 touches the same StreamPage JSX and stream-staging VM context as fixes6/7. If both are integrated, preserve both `onUnavailable` renderer cleanup and `onVisualTime` receipt clock behavior; the staging fixture needs both names. The source-switch feed fix is separate.

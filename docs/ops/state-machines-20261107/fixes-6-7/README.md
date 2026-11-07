# Viewer state repairs 6 and 7

Started 7 November 2026 at 15:25 Asia/Almaty from `da57582b` on `codex/implicit-state-fixes-20261107`. Plan: 1.5 hours, until 16:55. Findings 3, 5 and 8 remain unchanged. [Previous evidence](https://github.com/Jakisheff/alashi/blob/3ea380b5c8cdfdf84decc816c5f5e9cdaf6920b8/docs/ops/state-machines-20261107/README.md) records the original reproductions.

This report preserves the completed 15:25 repair session on its original branch. The separate merge PR copies only viewer source and its test/evidence files onto main. Earlier session and archive production fixes are not part of that PR. Fresh integration checks and PR decisions are recorded in [the merge review](../../PR_MERGE_REVIEW_20261107.md).

## Viewer outcome and acceptance

The viewer follows confirmed public events. If the renderer fails, the public transcript remains available and visual scheduling releases the abandoned action. A failed render must not be reported as a completed animation. When the viewer switches a legacy game or source, old queued actions and delayed reactions stop; fresh events from the selected source still play.

Acceptance starts with the existing failing browser invariants, followed by the same assertions passing after each fix. Additional controls cover queued work at renderer failure, late completion, new events in fallback, fresh playback after switching, same game ID at another API, and unmount. Existing healthy controls remain. Browser fixtures use isolated loopback ports and synthetic confirmed records; external requests are blocked. Required Rust/SBF checks and frontend build/animation checks must pass. No live game, funds, merge or deployment is part of this task.

## Animation intent and reference before implementation

The visual reference is the existing viewer and the [recorded poses in the animation canon](../../../200_ZETTEL/20261010-lasseter-animation-canon.md). The normal intent remains act, then accepted/rejected for that same confirmed event. Existing anticipation, contact and consequence poses remain the target. The change concerns ownership and cancellation of that sequence when its renderer or event source disappears. Browser review will check the normal sequence and the changed boundaries at desktop and phone width; fixtures are labelled as synthetic.

| Lasseter principle | Application to this repair |
|---|---|
| Squash and stretch | Body/hand deformation is unchanged; no new pose is introduced. |
| Timing | Preserve normal action/result timing; cancel old-source timers when that source ends. |
| Anticipation | Preserve existing anticipation poses for healthy playback; do not invent a result after an abandoned action. |
| Staging | Clear abandoned overlays and prevent an old source from speaking over the newly selected source. |
| Follow through and overlapping action | Existing secondary motion remains; cancelled source work cannot schedule a later reaction. |
| Straight ahead and pose to pose | Existing key poses remain the reference; this repair adds no movement path. |
| Slow in and out | Existing interpolation is unchanged. |
| Arcs | Hand and prop trajectories are unchanged. |
| Secondary action | Preserve current facial/smoke motion; clear only output owned by the discarded feed. |
| Exaggeration | Existing exaggeration is unchanged. |
| Appeal | Preserve composition and character; fallback must state renderer unavailability while the transcript remains readable. |

Personality and event provenance: a reaction belongs to its original confirmed event. Clearing stale work must not fabricate a new game action or a success receipt. No general animation-quality approval is claimed by numerical lifecycle tests.

## Results

Both findings are fixed with observed RED before the production edits and GREEN afterward. The [existing tests failed in all five expected cases](existing-red.log), while both healthy controls passed. The [expanded recovery suite](regressions-green.log) passes nine checks and both healthy controls.

| Finding | Change | Checked behavior |
|---|---|---|
| 6, HTTP `/stream` | The renderer error boundary notifies its scheduler. The scheduler drops active/pending visual work and overlays, then declines visual events for that failed renderer's lifetime. | Failure during buy with sell queued returns to idle; a late completion cannot revive work. Later confirmed events remain in the transcript. A fresh stream mount plays normally. |
| 7, legacy feed | Playback, cursor, queue and delayed reactions belong to the selected mode/API/game. Layout cleanup cancels old work before the new source is ingested. Sample cleanup discards its old snapshot. | A-to-B, live-to-off, same game at another API, unmount and sample re-entry pass. New-source playback works. Cleanup preserves newer manually triggered speech and clips by checking their ownership IDs. |

Normal playback timing remains 3.2 seconds between legacy events and 1.1 seconds before their result. No game rules or backend source changed from `da57582b`; findings 3, 5 and 8 remain outside this repair. The existing renderer fallback lasts until the stream remounts. Recovery does not claim that an abandoned animation finished.

## Checks and reproduction

Frontend dependencies were restored from the existing lockfile with `npm ci --offline --ignore-scripts --no-audit --no-fund`; [262 packages installed](frontend-install.log). Dependency declarations and lockfiles are unchanged. Recorded browser runs use Node 24.22.1, Playwright 1.62.1 and Chrome 156.0.8078.81. See the [runner instructions](../../../../frontend/scripts/state-repros/README.md) for the module path override.

Run from `frontend/`:

```sh
node scripts/state-repros/run.mjs all
npm run build
npm run check:animations
node scripts/state-repros/visual-run.mjs
```

The build and all animation checks pass: [build](frontend-build.log), [animations](animations.log). Scoped lint exits zero with one existing startup-effect warning in StreamPage and one Fast Refresh warning in the standalone visual fixture: [production lint](frontend-lint.log), [visual lint](visual-lint.log). Vite still reports its existing large-chunk warning.

All mandatory Rust/SBF gates pass. Commands, durations, artifact hashes and both program-test attempts are retained in [rust-gates.json](rust-gates.json).

| Command | Result | Log |
|---|---|---|
| `cargo test --offline --locked -p alashi-rules` | 40 passed | [rules](rules.log) |
| `cargo test --offline --locked --manifest-path arena/Cargo.toml` | 157 passed, 1 existing canonical-snapshot test ignored | [arena](arena.log) |
| `cargo test --offline --locked --manifest-path indexer/Cargo.toml` | 22 passed | [indexer](indexer.log) |
| `cargo check --offline --locked -p alashi` | Passed | [program check](program-check.log) |
| `cargo build --offline --locked --manifest-path bots/Cargo.toml` | Passed | [bots](bots-build.log) |
| `bash tools/build_sbf.sh` | Passed with pinned 4.1.0 / platform-tools v1.54 | [SBF](sbf-build.log) |
| `cargo test --offline --locked -p alashi`, after SBF | 37 passed, including both full-party replay tests | [program retry](program-tests-retry.log) |

Host checks used `CARGO_TARGET_DIR=target/host`, `CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, and dev/test debug information disabled. The host deploy path resolves to the fresh SBF artifact. Its SHA-256 before and after execution is `302885b13f9040f536e266c36d44f31df1e128bb991f8319b57e62d3f0827fcc`.

The first program-test compile failed with incompatible cached Anchor trait identities. The [failed attempt](program-tests.log) and [diagnosis](program-cache-diagnosis.log) remain. Cleaning only this task's `alashi` host package artifacts removed 49 files (58.7 MiB); the unchanged command then passed. [Scoped cleanup](program-cache-clean.log). No Rust source or lockfile was changed to obtain that result.

The first expanded browser run exposed a fixture timing error in sample re-entry: one large virtual-clock advance batched React's ingestion effects. The harness now waits for an effect marker before advancing again. [Initial attempt](regressions-first-attempt.log), [diagnostic check](sample-timing-check.log). Production code did not change for that timing diagnosis.

## Browser review

The separate fixture uses the real Scene, MarketTrade, LivingGenie and legacy GenieModel on WebGL 2.0 through ANGLE Metal / Intel UHD 617. All four scenarios pass at 1366 and 390 pixels. [Run log](visual-run.log), [observations and event traces](visual/observations.json).

| Scenario | Desktop evidence | Phone-width evidence |
|---|---|---|
| Buy completes; failure during sell clears active work; later transcript event arrives | [Recording](visual/stream-1366.webm), [buy](visual/stream-1366-buy-result.png), [fallback](visual/stream-1366-fallback-feed.png) | [Recording](visual/stream-390.webm), [buy](visual/stream-390-buy-result.png), [fallback](visual/stream-390-fallback-feed.png) |
| Legacy act/result/idle; source switch clears old speech; new source plays | [Recording](visual/legacy-1366.webm), [clean boundary](visual/legacy-1366-b-boundary-idle.png), [new result](visual/legacy-1366-b-result.png) | [Recording](visual/legacy-390.webm), [clean boundary](visual/legacy-390-b-boundary-idle.png), [new result](visual/legacy-390-b-result.png) |

Frames and recorded transitions were inspected. The buy frame preserves the visible held crate and receipt; the fallback removes the receipt and retains the confirmed feed. At the source boundary the legacy character is idle with no old text; the new source then performs act and accepted before returning idle. On the narrow screen earlier receipts remain in the scrollable feed while the newest events are visible. There were no page errors or external requests; stream console errors are the deliberately injected failure.

This checks the changed lifecycle, staging and timing with synthetic records. It is not a physical-phone test, GPU reliability test, performance measurement or a new acceptance of every pose/contact. No geometry, camera, key poses or interpolation were edited. Initial fixture-only failures are retained: [script setup](visual-fixture-initial.log), [development optimizer](visual-fixture-optimizer.log), [short-clip observation timing](visual-fixture-timing.log). The final fixture uses a built preview and transition traces.

## Focused quality evidence

Independent reviews of both production changes found no blocker. Three selected guard mutations were tested in isolated source overlays: renderer-failure latch, speech ownership and clip ownership. Two baselines passed; all three mutants failed behavioral assertions. Healthy controls passed in all five runs. No mutation run failed because of compilation, infrastructure or timeout. [Independent classification and raw-log links](mutation-classification.json), [stream driver](mutation-stream.log), [feed driver](mutation-feed.log), [logging wrapper](mutation-probe.py). This is a focused mutation check, not a project-wide score.

The runner binds imports to the selected checkout and verifies transformed source hashes, so symlinked mutation overlays cannot silently exercise the original source. The [complexity report](complexity.log) records ConnectedStream CC 93 and useArenaFeed CC 36; coverage and CRAP were not measured. [Duplicate detection](duplication.log) found no candidates at threshold 0.8. Broader structural refactoring remains outside this repair.

## Publication

The fixes, tests and run evidence were pushed as [0dafe83c](https://github.com/Jakisheff/alashi/commit/0dafe83c2d2d1e6a15d27a58cfca5a680d2fd6d7) to `codex/implicit-state-fixes-20261107`; GitHub returned that exact commit. The final [scoped lint](final-lint.log) and script syntax checks passed. Staged and committed secret scans found no leaks. Documentation links were checked, and the new prose passed FULL cleaning and manual fact/rhythm review. Diagnostic logs retain their content; trailing whitespace was trimmed before commit. Final journal and audit cross-reference updates follow separately.

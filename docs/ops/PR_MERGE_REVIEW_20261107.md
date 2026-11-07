# Owner PR review and merge

Started 7 November 2026 at 16:05 Asia/Almaty. План: 2 часа, до 18:05. Initial main: `7113b9af330a9bce90b35919fdb37a4d3d20f230`.

The owner authorized reviewing their open PRs and merging ready changes into main, plus a separate PR for viewer findings 6 and 7. PRs authored by other accounts are outside this task. Game decisions 3, 5 and 8 remain unchanged. No live games, funds or deployment.

The viewer PR copies the two production files, staging fixture, lifecycle/visual test harness and historical viewer evidence from `3ea380b5`. Session and archive fixes from that branch are excluded. Acceptance: failed renderer releases visual work without losing confirmed history; switching legacy sources cancels stale work and permits fresh playback. Prior RED/GREEN and visual evidence remain in [the repair report](state-machines-20261107/fixes-6-7/README.md). Fresh integration checks go in [the log directory](pr-merge-20261107/).

## Decisions so far

| PR | Result | Evidence |
|---|---|---|
| [68](https://github.com/Jakisheff/alashi/pull/68) | Merged as `90d515fd123a8e68dd16bf46f4509c80e8f1931a` | Independent source/workflow review and all five exact-head CI checks passed. The new workflow runs only manually or on release tags; merging does not deploy. [Review](pr-merge-20261107/pr68-review.md) |
| 59 | Merged as `0db54f864d63d593937200f3d3c1d2a1fcd00390` after all five fresh CI checks passed | Both journal histories and topic-map links preserved. 63 relative links, FULL and Gitleaks pass. [Resolution](pr-merge-20261107/alashi-pr59-resolution.json) |
| [67](https://github.com/Jakisheff/alashi/pull/67) | Merged as `8c1dffb4267083bcbab6b084769846ada0f70670` | Final head `1244cc9f` passed all five CI jobs. 24 pause/seek and 24 resume cases passed at desktop/phone sizes. [Browser acceptance](pr-merge-20261107/pr67-visual/ACCEPTANCE.md), [CI proof](pr-merge-20261107/pr67-final-ci.json). |
| [24](https://github.com/Jakisheff/alashi/pull/24) | Merged as `0422a03678cbe3a434f79c39bff612ce3f833d02` | Final `f48a44ac` passed all five checks. [Coin/Dance acceptance](pr-merge-20261107/pr24-25-visual/ACCEPTANCE.md), [CI proof](pr-merge-20261107/pr24-final-ci.json). |
| [25](https://github.com/Jakisheff/alashi/pull/25) | Merged as `96e9ea951f70628b9b40075ba7f37ec74c6cc8a7` | Final `591966c0` passed all five CI jobs. Runtime remains identical to accepted `c0e74cd8`. [CI proof](pr-merge-20261107/pr25-final-ci.json). |
| 33 | Already integrated | Original head `18650a65` is an ancestor of initial main; no repeat code merge needed. Closed as already integrated; no code was replayed. |
| Viewer 6/7 | [PR69](https://github.com/Jakisheff/alashi/pull/69), final candidate on main `96e9ea95` | Nine recovery checks, two controls, four real-renderer cases, build and all animation checks pass. Mandatory runtime CI jobs passed; final all-five CI gate follows the evidence commit. |

The viewer PR was moved onto main after PR68 before committing. Application and lockfile content from PR68 does not change the viewer validation inputs. Earlier session/archive fixes stay on `codex/implicit-state-fixes-20261107`.

## Combined viewer verification

Runtime commit `321bf71d86ddf7fa683b5bd7fac55f4cfa5e4235` combines replay, coin, Dance and viewer recovery. Its frontend is identical in `74ca4c53`, which adds the actual PR67 main merge and evidence. The two tests added to the coin branch only verify the existing recessed field geometry.

The production frontend build and complete animation suite passed. The suite includes 1532 trade-contact frames, 827 legacy frames, 1256 reaction frames and 5196 scenario frames, plus stream, replay, smoke and map checks. The exact local `cargo test --locked -p alashi-rules` invocation passed all 40 tests. Logs: [build](pr-merge-20261107/final-build.log), [animations](pr-merge-20261107/final-animations.log), [rules](pr-merge-20261107/final-rules.log).

All nine lifecycle recovery invariants and both healthy controls passed on the combined source. Four separate real-renderer cases passed: stream failure and legacy source switching at widths 1366 and 390. No page errors or external requests occurred. Each stream case records its deliberate renderer exception. [Lifecycle log](pr-merge-20261107/final-lifecycle.log), [rendered results and recordings](pr-merge-20261107/viewer-final-visual/observations.json), [independent frame-sequence review](pr-merge-20261107/viewer-final-visual-review.md).

These checks establish the changed boundaries with synthetic local data. They do not establish physical-phone performance, recovery from an actual GPU driver failure or first-viewer comprehension. Historical scoped CRAP measurements and classified mutation survivors remain disclosed in their original reports. No clean project-wide quality score is claimed.

## Mandatory execution and checksum scan

[Full mandatory CI logs](pr-merge-20261107/ci-42370377477/README.md) include rules/workspace, arena, indexer, program compilation, bots, fresh SBF followed by program tests, and all five requested Node files (91 passes, zero failures or skips). The archived run correctly retains its overall failed status: the runtime jobs passed, while Gitleaks found historical checksum strings.

All eight flagged values were independently recomputed from their named same-commit Git files by the reviewer and root. The four historical commits already belonged to main. PR24 added only eight exact commit/path/rule/line exceptions and preserved three existing exceptions. The same local range changed from eight findings to zero; GitHub's Gitleaks 8.24.3 then passed on the final PR24 head. [Classification and red/green proof](pr-merge-20261107/pr24-gitleaks-checksums.md), [root verification](pr-merge-20261107/checksum-independent-verification.log).

CI log publication removes trailing spaces only and retains original download hashes alongside published hashes. One trailing reporter space in the coin build log is normalized with the same original/published distinction in its manifest. No application behavior changes in these evidence updates. The final PR69 head must pass all five CI jobs before merge.

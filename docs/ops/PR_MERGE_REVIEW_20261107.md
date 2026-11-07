# Owner PR review and merge

Started 7 November 2026 at 16:05 Asia/Almaty. План: 2 часа, до 18:05. Initial main: `7113b9af330a9bce90b35919fdb37a4d3d20f230`.

The owner authorized reviewing their open PRs and merging ready changes into main, plus a separate PR for viewer findings 6 and 7. PRs authored by other accounts are outside this task. Game decisions 3, 5 and 8 remain unchanged. No live games, funds or deployment.

The viewer PR copies the two production files, staging fixture, lifecycle/visual test harness and historical viewer evidence from `3ea380b5`. Session and archive fixes from that branch are excluded. Acceptance: failed renderer releases visual work without losing confirmed history; switching legacy sources cancels stale work and permits fresh playback. Prior RED/GREEN and visual evidence remain in [the repair report](state-machines-20261107/fixes-6-7/README.md). Fresh integration checks go in [the log directory](pr-merge-20261107/).

## Decisions so far

| PR | Result | Evidence |
|---|---|---|
| [68](https://github.com/Jakisheff/alashi/pull/68) | Merged as `90d515fd123a8e68dd16bf46f4509c80e8f1931a` | Independent source/workflow review and all five exact-head CI checks passed. The new workflow runs only manually or on release tags; merging does not deploy. [Review](pr-merge-20261107/pr68-review.md) |
| 59 | Merged as `0db54f864d63d593937200f3d3c1d2a1fcd00390` after all five fresh CI checks passed | Both journal histories and topic-map links preserved. 63 relative links, FULL and Gitleaks pass. [Resolution](pr-merge-20261107/alashi-pr59-resolution.json) |
| 67 | Source and browser acceptance complete; final evidence-head CI pending | 24 pause/seek and 24 resume checks passed at desktop/phone sizes. Evidence committed as `1244cc9f`; runtime matches refreshed green head `1605f03f`. |
| 24, 25 | Source, geometry and desktop/phone visual review complete; publication and CI pending | Current animation checks preserved. Mutation review added a failing-then-passing field/rim geometry assertion; reduced-motion Dance uses PR67. |
| 33 | Already integrated | Original head `18650a65` is an ancestor of initial main; no repeat code merge needed. Closed as already integrated; no code was replayed. |
| Viewer 6/7 | [PR69](https://github.com/Jakisheff/alashi/pull/69) created | Nine fresh lifecycle checks and two controls, frontend build and all animation checks pass. Updated onto main after PR59; mandatory PR CI follows. |

The viewer PR was moved onto main after PR68 before committing. Application and lockfile content from PR68 does not change the viewer validation inputs. Earlier session/archive fixes stay on `codex/implicit-state-fixes-20261107`.

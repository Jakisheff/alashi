# State-machine regression evidence

This report preserves the first task's results through `da57582b`. A later owner instruction authorized fixing 6 and 7; those fixes and their checks are recorded in the [viewer repair report](fixes-6-7/README.md). The reproduced-only rows below describe the first task, before those fixes.

Work started on 7 November 2026 at 11:20, Asia/Almaty, from `7113b9af330a9bce90b35919fdb37a4d3d20f230`. Branch: `codex/implicit-state-fixes-20261107`. This directory accompanies the [original audit](../IMPLICIT_STATE_MACHINES_20261107.md).

Scope: fix findings 1 and 2 after reproducing them; reproduce findings 6 and 7 without fixing them; leave the rules involved in findings 3, 5 and 8 unchanged. Tests use isolated files and loopback services. No live game, funds or deployment is involved.

| Finding | Result in the first task | Evidence |
|---|---|---|
| 1. Rejected session action stays pending | Fixed after RED; 19 session tests pass | [RED](finding-1-red.log), [GREEN](finding-1-green.log) |
| 2. Terminal cache prevents archive retry | Fixed after RED; 22 indexer tests pass | [RED](finding-2-red.log), [GREEN](finding-2-green.log), [suite](indexer.log) |
| 6. Stream scheduler stays busy after renderer failure | Reproduced, no production fix | [Observation](finding-6-reproduced.log), [failing recovery invariant](finding-6-assert-fixed.log) |
| 7. Legacy queues survive game/source switches | Four cases reproduced, no production fix | [Observations](finding-7-reproduced.log), [failing recovery invariants](finding-7-assert-fixed.log) |
| 3, 5, 8. Roof, forecast, barter | Rules unchanged; decision options prepared | [Choices and costs](product-choices.md) |

## Node environment

Installed the existing lockfile with `npm ci --offline --ignore-scripts --no-audit --no-fund` in `tools/agent-bootstrap/`. No dependency declaration or lockfile changed. Node: v22.17.0. The installed `@solana/web3.js` version is 1.99.0, supplied by the lockfile. [Install log](node-install.log).

```sh
node --test tools/agent-bootstrap/alashi.test.mjs \
  tools/agent-bootstrap/strategy-run.test.mjs \
  tools/agent-bootstrap/strategy-scheduler.test.mjs \
  tools/agent-bootstrap/strategy-comparison.test.mjs \
  tools/agent-bootstrap/local-experiment.test.mjs
```

Observed result: all five files load; 91 tests pass, zero fail. [Successful log](node-five-green.log). The [first run](node-five.log) had 87 passes and four `listen EPERM` failures because the sandbox denied loopback listeners. The same command passed with permission for local mock ports. No test or application code was changed to obtain that result.

## Rust build environment

Use the repository's pinned Rust 1.89.0. Host runs share `target/host`, with `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, and at most two Cargo jobs for subsequent runs. These settings limit local disk use; they do not skip tests. SBF uses the repository script's separate target and pinned toolchain.

| Required command | Observed result | Log |
|---|---|---|
| `cargo test --locked -p alashi-rules` | 40 passed | [rules.log](rules.log) |
| `cargo test --locked --manifest-path arena/Cargo.toml` | 157 passed, 1 ignored | [arena.log](arena.log) |
| `cargo test --locked --manifest-path indexer/Cargo.toml` | 22 passed | [indexer.log](indexer.log) |
| `cargo build --locked --manifest-path bots/Cargo.toml` | Passed | [bots-build.log](bots-build.log) |
| `bash tools/build_sbf.sh` | Passed with 4.1.0 / v1.54 | [sbf-build-resumed.log](sbf-build-resumed.log) |
| `cargo check --locked -p alashi` | Passed | [program-check.log](program-check.log) |
| `cargo test --locked -p alashi` | 37 passed, including both full-party replay tests | [program-tests.log](program-tests.log) |

Host Cargo commands also use `--offline`; required lockfiles are unchanged. Arena's one ignored test requires an explicitly supplied canonical snapshot. It was left ignored. The program result is 4 unit tests, 28 game tests and 5 replay tests. Both `replay_equivalence_full_party` and `replay_epoch_90s_full_party` passed. These are local checks; no GitHub Actions result is claimed.

[Artifact metadata](sbf-artifact.json) identifies the fresh program used for the execution tests. Its SHA-256 remains unchanged [after testing](program-gates.json). [The first SBF attempt](sbf-build.log) failed to download inside the sandbox; [the subsequent attempt](sbf-build-interrupted.log) was interrupted before producing an artifact. Neither is counted as a successful build.

## Finding 1: definitive rejection releases pending

The [RED test](finding-1-red.log) failed before the production change: `bad_action must release the rejected action`. The fix follows the existing Node client contract. It clears pending only when the response has the same operation ID, explicit `ok: false` and `op_consumed: false`, and `bad_action`, `bad_params` or `bad_choice`. The operation ID remains available for the corrected action. The consumed-success path is unchanged.

All 19 session tests pass in the [GREEN run](finding-1-green.log). The new tests cover three definitive rejection codes and 12 ambiguous or malformed replies, plus connection refusal and a server that receives the request then drops its response. The uncertainty cases preserve the full normalized request and reject replacement actions before retry. Existing authentication rejection coverage also passes. Files and listener ports belong to each test fixture.

[Dryer](finding-1-duplication.log) found no duplicate candidates. [Crapper](finding-1-complexity.log) reports `act_v2` complexity increasing from 20 to 25 with the guarded rejection decision. Coverage and CRAP were not measured. All [eight selected mutations](finding-1-mutation-sites.log) were detected in the [verified run](finding-1-mutation-verified.log), after a passing baseline. [Per-run output](bots-mutation-runs/) records the changed condition and the failing behavior test.

## Finding 2: terminal archive retry fixed

Before the fix, a failed archive rename left the terminal game cached without `archived: true`. Removing the filesystem obstruction did not trigger another write. The regression failed for both fresh and stale cache entries: [fresh-cache RED](finding-2-red.log), [stale-cache RED](finding-2-red-stale.log). The earlier [environment failure](finding-2-environment.log) was a denied loopback listener and does not count as a reproduction.

The cache fast path now requires a successfully archived terminal snapshot when archival is configured. A request for an unarchived terminal snapshot takes the existing per-game lock, retries the write from that same cached value, and updates the cache. It does not fetch the game again.

The [passing regression](finding-2-green.log) checks recovery for both cache ages, concurrent retry requests, archive loading by a new API instance, and zero RPC connections. Controls preserve the fresh live-game fast path and prevent rearchiving an already archived cache entry. The full indexer suite also passes.

[Crapper](finding-2-complexity.log) reports complexity only: `get` is 9 and `ready_value` is 4. Coverage and CRAP were not measured. [Dryer](finding-2-duplication.log) reports no duplicate candidates.

The [verified mutation run](finding-2-mutation-verified.log) has a passing baseline and five compiled mutants detected by the regression. [Per-run output](indexer-mutation-runs/) distinguishes the observed failures: unwanted RPC for a fresh live cache, failure to retry the archive, and unwanted rearchiving that consumes the sentinel temporary file. There were no compiler failures or test-runner timeouts. The [initial mutation output](finding-2-mutation.log) lacked subprocess diagnostics; the detailed rerun resolves that evidence gap.

## Mutation evidence

The [classification](mutation-classification.json) covers 13 selected mutations in the changed guards, with two passing baselines. Each mutant compiled and then failed a behavior test; none failed during compilation or exceeded the test runner timeout. This selection does not measure whole-file coverage or a whole-file mutation score.

The [logging wrapper](mutation-probe.py) preserves each probe's source hash, source diff, Cargo output and exit status outside the temporary mutation checkout. Run from the repository root with the host build environment described above:

```sh
mutator --no-coverage --mutate-all --max-workers 1 --timeout-factor 1000 \
  --lines 131 --verbose \
  --test-command 'python3 docs/ops/state-machines-20261107/mutation-probe.py indexer' \
  indexer/src/chain_api.rs
mutator --no-coverage --mutate-all --max-workers 1 --timeout-factor 1000 \
  --lines 932,933,934 --verbose \
  --test-command 'python3 docs/ops/state-machines-20261107/mutation-probe.py bots' \
  bots/src/session_cli.rs
```

## Findings 6 and 7: reproduced, not fixed

The [browser harness](../../../frontend/scripts/state-repros/README.md) runs actual production React modules with isolated API data and an injected renderer error. Its healthy controls pass. Default commands verify that the bugs occur; `--assert-fixed` checks the requested recovery invariant and must currently fail. The probes are separate from normal CI.

| Finding | Observed behavior | Reproduction | Missing recovery invariant |
|---|---|---|---|
| 6 | The real stream error boundary catches the child failure during buy. Sell reaches the feed, but buy still occupies the scheduler after 20 simulated seconds. | [Control and bug observation, exit 0](finding-6-reproduced.log) | [One assertion fails, exit 1](finding-6-assert-fixed.log) |
| 7 | Old A reactions continue after A-to-B and live-to-off. Both queued actions and an already scheduled result callback survive the switch. | [Control and four bug observations, exit 0](finding-7-reproduced.log) | [Four assertions fail, exit 1](finding-7-assert-fixed.log) |

Runtime: Node v24.22.1, Playwright 1.62.1, Chrome 156.0.8078.81. [Harness syntax, scoped lint and text checks](frontend-harness-checks.log) pass. No production frontend file, package manifest or lockfile changed. The renderer error is injected; this is not evidence of a particular GPU failure or visual animation acceptance.

After all browser processes exited, this task's freshly installed `frontend/node_modules` was removed to recover 384 MB for Rust/SBF checks. Reinstall the frontend lockfile before repeating the browser commands. Agent-bootstrap dependencies remain installed.

## Product decisions

Findings 3, 5 and 8 are unchanged. [Options and costs](product-choices.md) distinguish documenting current behavior, adding guards, and introducing cancellation or expiry. These require an owner's decision before implementation.

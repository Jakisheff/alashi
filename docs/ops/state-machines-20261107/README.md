# State-machine regression evidence

Work started on 7 November 2026 at 11:20, Asia/Almaty, from `7113b9af330a9bce90b35919fdb37a4d3d20f230`. Branch: `codex/implicit-state-fixes-20261107`. This directory accompanies the [original audit](../IMPLICIT_STATE_MACHINES_20261107.md).

Scope: fix findings 1 and 2 after reproducing them; reproduce findings 6 and 7 without fixing them; leave the rules involved in findings 3, 5 and 8 unchanged. Tests use isolated files and loopback services. No live game, funds or deployment is involved.

## Node environment

Installed the existing lockfile with `npm ci --offline --ignore-scripts --no-audit --no-fund` in `tools/agent-bootstrap/`. No dependency declaration or lockfile changed. Node: v22.17.0. `@solana/web3.js` is supplied by the lockfile. [Install log](node-install.log).

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

`cargo test --offline --locked -p alashi-rules`: 40 tests pass. [Log](rules.log).

Regression results and remaining mandatory checks are pending. No finding is marked fixed by this environment entry.

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

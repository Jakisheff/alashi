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

## Product decisions

Findings 3, 5 and 8 are unchanged. [Options and costs](product-choices.md) distinguish documenting current behavior, adding guards, and introducing cancellation or expiry. These require an owner's decision before implementation.

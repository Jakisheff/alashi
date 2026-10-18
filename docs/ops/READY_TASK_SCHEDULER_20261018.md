# Dependency scheduling acceptance

18 October 2026. Base revision: `4026817`. The owner requested removal of the
whole-group wait while preserving budget checks and crash recovery.

## Behavior

Observed: the local runner starts a task after its own dependency reports are
accepted with known usage. An independent slow task no longer delays that start.
The server still reserves each call against known consumption plus active
reservations. No game rule, action schema or animation contract changed.

```text
fast -> follow -> deliver
slow ----------> deliver
```

Previously, `follow` waited for `slow`. The regression test holds `slow` open
with a controlled promise and verifies that `follow` starts before its release.
The original runner fails this check. The updated runner passes it.

## Failure and recovery

A failed reservation prevents that task from calling its provider. Once the
runner observes a failure, it stops new calls and drains started work before
releasing the journal lock. A reservation response received after that failure
cannot start another call. Known consumption remains recorded even when another
task has unknown usage or exceeds its reservation.

Resume reconciles saved responses before starting new work. It restores accepted
dependency reports from the server, including when the server accepted a report
whose HTTP response was lost. A stopped run can still report previously paid
work. An ambiguous inflight call stays unknown and is never called again.
Prepared reservations can resume once; completed calls are not reserved again.
Saved responses can be reported after the deadline, but new calls cannot start.

Concurrent report responses merge only their own task. A delayed older response
therefore cannot erase a newer accepted dependency. The runner reloads the full
server state before trading or finishing.

Observed: the complete arena suite passes 137 tests, including the signed HTTP
dependency-order check and the trading cycle with lost-response recovery. One
read-only canonical-snapshot test is explicitly ignored by the project.
[Full arena log](ready-task-scheduler-20261018/arena.log).

## Before and after

Observed: the original runner passed 8 of the 11 initial regression tests.
Failures covered the group barrier, recovery after a lost accepted report and
reconciliation on a stopped run. The completed Node suite passes all 44 tests,
including additional active-budget and deadline checks.

The timing fixture uses the same four tasks and identical reports in both
versions. Provider delays are 30 ms for `fast`, 200 ms for `slow`, 150 ms for
`follow` and 10 ms for `deliver`. Both versions make four calls and record
40 synthetic tokens per run. No external model is called.

| Runner | Samples, ms | Median, ms |
| --- | --- | ---: |
| Original | 873, 748, 637 | 748 |
| Updated | 580, 457, 428 | 457 |

Observed median reduction: 38.9% in this local fixture. Filesystem costs and
concurrent compilation affect these wall-clock measurements. Three samples
per version do not establish a production latency distribution. The controlled
promise test separately establishes the changed dependency ordering.

Raw results and source hash: [summary](ready-task-scheduler-20261018/summary.json),
[baseline regression](ready-task-scheduler-20261018/baseline-regression.tap),
[Node suite](ready-task-scheduler-20261018/node-suite.tap),
[timing before](ready-task-scheduler-20261018/timing-before.tap) and
[timing after](ready-task-scheduler-20261018/timing-after.tap).

## Reproduce

From the repository root:

```sh
node --test tools/agent-bootstrap/strategy-run.test.mjs tools/agent-bootstrap/strategy-scheduler.test.mjs tools/agent-bootstrap/alashi.test.mjs
node --test --test-name-pattern 'fixed workload timing' tools/agent-bootstrap/strategy-scheduler.test.mjs
cargo test --locked --manifest-path arena/Cargo.toml
```

For the original timing, copy `strategy-run.mjs` and `alashi.mjs` from revision
`4026817` into a temporary directory alongside the current scheduler test file.
Use the same installed bootstrap dependencies and timing filter. The baseline
regression log predates the final additional cases; its 11 tests are preserved
as the recorded reproduction, rather than reported as the final 15-case suite.

The signed HTTP duel now uses an audience review dependent on one report while
an independent angle task runs longer. It asserts overlap at the provider and
retains lost-report recovery checks. The ordinary trading-cycle test covers
accepted actions and retry without a second trade.

Not validated: production latency, real-provider cost savings or a fresh Solana
match. Deployment is outside this change.

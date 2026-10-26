# Private comparison repairs, 26 October 2026

The owner must be able to inspect the two results of a frozen hypothesis and record a decision after execution or an interrupted attempt. The targeted audit of `e413a0809cc625c172d393d313fddf48fd997a32` found three failures in that path. This change repairs the existing static advertising scenario.

```text
Human hypothesis and decision rule
→ authorized preparation with the actual instruction difference
→ one executor claim for each task
→ saved response or explicit unresolved outcome
→ owner decision
```

## Changes and acceptance

| Failure | Implemented rule | Regression evidence |
| --- | --- | --- |
| The frontend rejected the server's redacted task plan | Authorized comparison responses include the frozen instructions needed to verify the diff. Public run projection remains redacted | `private_comparison_serialization_matches_frontend_through_owner_decision` sends actual Rust serialization to the frontend parser. `signed_human_comparison_http` repeats this through HTTP for preparation, execution, completion and decision |
| A delayed reservation remained pending after another task failed | The reserving executor closes a prepared call with a failed zero-usage receipt, including on a stopped or expired run. Saved inflight calls with no response retain unknown usage | Scheduler tests cover delayed acceptance and lost cancellation reports. `stopped_comparison_releases_unstarted_reservation_before_owner_decides` verifies release and persisted owner decision, including prior unknown usage |
| Independent journals called the same model task twice | Each new reservation requires a random execution ID saved before the request. The server atomically persists the claim and rejects another executor before granting the reservation | Concurrent runner regression and real HTTP race: four reservation requests yield two accepted claims, two conflicts and two provider calls. Both variants complete |

The runner also preserves a terminal status returned by a reservation retry. A stale initial running snapshot cannot authorize another model call after the server has stopped the run.

## Protocol and recovery

`reserve` requires `execution_id`, 64 lowercase hexadecimal characters. The same ID is required on reports for that call. Exact reservation retries are permitted after the deadline to recover already accepted work; the runner still prevents new provider calls after expiry. IDs are stored in private journals and server snapshots, and removed from every run projection. Responses expose only `execution_claimed`.

Old snapshots load with an absent execution ID. Saved legacy receipts can still be reported, but an unresolved legacy reservation cannot be reclaimed for fresh execution. An unresolved legacy prepared journal becomes unknown because the old protocol did not establish exclusive ownership. The arena and runner must be upgraded together.

The guarantee covers independently created journals. Simultaneous copies of one journal carry the same IDs and are indistinguishable from retries; only one copy may be active. No claim of universal exactly-once provider execution is made. Zero usage for a prepared cancellation is an owner-submitted record, just like other usage receipts. It is not independently attested by a provider.

## Validation

Fresh local outcomes and source hashes are recorded in [the acceptance record](HUMAN_COMPARISON_20261026_REPAIR.json). The original audit was targeted; this repair is not an audit of the entire platform.

Local socket tests initially failed at the sandbox bind restriction, before exercising the HTTP path. The same tests were rerun with permission to bind isolated localhost servers. Existing compiled dependencies were reused because local disk space was limited. No unrelated cache or user data was removed.

Remaining mandatory project checks are run in CI for the pushed commit. Previous CI results are not evidence for this change. SBF execution, frontend production build and any remaining Rust checks must be read from that commit's check results.

No live model service was called. Local provider responses are deterministic fixtures. This work does not establish advertising effectiveness, verify USD costs or implement a general hypothesis simulator. No deployment or blockchain transaction was performed.

## Sources

- [Private comparison API and backend regressions](../../arena/src/strategy_comparison.rs)
- [Reservation ownership, reports and snapshot validation](../../arena/src/strategy_cycle.rs)
- [Runner and recovery](../../tools/agent-bootstrap/strategy-run.mjs)
- [Actual HTTP comparison acceptance](../../tools/agent-bootstrap/comparison-http-check.mjs)
- [Frontend contract checker](../../frontend/scripts/comparison-contract-check.ts)
- [Owner guide and recovery procedure](../../tools/agent-bootstrap/COMPARISONS.md)

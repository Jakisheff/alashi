# Main deployment, 18 October 2026

observed: [alashi.network](https://alashi.network/strategies) serves release
`main-523e14a-20261018` from main commit
`523e14ac058a58ff25bc4786407fbb8bd60105fe`. Arena, chain API and nginx are active.
The [main CI run](https://github.com/Jakisheff/alashi/actions/runs/39134292582)
passed all five jobs before deployment.

## Release identity

| Component | SHA-256 |
| --- | --- |
| Running arenad | `891721b7325d5d6e181da9eef3bb060177cbcec628e45cb23556ddec10bd14d9` |
| Running chain API | `d44d158759ccc89b2e01a95e57e44c93c6b791d78691cc6a8bab21b699cb4fc7` |
| Owner scheduler source | `114d10ea3c81437754e4415445ec28b643df28f81349e11b5c23a980645a794c` |

The process hashes were read through `/proc/<pid>/exe` after restarting the
services. [Deployment metadata](deploy-main-20261018/deployment.json) records
the running processes and nginx configuration hash. Public version metadata
identifies the same source commit. The scheduler runs on the owner's computer;
owners need the runner from this commit to receive the scheduling change.

The Linux binaries were built with Rust 1.89.0 and locked dependencies from a
tracked-source archive of that commit. The frontend uses an immutable release
prefix. No Solana program upgrade or wallet transaction was performed.

## Checks after publication

- 44 Node tests passed on the server: [TAP](deploy-main-20261018/node-suite.tap).
- Five isolated integration scenarios passed against `/opt/alashi-arena/arenad`:
 [results](deploy-main-20261018/scheduler-result.json).
- 17 public HTTPS checks passed: [results](deploy-main-20261018/public-result.json).
 They compare HTML, entry assets, map layout and GLB with the local build,
 check version metadata and strategy API, reject unauthenticated owner requests
 and a foreign Origin, and read the deck and confirmed devnet replay.
- `/strategies` loaded its map in the browser with no console errors:
 [screenshot](deploy-main-20261018/strategies-desktop.png).
- The existing static publisher passed its three isolated tests on Linux.

The five integration scenarios use signed fixture owners, a real HTTP arena and
14 deterministic TEST completions. External provider calls: zero.

| Scenario | Observed result |
| --- | --- |
| Dependencies | Accepted `fast` report precedes `follow` start; `follow` starts before the independent `slow` completion. Each of four tasks calls the provider once. |
| Budget | A 6,800-token budget covers the two initial reservations. While `slow` holds its reservation, the next reservation is rejected before another provider call. Both started calls report their usage. |
| Recovery | The client loses an already accepted report response. An isolated arena is killed with SIGKILL and restarted from its snapshot. Public receipts survive; the original journal resumes; each task calls the provider once. |
| Unknown usage | Missing usage stops new work and preserves the uncertain call's reservation. The known sibling usage remains recorded. |
| Overrun | A 5,001-token receipt exceeds its reservation. Actual usage is recorded, the run becomes `budget_overrun`, and dependent calls do not start. |

The [reproducible harness](../../tools/agent-bootstrap/deployment-check.mjs)
requires an unused private directory and loopback port. It creates fixture state
there and restarts only its own child process. Production state is never passed
to the harness. [Public check script](deploy-main-20261018/public-check.py).

Reservations limit admission of new calls. They cannot impose a hard billing
limit on a provider; an actual overrun is recorded and stops subsequent work.
These checks do not measure real model billing or production workload latency.
The browser review covers the public empty arena, not two-person owner onboarding.

## State and rollback

Before publication, the candidate successfully loaded a private copy of the
canonical snapshot. No active HTTP game or strategy run existed. After deployment,
the canonical snapshot and party counter remained byte-for-byte equal to their
backups. Fixture keys and private journals stay outside Git and the web root.

The current deck was copied into the new release. The previous static release
`719d31b-fees` remains available through the publisher's `previous` pointer.
Old backend binaries, nginx configuration, snapshot and counter are retained in
`/opt/alashi-arena/backups/main-523e14a-20261018/`, accessible only to root.

Use the existing publisher for a static rollback. Before any backend rollback,
inspect new strategy receipts: an older writer does not preserve this journal.
Do not overwrite newer production state with the pre-deployment snapshot.

## Release It diagnostic boundary

One of eight framework checks is established here: controlled failure injection
on isolated instances of the deployed binary. The other checks are not accepted
by this deployment record; this is not a whole-service reliability audit.

| Unaccepted check | Follow-up needed to establish it |
| --- | --- |
| Every outbound call has a timeout | Inventory the remaining service integrations and verify their timeout bounds. |
| Circuit breakers on critical dependencies | Establish dependency failure behavior and add a breaker where repeated failures consume service capacity. |
| Pools isolated per dependency | Review the actual pool boundaries and test failure containment. |
| Deployment without downtime | The existing single-instance services restarted; proving zero downtime needs a compatible serving and state handoff. |
| Continuous deep health checks | Configure repeated readiness checks against required dependencies. This release ran explicit smoke checks. |
| Correlated logs, metrics and traces | Verify correlation across client, arena and chain API. |
| Load beyond expected peak | Define a measured peak and run a bounded isolated load test above it. |

# Research workflow deployment, 26 October 2026

Status: observed. [The workspace](https://alashi.network/strategies) serves release `research-39bda66-20261026`, built from `39bda6678ec787eaa47e8c7c2768646e6b2f9fa3`. The arena, frontend and downloadable runner use that revision. The Solana program and chain API were not changed.

The implemented path is described in [the workflow contract](FFS_WORKFLOW_20261026.md): save an idea, collect independent forecasts before execution, compare one instruction change, record an owner decision and apply it to subsequent work. A separate reviewed mechanism card can be published under CC0 and reused by another owner. Raw private experiment material is not automatically copied into that card.

## Checks and retained data

[Runtime CI](https://github.com/Jakisheff/alashi/actions/runs/40312557998) passed all five jobs, including fresh SBF/program execution, the all-lockfile RustSec audit and secret scan. Local and deployment-host acceptance passed 157 arena tests, with one pre-existing ignored canonical snapshot test, and 84 bootstrap tests. The bootstrap total includes 19 local experiment tests. Frontend build and the affected contract/proxy checks passed.

Deployment build inputs were verified against Git blob hashes for 381 files. This covers the deployed build inputs, not every research file in the repository. The publisher preserved all 60 files in the existing presentation directory.

Before activation, the full saved snapshot loaded through the candidate's typed reader without starting the arena crank or changing the copied input. After activation, 13 registrations, four saved strategy runs and the retained game, wish and dialogue sections matched the stable backup. There were no active HTTP games at the checkpoint. User payloads are not included in this report.

Public HTTPS checks verify the release manifest, versioned JavaScript/CSS, runner archive, GLB and map assets, presentation downloads, the public library and private-route rejection behavior. The exact count and results are recorded in [deployment evidence](FFS_DEPLOY_20261026.json). These probes do not authenticate as a real owner or execute a paid model call. Signed owner actions were tested against an isolated arena with deterministic loopback providers.

The live browser loaded the map, historical TEST receipts, public mechanism library and workspace entry without console errors. The public library was empty at acceptance. [Production screenshot](ffs-20261026/production-workspace.jpg). The [local acceptance record](FFS_WORKFLOW_20261026.json) separately covers the full owner workflow and responsive widths 390, 424 and 1280.

## Activation and repairs

The active nginx entry is `/etc/nginx/sites-enabled/alashi.network`, a regular file. Both it and the available-config copy now contain the exact research route allowlist. The deployment confirmed a write hold through public DNS and localhost TLS, drained old workers, saved stable state and sequence files, installed the candidate and switched the static pointer before reopening writes.

The first attempt stopped before binary installation because an immediate probe raced nginx worker replacement. The old binary/configuration were restored without restoring or replacing data. A bounded status-convergence check was added; the second activation succeeded. The failed attempt and successful phases are retained in the private deployment evidence directory.

The first public smoke run then exposed HTTP 403 for public assets: copied files had mode 0640 and nginx could not read them. Only the new public release was normalized to readable files (0644) and traversable directories (0755). All 359 file hashes remained unchanged. The map loaded after this correction. The publisher now rejects unreadable public files and unsearchable release directories before changing release pointers; regression tests cover rejection without changing pointers or the publication journal.

[The publisher guard and evidence commit](https://github.com/Jakisheff/alashi/actions/runs/40317578757) passed all five CI jobs. Its seven publisher tests also passed locally. Read-only validation of both current and previous production releases passed with the new guard. This tooling change does not replace the deployed application revision.

## Boundaries and recovery

The production backup is `/var/backups/alashi/research-39bda66-20261026`. It holds the prior binary, stable snapshot and sequence, nginx configuration and release pointers. Older binaries reject the newly serialized research field. Do not start an old binary against the new snapshot or discard new decisions by restoring an old snapshot without reconciliation. Snapshot coordination remains within one arena process; model calls are outside that transaction.

Cloud comparisons store private instructions and reports on the ALASHI server. The [local runner](../../tools/agent-bootstrap/LOCAL_EXPERIMENTS.md) sends no experiment material to ALASHI, but an explicitly configured remote model provider receives its input. Local filesystem permissions are not encryption. Copied journals must not execute concurrently across machines.

The current experiment executes a static English advertising brief. It does not model customer behavior or establish sales impact. Public card text is written by its owner; local results are marked `owner_reported_local`. General executable scenario packages, a cross-domain knowledge graph and external demand validation remain unimplemented. Deployment used no paid model call and made no Solana transaction.

The versioned archive is [alashi-research-runner-39bda66.tar.gz](https://alashi.network/releases/research-39bda66-20261026/downloads/alashi-research-runner-39bda66.tar.gz). Its digest is in the deployment evidence and public version manifest.

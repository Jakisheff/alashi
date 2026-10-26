# Private comparison deployment, 26 October 2026

Observed: [the owner workspace](https://alashi.network/strategies) serves release `comparison-678861a-20261026`. Source is `678861a7cd2f1a64b93871480d3cf8aa31152812`; executable changes are from `e9b181c390269a73c54694587ca22b72a9e1702b`. The intervening commit contains acceptance documentation only. The release uses the reviewed PR branch; no merge into main was performed.

The arena and frontend are updated. The compatible local runner is available as a [versioned archive](https://alashi.network/releases/comparison-678861a-20261026/downloads/alashi-comparison-runner-678861a.tar.gz). Extract it into a new directory, run `npm ci --ignore-scripts --prefix tools/agent-bootstrap`, then follow the current [comparison guide](../../tools/agent-bootstrap/COMPARISONS.md). The archive preserves the source revision, including its earlier documentation; this deployment record supersedes its historical statement that the feature was not deployed. Provider credentials and journals remain on the owner's computer.

The separate chain API binary and Solana program were not changed. All 60 existing deck files were retained byte for byte.

## Evidence

[Machine-readable evidence](HUMAN_COMPARISON_DEPLOY_20261026.json) records hashes, public requests and state continuity. The source passed [all five CI jobs](https://github.com/Jakisheff/alashi/actions/runs/40297668440). Fresh checks on the deployment host passed:

- 148 arena tests, with one intentionally ignored canonical snapshot test; 64 runner tests.
- Frontend build and comparison, strategy, trading and real Vite proxy checks.
- Full typed loading of a private copy of the canonical snapshot. An occupied loopback socket prevented the listener and crank from starting; file hashes stayed unchanged.
- 48 public HTTPS checks: release metadata, pages, JS/CSS, a GLB, PDF/PPTX, runner download and API access boundaries.

Anonymous owner requests returned 401. Wrong methods, query parameters and content types were rejected. Private responses carried `no-store` and `no-referrer`. Public checks used no owner credential, so they do not independently establish authenticated production CSRF behavior; signed isolated HTTP tests cover that contract.

The browser loaded the public test receipts without console errors. No new fixture was inserted into production. Registrations (13), proposal key, completed games and their replays, dialogue and wish sections matched the pre-upgrade snapshot. No active HTTP game existed during switching. No paid model call, live owner comparison or Solana transaction was executed for deployment.

## Operating details

The active nginx file is `/etc/nginx/sites-enabled/alashi.network`, a regular file. It is not a symlink to `sites-available`. The initial update reached the inactive file; therefore its intended POST hold was ineffective. Public verification exposed the discrepancy. The original active file was then backed up, its strategy block replaced with the exact comparison allowlist, and nginx tested and reloaded. Both configuration files now contain the reviewed configuration. Final public checks passed after this correction.

The system Node binary lacks TypeScript stripping. An isolated official Node 22.18.0 installation, with archive SHA verified against the vendor manifest, ran the TypeScript checks. The frontend build itself used Node 22.22.1. Full npm audit reported a pre-existing high advisory in build-only Zod 4.6.5; `npm audit --omit=dev` reported zero vulnerabilities. Build dependency maintenance remains separate from this deployment.

## Recovery boundary

Private backups are in `/var/backups/alashi/comparison-678861a-20261026`. They contain the old arena binary, stable `state.json` and `party-no.txt`, both original nginx configurations, and static pointers. The original active nginx backup is `nginx.enabled.conf`; `nginx.conf` is the former inactive configuration.

The previous static release is `main-523e14a-20261018`. The existing static release helper controls the pointers and journal. Inspect both pointers before any rollback; its two replacements are not a transaction.

The old arena cannot read a newly serialized strategy snapshot, even with empty comparisons. Do not start the old binary against new state. Before rollback, hold mutations in the active nginx configuration, stop the arena and retain the latest state separately. Any post-deployment records must be reconciled before restoring the pre-upgrade snapshot/sequence pair. Restore their `alashi-arena` ownership and permissions, then restore the binary and static/configuration versions together and repeat health checks. No rollback was needed here.

Independent journals cannot claim the same execution. Concurrent copies of one journal remain unsupported. See [repair acceptance](HUMAN_COMPARISON_20261026_REPAIR.md) for the protocol boundary.

## Secret scan of deployment evidence

The documentation CI run `40303742720` passed its build, RustSec and runtime test jobs but reported two `generic-api-key` findings in the recorded build metadata. The values are the public chain API source commit and binary SHA256, verified against Git and the public version manifest. The root `.gitleaksignore` identifies only those two findings by full historical commit, path, rule and line. The default detection rule and all file scanning remain enabled. Changing only the current document would not remove findings from the scanned Git history.

The pinned Gitleaks 8.24.3 scanner was also checked with a new temporary commit containing an inert synthetic key at the same report path and line 337. It still reported the new occurrence with the historical exclusions present. The control did not use a live credential.

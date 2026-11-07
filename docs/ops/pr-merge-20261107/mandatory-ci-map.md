# Mandatory check mapping

observed: the workflow was fetched from GitHub main on 07.11.2026 and compared with the local repository. `.github/workflows/ci.yml` blob is `9eb4314338dfdc4c2557e5a826d030fe6d211484`. The locally recorded main commit was `0db54f864d63d593937200f3d3c1d2a1fcd00390`. The execution archive below now proves the mandatory jobs for PR 69 source head 74ca4c53; the overall run still failed its independent Gitleaks job.

| Requested check | CI job and step | Configured command and location | Status of mapping |
| --- | --- | --- | --- |
| Rules tests with lockfile | `tests`: Workspace tests (rules crate; the Anchor program needs `anchor build` for its .so tests, excluded here) | `cargo test --locked --workspace --exclude alashi`, ci.yml:54 | Includes `alashi-rules` and vendor recovery tests. The exact `-p alashi-rules` invocation is not in CI. Root requested a separate exact local run, recorded below. |
| Arena tests | `tests`: Arena tests (unit + http e2e) | `cargo test --locked --manifest-path arena/Cargo.toml`, ci.yml:62 | Exact command. |
| Indexer tests | `tests`: Indexer tests | `cargo test --locked --manifest-path indexer/Cargo.toml`, ci.yml:74 | Exact command. |
| Program host check | `tests`: Anchor program host compile (SBF execution is the sbf job below) | `cargo check --locked -p alashi`, ci.yml:77 | Requested operation with additive `--locked`. |
| Bots build | `tests`: Bots compile check | `cargo build --locked --manifest-path bots/Cargo.toml`, ci.yml:65 | Exact command. |
| SBF build before program tests | `sbf`: Build fresh SBF (tools/build_sbf.sh pins 4.1.0 / platform-tools v1.54) | `bash tools/build_sbf.sh`, ci.yml:106 | Exact command, immediately before the program-test step in the same job. |
| Program execution tests | `sbf`: Program tests on fresh SBF (game + replay equivalence) | `cargo test --locked -p alashi`, ci.yml:109 | Requested operation with additive `--locked`. |

The `tests` job is named “Rules, arena, indexer, and program compile”. The `sbf` job is named “Fresh SBF build and program tests”. Neither the SBF build nor program-test step has `continue-on-error`, `if: always()` or another override that would run tests after a failed SBF build. They execute sequentially in one runner checkout.

`tools/build_sbf.sh` blob `c9bb897cb11ec2a0af779628f1fe908709e45fb9` uses `set -euo pipefail`, requires cargo-build-sbf 4.1.0, pins platform-tools v1.54 and runs cargo-build-sbf with `-- --locked`. It builds `programs/alashi/Cargo.toml` into `target/deploy`; its compilation cache is isolated in `target/sbf-v1.54`.

The root workspace explicitly includes `rules`, `programs/*` and `vendor/solana-secp256k1-recover`. The workspace test command therefore covers the rules crate while excluding the Anchor package. A successful workspace run is evidence for those rules tests, but must not be described as execution of the literal `cargo test --locked -p alashi-rules` command.

## Exact local rules invocation

Root authorized the literal command in `/Users/amir/Documents/Codex/worktrees/alashi-pr-merge-20261107`, source head `321bf71d86ddf7fa683b5bd7fac55f4cfa5e4235`.

Command: `cargo test --locked -p alashi-rules`.

Log: `docs/ops/pr-merge-20261107/final-rules.log` in that worktree. observed: completed with exit 0 on 07.11.2026 at 16:59 Asia/Almaty. All 40 tests passed across five test binaries; doc tests had 0 tests. Elapsed time 482.36 seconds. No code or workflow edits are required for this check.

## Five Node test files

The `animations` job, “Animation geometry and frontend build”, installs agent-bootstrap dependencies with `npm ci --ignore-scripts --prefix ../tools/agent-bootstrap` at ci.yml:24. Its ci.yml:33 command explicitly runs strategy-run, strategy-scheduler, strategy-comparison, local-experiment and alashi test files with `node --test`. Save this job log too because it proves the previously blocked files loaded and executed.

## Final run log retrieval

Retrieval procedure, executed for PR 69 run 42370377477 and source head 74ca4c53:

1. Fetch workflow runs for that head and match the selected run ID, event, attempt, conclusion and head SHA. For pull_request runs also record the checkout merge SHA from the log; do not confuse it with the source head.
2. Fetch all jobs for that run with `mcp__codex_apps__github_fetch_workflow_run_jobs`. Match the `tests`, `sbf` and `animations` display names above. The connector returns only the latest attempt and first page, so record the attempt and verify the five expected jobs are present.
3. Fetch step summaries with `mcp__codex_apps__github_fetch_workflow_job_steps` for those job IDs. Require completed/success for each mapped step, not merely a green aggregate or a skipped step. Verify SBF build finishes before program tests start.
4. Fetch decoded full job logs with `mcp__codex_apps__github_fetch_workflow_job_logs`. Persist complete text without truncation near the audit, for example `docs/ops/pr-merge-20261107/ci-<run-id>-tests.log`, `docs/ops/pr-merge-20261107/ci-<run-id>-sbf.log` and `docs/ops/pr-merge-20261107/ci-<run-id>-animations.log`. The temporary redirect URL is not the durable evidence link; retain the run/job GitHub URLs.
5. Save a JSON manifest with run/head/attempt metadata, job and step conclusions, log byte counts and SHA256 hashes. Check each log contains the command and successful test/build result; retain existing compiler warnings. Include the local literal rules log in the manifest.
6. If connector log retrieval cannot return the full body, use read-only `gh run view <run-id> --repo Jakisheff/alashi --log` or per-job logs and compare saved file size with the response. Do not infer log completeness from a truncated tool preview.

No CI rerun, cache deletion or workflow change is needed to retrieve evidence. The completed archive is in root `docs/ops/pr-merge-20261107/ci-42370377477/`. Mandatory jobs succeeded, including 91 Node tests across the five files. Overall run conclusion is failure because of the eight historical Gitleaks fingerprints. Actual checkout merge SHA is 7407ff38ccd573df93ae77527ed2328f26105e89; source head is 74ca4c53aad2cf4d1efe431f6e297501c7bb83f4. See archive manifest for complete step results, hashes and SBF ordering. A later final-head green run remains the merge gate.

# CI run 42370377477 evidence

The three mandatory runtime jobs passed. The complete run failed because Gitleaks reported eight historical findings. This archive is not evidence of an overall green merge gate.

[Run metadata](run.json) records pull request 69, source head `74ca4c53aad2cf4d1efe431f6e297501c7bb83f4`, attempt 1 and conclusion failure. The jobs actually checked out GitHub merge commit `7407ff38ccd573df93ae77527ed2328f26105e89`, combining that head with base `8c1dffb4267083bcbab6b084769846ada0f70670`. Each full log contains the checkout merge SHA. The tested checkout is distinct from the source head.

| Job | Result | Full log |
| --- | --- | --- |
| Rules, arena, indexer, and program compile | Success; required steps completed successfully | [tests.log](tests.log) |
| Fresh SBF build and program tests | Success; fresh build precedes program execution and both epoch replay tests pass | [sbf.log](sbf.log) |
| Animation geometry and frontend build | Success; all five requested Node files execute, 91 tests pass with 0 failures or skips | [animations.log](animations.log) |

The SBF release build finished at `2026-11-07T11:59:15.9819939Z`. Program tests started afterward at `2026-11-07T11:59:16.0152480Z`. Their step numbers are 6 and 7 in the same job, both successful.

The exact local `cargo test --locked -p alashi-rules` command also passed 40 tests at runtime head `321bf71d86ddf7fa683b5bd7fac55f4cfa5e4235`; see [final-rules.log](../final-rules.log). CI uses the broader locked workspace command excluding the Anchor package, which includes the rules crate.

[manifest.json](manifest.json) contains command evidence, step conclusions, source/merge identities, UTF-8 byte counts and SHA256 hashes. [jobs.json](jobs.json) includes all five job conclusions; [step-status.json](step-status.json) records the three required jobs' separately fetched step summaries. [scan-failure-fingerprints.json](scan-failure-fingerprints.json) preserves the eight scan locations without matched secret text. The root and an independent reviewer recomputed every flagged checksum from its same-commit file. The narrow exception change and its red/green evidence are recorded separately in PR24; this archived run remains marked failed.

The logs came from the GitHub plugin and were saved in full without whitespace normalization. Their saved byte counts exactly match the decoded download sizes: tests 209163, animations 75778 and SBF 47764 bytes. Publication removes only trailing spaces and tabs from log lines. The manifest retains each original download byte count and SHA256, alongside the published byte count and SHA256. No test output line was omitted.

Run metadata was retrieved through a read-only GitHub CLI API request because the selected plugin wrapper omitted head/event/attempt fields. No job rerun or workflow mutation was performed by this reviewer.

---
name: sbf-verify
description: >
  Gate before `cargo test -p alashi` and any litesvm work: verifies the SBF
  .so artifact (sha256 manifest, ELF header, freshness) and refuses stale or
  foreign artifacts that surface as Access violations. Use for SBF builds and
  program tests.
---

# SBF verify

Mandatory gate before program tests. The tests embed `target/deploy/alashi.so`
via `include_bytes`; a stale or foreign `.so` produces litesvm Access
violations that look like code bugs (PROGRESS.md, 07.09).

## Steps

1. Run: `bash tools/sbf_gate.sh`
2. On exit 2 run `bash tools/build_sbf.sh` (rebuild + fresh manifest), then
   repeat the gate. Do not proceed on a failed gate.
3. With gate exit 0 only: `cargo test --locked -p alashi`

## Rules, no exceptions

- A green rules-only, arena, or `cargo check` run is not a green program.
  SBF execution is a separate check.
- Running program tests while the gate refuses is a false report by default.
- Do not "fix" the gate by deleting `target/sbf-v1.54` or editing the
  manifest; rebuild via `tools/build_sbf.sh`.
- If the gate refuses after a fresh rebuild, report the refusal lines
  verbatim; do not paraphrase them as a build success.

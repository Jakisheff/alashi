#!/usr/bin/env bash
# Refuse to let `cargo test -p alashi` run against a stale or foreign SBF
# artifact. Run this after tools/build_sbf.sh and before program tests.
# Exit 0 = artifact verified; exit 2 = refused (rebuild via tools/build_sbf.sh).
set -euo pipefail
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
exec python3 "$repo/tools/sbf_gate.py" --repo "$repo"

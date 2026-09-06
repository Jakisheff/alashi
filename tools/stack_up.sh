#!/usr/bin/env bash
# Start the local HTTP arena. Build from the repository root first.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ ! -x ./arena/target/release/arenad ]]; then
  echo "[ERROR] Build first: cargo build --release --locked --manifest-path arena/Cargo.toml" >&2
  exit 1
fi
exec ./arena/target/release/arenad "$@"

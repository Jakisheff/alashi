#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
build_version=$(cargo-build-sbf --version | sed -n '1p')
if [[ "$build_version" != 'cargo-build-sbf 4.1.0' ]]; then
  printf 'Expected cargo-build-sbf 4.1.0; found %s. Check your PATH.\n' "$build_version" >&2
  exit 1
fi

# Keep platform-tools artifacts separate from the default Cargo target directory.
export CARGO_TARGET_DIR="$repo/target/sbf-v1.54"
cargo-build-sbf --tools-version v1.54 --arch v0 \
  --manifest-path "$repo/programs/alashi/Cargo.toml" \
  --sbf-out-dir "$repo/target/deploy" -- --locked

# Record provenance for tools/sbf_gate.sh: sha256 + ELF header + build time.
# The gate refuses stale or foreign .so artifacts before program tests.
python3 "$repo/tools/sbf_gate.py" --repo "$repo" --write-manifest

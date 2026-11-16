#!/usr/bin/env bash
# Build the frontend rules WASM artifact from the canonical rules crate.
# Output (committed; freshness is enforced by frontend/scripts/rules-wasm-check.ts,
# which compares WASM outputs against the current rules/src/constants.rs):
#   frontend/src/rules-wasm/  bundler target; used by the browser (src/rules/browserInit.ts)
#                             and by node checks (scripts/nodeRulesInit.ts feeds init() bytes)
# Requires: rustup target wasm32-unknown-unknown, wasm-bindgen-cli 0.2.130.
set -euo pipefail
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo/rules-wasm"

BINDGEN_VERSION="0.2.130"
command -v wasm-bindgen >/dev/null || { echo "wasm-bindgen not found; cargo install wasm-bindgen-cli --locked --version $BINDGEN_VERSION" >&2; exit 1; }

cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --version | grep -q "$BINDGEN_VERSION" || { echo "wasm-bindgen $BINDGEN_VERSION required (crate/CLI versions must match)" >&2; exit 1; }

wasm-bindgen "target/wasm32-unknown-unknown/release/alashi_rules_wasm.wasm" \
  --target web --out-dir "$repo/frontend/src/rules-wasm" --typescript

echo "rules-wasm artifact regenerated; commit it together with any rules change."

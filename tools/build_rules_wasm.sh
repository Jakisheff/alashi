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

# 25.11: пути ремапятся, чтобы в артефакт не попадали локальные пути машины сборки
# (правило 20: /Users/... в публичном wasm — утечка; проверено strings после сборки).
export RUSTFLAGS="--remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME/.rustup=/rustup --remap-path-prefix=$repo=/repo"

cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --version | grep -q "$BINDGEN_VERSION" || { echo "wasm-bindgen $BINDGEN_VERSION required (crate/CLI versions must match)" >&2; exit 1; }

wasm-bindgen "target/wasm32-unknown-unknown/release/alashi_rules_wasm.wasm" \
  --target web --out-dir "$repo/frontend/src/rules-wasm" --typescript

if strings "$repo/frontend/src/rules-wasm/alashi_rules_wasm_bg.wasm" | grep -q "$HOME"; then
  echo "rules-wasm artifact contains local build-machine paths; refusing" >&2; exit 1
fi

echo "rules-wasm artifact regenerated; commit it together with any rules change."

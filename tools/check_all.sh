#!/bin/bash
# Локальный набор проверек перед PR. Одна команда вместо пяти.
# SBF-сборка и тесты программы alashi сюда НЕ входят: они требуют
# cargo-build-sbf 4.1.0 (tools/build_sbf.sh) и отдельного прогона;
# зелёный этот скрипт не заменяет их для правок программы.
set -euo pipefail
cd "$(dirname "$0")/.."

fail() { printf 'FAIL: %s\n' "$1" >&2; exit 1; }
step() { printf '\n== %s\n' "$1"; }

step "rules (юнит-тесты правил)"
cargo test --locked -p alashi-rules || fail "alashi-rules"

step "arena (юнит + http e2e; npm-зависимости ставятся тестами сами)"
cargo test --locked --manifest-path arena/Cargo.toml || fail "arena"

step "indexer"
cargo test --locked --manifest-path indexer/Cargo.toml || fail "indexer"

step "bots compile"
cargo build --locked --manifest-path bots/Cargo.toml || fail "bots"

step "cargo check программы alashi (компиляция без SBF-тестов)"
cargo check --locked -p alashi || fail "cargo check alashi"

if [ -d frontend/node_modules ]; then
    step "frontend typecheck"
    (cd frontend && npm run typecheck) || fail "typecheck"
    step "frontend build"
    (cd frontend && npm run build) || fail "build"
else
    printf 'frontend: node_modules отсутствует, пропускаю (npm ci --prefix frontend)\n'
fi

printf '\nALL LOCAL CHECKS GREEN\n'
printf 'Не покрыто здесь: SBF-сборка и тесты программы (bash tools/build_sbf.sh, затем cargo test --locked -p alashi), gitleaks, RustSec-аудит (CI).\n'

#!/usr/bin/env bash
# contract-gate.sh: an interface change requires BOTH sides' suites.
#
# Lesson of 22.11 (f2f69e6d post-mortem): a green suite on the side you
# edited proves nothing about the contract the other side validates. This
# gate maps changed files to every suite that knows their interfaces and
# refuses to pass until all of them ran green in one go. (Local pre-push
# tool by design: in CI the pushed tree is clean and the script reports
# "no changes"; it is not a CI gate.)
#
# Usage: bash tools/contract-gate.sh [base]   (default base: origin/main)

set -uo pipefail
cd "$(dirname "$0")/.."
BASE="${1:-origin/main}"

changed() {
  { git diff --name-only "$BASE"...HEAD 2>/dev/null
    git diff --name-only 2>/dev/null
    git status --porcelain | awk '{print $2}'
  } | sort -u
}

FILES_TMP="$(mktemp)"
changed > "$FILES_TMP"
if [ ! -s "$FILES_TMP" ]; then
  echo "contract-gate: no changes against $BASE"
  rm -f "$FILES_TMP"
  exit 0
fi

need_arena=0; need_bootstrap=0; need_egress=0; need_rules=0
count=0
while IFS= read -r f; do
  count=$((count+1))
  case "$f" in
    arena/src/*|arena/tests/*) need_arena=1 ;;
    rules/src/*|rules/tests/*) need_rules=1; need_arena=1 ;;
    # The driver<->arena HTTP contract: both languages must run.
    tools/agent-bootstrap/strategy-run.mjs|tools/agent-bootstrap/strategy-comparison.mjs|tools/agent-bootstrap/local-experiment.mjs|tools/agent-bootstrap/report-contract.json)
      need_bootstrap=1; need_arena=1 ;;
    tools/agent-bootstrap/*) need_bootstrap=1 ;;
    tools/egress-proxy/*) need_egress=1 ;;
  esac
done < "$FILES_TMP"
rm -f "$FILES_TMP"

echo "contract-gate: $count changed file(s) against $BASE"
[ "$need_arena" = 1 ]    && echo "  -> arena suite (Rust side of the report contract, phase timers)"
[ "$need_bootstrap" = 1 ] && echo "  -> agent-bootstrap suite (Node drivers, contract fixture)"
[ "$need_egress" = 1 ]   && echo "  -> egress-proxy suite (receipts, budget)"
[ "$need_rules" = 1 ]    && echo "  -> rules suite"

fail=0
if [ "$need_rules" = 1 ]; then
  cargo test --locked -p alashi-rules || fail=1
fi
if [ "$need_arena" = 1 ]; then
  cargo test --locked --manifest-path arena/Cargo.toml || fail=1
fi
if [ "$need_bootstrap" = 1 ]; then
  (cd tools/agent-bootstrap && node --test) || fail=1
fi
if [ "$need_egress" = 1 ]; then
  (cd tools/egress-proxy && node --test) || fail=1
fi

if [ "$fail" = 1 ]; then
  echo "contract-gate: FAILED — a required suite is red"
  exit 1
fi
echo "contract-gate: all required suites green"

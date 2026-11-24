#!/bin/sh
# tools/hooks/pre-checkout-guard: ловушка для операции (a) «откат/сброс поверх
# незакоммиченного» (класс act-wider-than-intent, инциденты I4/I5).
# git не имеет pre-checkout hook; это обёртка для явного использования:
#   bash tools/hooks/pre-checkout-guard.sh checkout -- FILE...
#   bash tools/hooks/pre-checkout-guard.sh reset --hard [REF]
# Блокирует, если в затрагиваемых файлах есть незакоммиченные изменения,
# не входящие в целевой diff (то есть операция уничтожит чужую работу).
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"

CMD="$1"; shift

case "$CMD" in
  checkout)
    TARGET="$1"; shift || true
    # только опасная форма: checkout -- FILE (откат файла)
    if [ "$TARGET" != "--" ] && [ "$TARGET" != "HEAD" ]; then
      exec git checkout "$TARGET" "$@"
    fi
    FILES="$*"
    DIRTY=$(git status --porcelain -- $FILES 2>/dev/null | grep -E '^( M|M |MM|AM| D|D )' || true)
    ;;
  reset)
    # опасная форма: reset --hard (сносит всё дерево)
    if [ "$1 $2" != "-- hard" ] && [ "$2" != "--hard" ]; then
      exec git reset "$@"
    fi
    FILES=""
    DIRTY=$(git status --porcelain | grep -E '^( M|M |MM|AM| D|D |\?\?)' || true)
    ;;
  *)
    echo "usage: pre-checkout-guard.sh checkout -- FILE... | reset --hard [REF]" >&2
    exit 2
    ;;
esac

if [ -n "$DIRTY" ]; then
  echo "checkout-guard BLOCKED: незакоммиченные изменения будут уничтожены:" >&2
  echo "$DIRTY" | head -10 >&2
  echo "Закоммить или stash ПЕРЕД откатом (класс act-wider-than-intent, I4/I5)." >&2
  echo "Для принуждения: FORCE=1 bash $0 $CMD $TARGET $FILES" >&2
  exit 1
fi
exec git "$CMD" "$TARGET" "$@"

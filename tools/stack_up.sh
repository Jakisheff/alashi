#!/bin/bash
# Поднять весь стек alashi одной командой (см. HANDOFF.md).
# Проверяет/поднимает: arenad :8090, cloudflared-туннель, демон agent_inbox.
# Запуск: bash tools/stack_up.sh   (из корня репо или откуда угодно)
set -u
cd "$(dirname "$0")/.."
echo "== alashi stack up =="

# 1. arenad
if pgrep -f "arenad --port 8090" >/dev/null; then
  echo "[ok] arenad :8090 уже жив (pid $(pgrep -f 'arenad --port 8099' | head -1))"
else
  nohup ./arena/target/release/arenad --port 8090 >> /tmp/opencode/arenad_8090.log 2>&1 &
  sleep 1
  curl -s -m 3 http://127.0.0.1:8090/games >/dev/null && echo "[ok] arenad :8090 поднят" || echo "[ERROR] arenad не поднялся"
fi

# 2. cloudflared quick-tunnel (URL печатаем — он меняется при каждом перезапуске)
# бинарник ищем в персистентных путях: /tmp вычищается при перезагрузке
CF_BIN="$(command -v cloudflared || true)"
[ -z "$CF_BIN" ] && [ -x "$HOME/.local/bin/cloudflared" ] && CF_BIN="$HOME/.local/bin/cloudflared"
[ -z "$CF_BIN" ] && [ -x /tmp/cloudflared ] && CF_BIN=/tmp/cloudflared
if pgrep -f "cloudflared tunnel" >/dev/null; then
  echo "[ok] cloudflared жив, свежий URL:"
  grep -hoE "https://[a-z0-9-]+\.trycloudflare\.com" /tmp/opencode/cf_tunnel*.log 2>/dev/null | tail -1
elif [ -n "$CF_BIN" ]; then
  nohup "$CF_BIN" tunnel --url http://127.0.0.1:8090 > /tmp/opencode/cf_tunnel_$(date +%H%M%S).log 2>&1 &
  sleep 10
  LATEST=$(ls -t /tmp/opencode/cf_tunnel_*.log | head -1)
  URL=$(grep -oE "https://[a-z0-9-]+\.trycloudflare\.com" "$LATEST" | head -1)
  if [ -n "$URL" ]; then echo "[ok] туннель поднят: $URL"; else echo "[ERROR] туннель не поднялся (см. $LATEST)"; fi
fi

# 3. демон автосбора inbox
if pgrep -f "agent_inbox.py" >/dev/null; then
  echo "[ok] agent_inbox демон уже жив"
else
  nohup python3 tools/agent_inbox.py >> /tmp/agent_inbox.log 2>&1 &
  sleep 2
  pgrep -f "agent_inbox.py" >/dev/null && echo "[ok] agent_inbox демон поднят" || echo "[ERROR] демон не поднялся"
fi

echo "== стек готов. Новый туннель сообщи агентам (URL одноразовый) =="

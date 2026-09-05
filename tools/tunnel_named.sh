#!/bin/bash
# Именованный Cloudflare Tunnel для arenad — постоянный URL для демо.
#
# ОДНО ЧЕЛОВЕЧЕСКОЕ ДЕЙСТВИЕ (один раз, ~1 минута):
#   cloudflared tunnel login
#   (откроет браузер, войди в аккаунт Cloudflare и выбери свой домен;
#    без домена на Cloudflare именованный туннель невозможен — тогда
#    альтернатива: Fly.io/Render, см. docs/ops/DEMO_RUNBOOK.md)
#
# Дальше этот скрипт сам: создаёт туннель "alashi-arena", пишет конфиг,
# маршрутизирует DNS alashi.<домен> и поднимает туннель на arenad :8090.
# URL постоянный: переживает перезапуски ноутбука, туннеля и арены.
set -u
DOMAIN="${1:-}"
TUNNEL="alashi-arena"
CFG="$HOME/.cloudflared/config-alashi.yml"
CF="$HOME/.local/bin/cloudflared"
command -v cloudflared >/dev/null && CF="$(command -v cloudflared)"

[ -x "$CF" ] || { echo "[ERROR] cloudflared не найден ни в PATH, ни в ~/.local/bin"; exit 1; }
[ -f "$HOME/.cloudflared/cert.pem" ] || { echo "[ERROR] сначала: cloudflared tunnel login (см. шапку скрипта)"; exit 1; }
[ -z "$DOMAIN" ] && { echo "usage: tools/tunnel_named.sh <ваш-домен.ру>"; exit 1; }

if ! "$CF" tunnel list 2>/dev/null | grep -q "$TUNNEL"; then
  "$CF" tunnel create "$TUNNEL" || { echo "[ERROR] не создать туннель"; exit 1; }
fi
ID=$("$CF" tunnel list 2>/dev/null | awk -v t="$TUNNEL" '$1==t{print $2}')
[ -z "$ID" ] && { echo "[ERROR] id туннеля не найден"; exit 1; }
cat > "$CFG" << EOF
tunnel: $ID
credentials-file: $HOME/.cloudflared/$ID.json
ingress:
  - hostname: alashi.$DOMAIN
    service: http://127.0.0.1:8090
  - service: http_status:404
EOF
"$CF" tunnel route dns "$TUNNEL" "alashi.$DOMAIN" 2>/dev/null | tail -1
pkill -f "cloudflared tunnel" 2>/dev/null; sleep 1
nohup "$CF" tunnel --config "$CFG" run >> /tmp/opencode/cf_named.log 2>&1 &
sleep 5
echo "== туннель поднят, постоянный URL: https://alashi.$DOMAIN/ui =="
echo "   экран зрителя: https://alashi.$DOMAIN/ui"
curl -s -m 10 -o /dev/null -w "проверка: HTTP %{http_code}\n" "https://alashi.$DOMAIN/ui" || echo "[ERROR] проверь /tmp/opencode/cf_named.log"

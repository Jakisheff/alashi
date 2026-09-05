#!/bin/bash
# Сборка деки: docs/deck.html -> PNG (Chrome headless) -> docs/out/deck.pptx + deck.pdf
# Использование: tools/deck_build.sh [число_слайдов]
# DECK_PYTHON - python с python-pptx и pillow (иначе ищется системный).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HTML="$ROOT/docs/deck.html"
OUT="$ROOT/docs/out"
N="${1:-8}"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
PY="${DECK_PYTHON:-python3}"

mkdir -p "$OUT/png"

echo "[1/3] PNG: $N слайдов, 1920x1080 @2x"
for i in $(seq 1 "$N"); do
  n=$(printf "%02d" "$i")
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars \
    --window-size=1920,1080 --force-device-scale-factor=2 \
    --virtual-time-budget=4000 \
    --screenshot="$OUT/png/slide_$n.png" \
    "file://$HTML?slide=$i" 2>/dev/null
  echo "  slide_$n.png $(stat -f%z "$OUT/png/slide_$n.png") bytes"
done

echo "[2/3] PPTX 16:9"
"$PY" - "$OUT" "$N" <<'EOF'
import sys
from pptx import Presentation
from pptx.util import Inches

out, n = sys.argv[1], int(sys.argv[2])
prs = Presentation()
prs.slide_width = Inches(13.333)
prs.slide_height = Inches(7.5)
blank = prs.slide_layouts[6]
for i in range(1, n + 1):
    s = prs.slides.add_slide(blank)
    s.shapes.add_picture(f"{out}/png/slide_{i:02d}.png", 0, 0,
                         width=prs.slide_width, height=prs.slide_height)
prs.save(f"{out}/deck.pptx")
print(f"  deck.pptx: {n} слайдов")
EOF

echo "[3/3] PDF (Pillow, 288 dpi)"
"$PY" - "$OUT" "$N" <<'EOF'
import sys
from PIL import Image

out, n = sys.argv[1], int(sys.argv[2])
paths = [f"{out}/png/slide_{i:02d}.png" for i in range(1, n + 1)]
imgs = [Image.open(p).convert("RGB") for p in paths]
imgs[0].save(f"{out}/deck.pdf", save_all=True, append_images=imgs[1:],
             resolution=288.0, quality=92)
print(f"  deck.pdf: {n} страниц")
EOF

echo "Готово: $OUT/deck.pptx, $OUT/deck.pdf"

"""Rasterize the canonical SVGs. Development tools: CairoSVG and Pillow, not browser dependencies."""
from pathlib import Path
from io import BytesIO
import cairosvg
from PIL import Image

public = Path(__file__).resolve().parents[1] / 'public'
brand = public / 'brand'
for stem in ['alashi-forall-symbol', 'alashi-forall-symbol-dark', 'alashi-forall-symbol-light', 'alashi-coin-flat']:
    data = (brand / f'{stem}.svg').read_text()
    if 'symbol' in stem:
        data = data.replace('viewBox="0 0 364 332"', 'viewBox="-18 -34 400 400"')
    for size in [32, 64, 128, 256, 512, 1024]:
        cairosvg.svg2png(bytestring=data.encode(), write_to=str(brand / f'{stem}-{size}.png'), output_width=size, output_height=size)
icon = (public / 'favicon.svg').read_bytes()
for size in [16, 32, 48, 192, 512]:
    cairosvg.svg2png(bytestring=icon, write_to=str(public / 'icons' / f'icon-{size}.png'), output_width=size, output_height=size)
touch = cairosvg.svg2png(bytestring=icon, output_width=180, output_height=180)
for target in [public / 'icons/apple-touch-icon.png', public / 'apple-touch-icon.png']:
    target.write_bytes(touch)
Image.open(BytesIO(cairosvg.svg2png(bytestring=icon, output_width=64, output_height=64))).save(public / 'favicon.ico', sizes=[(16, 16), (32, 32), (48, 48)])

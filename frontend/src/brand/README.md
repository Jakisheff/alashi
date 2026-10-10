# ALASHI NETWORK mark and decorative coins

Reference: Amir's `ALASHI-FORALL-BRAND-20261010-01`, exact team docs
`014711da93b3d3b5e9322304abd39eb2c7f1b25e`, concept SHA256
`9e1236c09306433fadb841dd78e8b45bb95e30fc6b178d89c014eb1bcf5dfcde`.
Din explicitly requested implementation after receiving the reference.

`geometry.ts` is the single symmetric vector contour, technically traced from
the selected ∀ reference. Its flat ends, crossbar and triangular aperture are
retained. No font glyph, bitmap trace or Nd detail is embedded in the symbol.
`BrandMark` combines it with the ordinary project name, ALASHI NETWORK.
Georgia is a provisional UI name treatment, preserving the reference's serif
character. Amir has not approved the font family or final NETWORK lockup; this
delivery does not assert that a final outlined wordmark was approved.

Approved flat palette: lilac #A78BFA, silver #D9D9E2, dark #111018,
light #F4F2F8. The former sage/green page palette was different. The gameplay
shell now uses the reference neutrals; smaller interactive accents use derived
dark violet #6544A6 for readable contrast on the light background. The original
green Degenie materials and semantic confirmation/warning colors are preserved.
The Login owner/pairing implementation is not changed by a palette update.

The mark means “for all”; it does not promise free admission or product access.
Amir associates lilac with neodymium-colored glass, technology and the attraction
of participants to a network. The silver rim follows the supplied coin concept.
These are design associations, not claims about a token's composition, backing,
issuance or value. Nd/60 remains outside the logo and coins; FOR ALL is an
explanation on the concept sheet, not an added product slogan.

Exports in `public/brand`: separate lilac/dark/light SVG marks and a flat SVG coin,
plus actual transparent PNGs at 32/64/128/256/512/1024. Browser SVG/ICO, app icons
and touch icons use the same contour. From `frontend`, regenerate with:

```
node --experimental-strip-types scripts/export-brand.ts
python3 scripts/export-brand-png.py
```

PNG generation needs local CairoSVG/Pillow; these are not new frontend packages.
SVGs contain paths, not raster images. Artwork is local and needs no font or
image service at runtime. The UI uses a normal system serif fallback.

`coin.ts` uses that exact contour in owned Three.js geometry, with lilac fields,
silver rims and a raised silver mark on both faces. Buy/Sell replace the old
Bitcoin coin from the cloned prop scene; Mule replaces its old numeral coin.
Sizes/contact origins, action paths, durations, fade and idle return are retained.
The shared 3D coin now has a broad flat silver annulus with a short bevel,
a recessed lilac field and a raised silver mark on both faces. A locally generated
machining texture adds fine circular grain; mipmaps limit shimmer on the small
payment prop. Base colors stay #A78BFA and #D9D9E2. No lettering, Nd/60 detail
or gold finish is added. The radius and depth stay inside the previous contact
envelope. One shared contour supplies the website mark and coin geometry.

The [owner concept](../../../docs/assets/coin-20261010/owner-concept.png)
and [rendered coin](../../public/brand/alashi-coin-render.png) document the target
and implementation separately. `art/brand-coin-review.html` runs with the local
Vite development server and renders this same factory; it is not a public route.
Its PNG download captures a 1600px render. Flat SVG and transparent PNG icons
remain available at 32/64/128/256/512/1024px.
[Coin delivery and visual checks](../../../docs/ops/ALASHI_COIN_20261010.md)
record the reference, Lasseter application and phone frames.

Cached GLB/model/source `.blend` files are not rewritten. The dormant old coin
still exists in the stand/crate GLB, but is discarded before rendering; it is not
the runtime payment artwork. Coins are decorative props and add no currency,
balance or transaction. Both factories dispose owned geometry and materials.

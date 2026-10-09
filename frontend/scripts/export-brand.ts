// Run with Node's type stripping; raster exports are generated from these SVGs.
import { mkdir, writeFile } from 'node:fs/promises'
import { BRAND, SYMBOL_PATH } from '../src/brand/geometry.ts'
const output = new URL('../public/brand/', import.meta.url)
await mkdir(output, { recursive: true })
const svg = (viewBox: string, body: string) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}" role="img"><title>ALASHI NETWORK</title>${body}</svg>\n`
for (const [name, color] of [['', BRAND.lilac], ['-dark', BRAND.dark], ['-light', BRAND.light]] as const) {
  await writeFile(new URL(`alashi-forall-symbol${name}.svg`, output), svg('0 0 364 332', `<path fill="${color}" fill-rule="evenodd" d="${SYMBOL_PATH}"/>`))
}
const coin = svg('0 0 100 100', `<circle cx="50" cy="50" r="49" fill="${BRAND.silver}"/><circle cx="50" cy="50" r="44" fill="${BRAND.lilac}"/><path transform="translate(19 22) scale(.17)" fill="${BRAND.light}" fill-rule="evenodd" d="${SYMBOL_PATH}"/>`)
await writeFile(new URL('alashi-coin-flat.svg', output), coin)
await writeFile(new URL('../favicon.svg', output), svg('0 0 64 64', `<rect width="64" height="64" rx="12" fill="${BRAND.dark}"/><path transform="translate(10 12) scale(.12)" fill="${BRAND.lilac}" fill-rule="evenodd" d="${SYMBOL_PATH}"/>`))

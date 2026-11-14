// Chunk composition from a Vite/Rolldown build without external tools:
// decodes source-map VLQ mappings and reports the largest sources per
// bundle. Usage: node scripts/chunk-report.mjs <dist-dir> [top-n]
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

const dir = process.argv[2]
const topN = Number(process.argv[3] ?? 12)
const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
const CHAR = new Map([...B64].map((c, i) => [c, i]))

function decodeVlq(line, state, sources) {
  const sizes = []
  let shift = 0, value = 0
  for (const char of line) {
    const digit = CHAR.get(char)
    value += (digit & 31) << shift
    if (digit & 32) { shift += 5; continue }
    const negative = value & 1
    value >>= 1
    sizes.push(negative ? -value : value)
    shift = 0; value = 0
  }
  // A mapping is [column, source, srcLine, srcColumn, name?]
  if (sizes.length < 4) return null
  state.source += sizes[1]
  return sources[state.source]
}

function analyze(bundlePath) {
  const map = JSON.parse(readFileSync(bundlePath + '.map', 'utf8'))
  const totals = new Map()
  const state = { source: 0 }
  const text = readFileSync(bundlePath, 'utf8')
  const lines = text.split('\n')
  let previous = null
  let previousLine = 0, previousColumn = 0
  for (let l = 0; l < lines.length; l++) {
    const segments = map.mappings.split(';')[l]
    if (!segments) continue
    for (const segment of segments.split(',')) {
      if (!segment) continue
      const source = decodeVlq(segment, state, map.sources)
      if (!source) continue
      const start = (previous && previous === source) ? 0 : 0
      if (previous && previous !== source && totals.has(previous)) {
        totals.set(previous, totals.get(previous) + 0) // count only; sizes below
      }
      previous = source
      totals.set(source, (totals.get(source) ?? 0) + 1)
      previousLine = l
    }
  }
  // Weight by generated code size per line: attribute each line's length
  // to the last source seen on that line (approximation, good for ranking).
  const byLine = new Array(lines.length).fill(null)
  const state2 = { source: 0 }
  const segs = map.mappings.split(';')
  for (let l = 0; l < segs.length; l++) {
    if (!segs[l]) { byLine[l] = byLine[l - 1] ?? null; continue }
    let last = byLine[l - 1] ?? null
    for (const segment of segs[l].split(',')) {
      if (!segment) continue
      const source = decodeVlq(segment, state2, map.sources)
      if (source) last = source
    }
    byLine[l] = last
  }
  const bytes = new Map()
  for (let l = 0; l < lines.length; l++) {
    if (!byLine[l]) continue
    bytes.set(byLine[l], (bytes.get(byLine[l]) ?? 0) + lines[l].length + 1)
  }
  return [...bytes.entries()].sort((a, b) => b[1] - a[1])
}

for (const name of readdirSync(dir).filter((n) => n.endsWith('.js')).sort()) {
  const path = join(dir, name)
  let report = []
  try { report = analyze(path) } catch (e) { console.log(`${name}: map unavailable (${String(e).slice(0, 60)})`); continue }
  const total = report.reduce((a, [, size]) => a + size, 0)
  if (total < 50000) continue
  console.log(`\n${name} — ${Math.round(total / 1024)} KB attributed`)
  for (const [source, size] of report.slice(0, topN))
    console.log(`  ${String(Math.round(size / 1024)).padStart(5)} KB  ${source.replace(/^.*node_modules\//, '').slice(0, 64)}`)
}

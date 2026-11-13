#!/usr/bin/env node
// Paired per-seed analysis for the preregistered ablation
// (docs/200_ZETTEL/20261113-blind-ablation-prereg.md): pairs L4 games
// with uniform games on the same seed, reports per-seed differences in
// largest share, HHI and the conditional cash percentile, a sign test,
// a Wilcoxon signed-rank test with normal approximation and tie
// correction, and a bootstrap CI of the median difference.
// Usage: node world-bribe-paired.mjs <grid-base-dir> <armA-prefix> <armB-prefix>
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const base = process.argv[2], aPrefix = process.argv[3], bPrefix = process.argv[4];
const load = prefix => { const games = new Map();
  for (const name of readdirSync(base).filter(n => n.startsWith(prefix + '-') && n.endsWith('-deep.json'))) {
    const row = JSON.parse(readFileSync(join(base, name), 'utf8'));
    games.set(row.seed, row);
  } return games; };
const A = load(aPrefix), B = load(bPrefix);
const seeds = [...A.keys()].filter(s => B.has(s)).sort((x, y) => x - y);
const diffs = { largest_share: [], hhi: [], percentile: [] };
for (const seed of seeds) {
  const a = A.get(seed), b = B.get(seed);
  diffs.largest_share.push(a.largest_share - b.largest_share);
  diffs.hhi.push(a.hhi - b.hhi);
  diffs.percentile.push(a.mean_cash_percentile_within_offer - b.mean_cash_percentile_within_offer);
}
const binomP = k => { let p = 0; const n = seeds.length;
  for (let i = k; i <= n; i++) p += Math.comb(n, i); return p / 2 ** n; };
const wilcoxon = values => {
  const nonzero = values.map((v, i) => ({ v, i })).filter(x => x.v !== 0);
  const ranks = [...nonzero].sort((x, y) => Math.abs(x.v) - Math.abs(y.v))
    .map((x, index) => ({ ...x, rank: index + 1 }));
  let i = 0; while (i < ranks.length) { let j = i;
    while (j + 1 < ranks.length && Math.abs(ranks[j + 1].v) === Math.abs(ranks[i].v)) j++;
    const tie = (i + j + 2) / 2; for (let k2 = i; k2 <= j; k2++) ranks[k2].rank = tie; i = j + 1; }
  const wPlus = ranks.filter(x => x.v > 0).reduce((a, x) => a + x.rank, 0);
  const n = ranks.length;
  const tieGroups = new Map(); for (const x of ranks) tieGroups.set(Math.abs(x.v), (tieGroups.get(Math.abs(x.v)) ?? 0) + 1);
  const varW = n * (n + 1) * (2 * n + 1) / 6 - [...tieGroups.values()].reduce((a, t) => a + (t ** 3 - t) / 48, 0);
  const z = varW > 0 ? (wPlus - n * (n + 1) / 4) / Math.sqrt(varW) : 0;
  const p = 2 * (1 - normalCdf(Math.abs(z)));
  return { n_nonzero: n, w_plus: wPlus, z: round(z, 3), p_two_sided: round(p, 5) };
};
function normalCdf(x) { return 0.5 * (1 + erf(x / Math.SQRT2)); }
function erf(x) { const s = Math.sign(x); x = Math.abs(x);
  const t = 1 / (1 + 0.3275911 * x);
  const y = 1 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * Math.exp(-x * x);
  return s * y; }
function round(v, d = 4) { return Math.round(v * 10 ** d) / 10 ** d; }
function bootstrapMedian(values, iterations = 10000) {
  const medians = [];
  for (let i = 0; i < iterations; i++) {
    const sample = values.map(() => values[Math.floor(rand() * values.length)]);
    const sorted = [...sample].sort((x, y) => x - y);
    medians.push(sorted[Math.floor(sorted.length / 2)]);
  }
  medians.sort((x, y) => x - y);
  return { median: round(medians[Math.floor(iterations / 2)], 5),
    ci95: [round(medians[Math.floor(iterations * 0.025)], 5), round(medians[Math.floor(iterations * 0.975)], 5)] };
}
let rngState = 123456789n;
const rand = () => { rngState = (rngState * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
  return Number(rngState >> 11n) / 2 ** 53; };
const report = { armA: aPrefix, armB: bPrefix, paired_seeds: seeds.length };
for (const [name, values] of Object.entries(diffs)) {
  const positive = values.filter(v => v > 0).length, negative = values.filter(v => v < 0).length;
  report[name] = { mean_diff: round(values.reduce((a, b) => a + b, 0) / values.length, 5),
    positive: positive, negative: negative,
    sign_p_one_sided_greater: round(binomP(Math.max(positive, negative)) * 1, 5),
    wilcoxon: wilcoxon(values), median_bootstrap: bootstrapMedian(values),
    per_seed: values.map(round) };
}
process.stdout.write(JSON.stringify(report, null, 2) + '\n');

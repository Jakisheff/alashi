#!/usr/bin/env node
// Aggregate the preregistered bribe grid
// (docs/200_ZETTEL/20261113-bribe-grid-prereg.md) from per-game deep
// reports plus the engine-null uniform arm, into one summary table.
// Pre-place the seed-20261109 L4 deep report as l4-20261109-deep.json.
// Usage: node world-bribe-grid-summary.mjs <grid-base-dir>
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const base = process.argv[2];
const rows = [];
for (const name of readdirSync(base).filter(n => n.endsWith('-deep.json')).sort()) {
  rows.push(JSON.parse(readFileSync(join(base, name), 'utf8')));
}
const quantile = (values, q) => { const s = [...values].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor(q * s.length))]; };
const arm = row => row.model.includes('l4') ? 'l4-richest'
  : row.model.includes('first-slot') ? 'first-slot'
  : row.model.includes('uniform') ? 'uniform-legal' : 'model';
const byArm = {};
for (const row of rows) { (byArm[arm(row)] ??= []).push(row); }
const summary = {};
for (const [name, games] of Object.entries(byArm)) {
  summary[name] = { games: games.length,
    seeds: games.map(g => g.seed),
    n_bribe_decisions: games.map(g => g.n_bribe_decisions),
    largest_share: games.map(g => round(g.largest_share)),
    hhi: games.map(g => round(g.hhi)),
    mean_cash_percentile: games.map(g => round(g.mean_cash_percentile_within_offer)),
    perm_p_share: games.map(g => g.perm_p_share),
    perm_p_hhi: games.map(g => g.perm_p_hhi) };
  if (name === 'uniform-legal') {
    summary[name].null_quantiles = {
      largest_share_p50: round(quantile(games.map(g => g.largest_share), 0.5)),
      largest_share_p95: round(quantile(games.map(g => g.largest_share), 0.95)),
      hhi_p50: round(quantile(games.map(g => g.hhi), 0.5)),
      hhi_p95: round(quantile(games.map(g => g.hhi), 0.95)) };
  }
}
function round(v) { return v === null || v === undefined ? null : Math.round(v * 10000) / 10000; }
process.stdout.write(JSON.stringify(summary, null, 2) + '\n');

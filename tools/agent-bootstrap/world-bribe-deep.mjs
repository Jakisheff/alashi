#!/usr/bin/env node
// Deep bribe analysis for the preregistered grid
// (docs/200_ZETTEL/20261113-bribe-grid-prereg.md). Reads the raw calls/
// and phases/ of one world-run directory and computes, without trusting
// any earlier summary: accepted bribe concentration (largest recipient
// share, HHI), the conditional cash percentile of the chosen target
// within that decision's offered set, the structural HHI expected under
// uniform-within-offer choices on the same recorded offers, and the
// one-sided permutation p-values for share and HHI (B resamples of
// uniform target re-picks on the same offer sets).
// Usage: node world-bribe-deep.mjs <world-dir> [B] [--true-cash]
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { createHash } from 'node:crypto';

const dir = process.argv[2];
const B = Number(process.argv[3] ?? 2000);
const trueCash = process.argv.includes('--true-cash');
if (!dir) { console.error('usage: node world-bribe-deep.mjs <world-dir> [B] [--true-cash]'); process.exit(1); }

const frozen = JSON.parse(readFileSync(join(dir, 'frozen.json'), 'utf8'));

// Under --true-cash, the cash used for the within-offer percentile comes
// from the engine snapshot at the decision step, not from the prompt;
// this is what the preregistered blind ablation requires, because the
// blind driver zeroes the prompt's cash channel.
const snapshotCash = step => {
  const path = join(dir, 'snapshots', String(step).padStart(4, '0') + '.json');
  if (!existsSync(path)) return null;
  const snapshot = JSON.parse(readFileSync(path, 'utf8'));
  return new Map(snapshot.world.public_state.map(row => [row.actor, row.cash]));
};
const receiptsByStep = new Map();
for (const name of readdirSync(join(dir, 'phases')).filter(n => /^[0-9]{4}\.json$/.test(n))) {
  const phase = JSON.parse(readFileSync(join(dir, 'phases', name), 'utf8'));
  if (phase.response) for (const r of phase.response.receipts)
    receiptsByStep.set(`${phase.request.expected_step}:${r.actor}`, r);
}

// A bribe decision: the offered target set with visible cash, plus the
// accepted chosen target. Offers come from the recorded prompt menus.
const decisions = [];
for (const name of readdirSync(join(dir, 'calls')).filter(n => /^[0-9]{4}-[0-9]{3}\.json$/.test(n)).sort()) {
  const call = JSON.parse(readFileSync(join(dir, 'calls', name), 'utf8'));
  if (call.phase !== 'action' || call.result?.action?.action !== 'bribe') continue;
  const receipt = receiptsByStep.get(`${call.step}:${call.actor}`);
  if (!receipt?.accepted || receipt.skipped) continue;
  const prompt = JSON.parse(call.messages[1].content);
  const offers = prompt.allowed_actions.filter(a => a.action === 'bribe').map(a => a.params.to);
  if (!offers.includes(call.result.action.params.to)) continue;
  const cash = new Map(prompt.selected_opponents.map(row => [row[0], row[1]]));
  decisions.push({ step: call.step, offers, chosen: call.result.action.params.to, cash });
}

const countsOf = picks => { const counts = new Map(); let i = 0;
  for (const d of decisions) { const target = picks ? picks[i++] : d.chosen;
    counts.set(target, (counts.get(target) ?? 0) + 1); }
  return counts; };
const statsOf = counts => { const total = [...counts.values()].reduce((a, b) => a + b, 0);
  return { total, distinct: counts.size,
    largest_share: total ? Math.max(...counts.values()) / total : 0,
    hhi: total ? [...counts.values()].reduce((a, b) => a + (b / total) ** 2, 0) : 0 }; };

const observed = { n_bribe_decisions: decisions.length, ...statsOf(countsOf()) };

// Conditional cash percentile of the chosen target inside its offer set
// (rank 1..k mapped to (rank-0.5)/k; uniform choice gives expectation 0.5).
const percentiles = decisions.map(d => {
  const cashMap = trueCash ? snapshotCash(d.step) : d.cash;
  const at = to => (trueCash ? cashMap?.get(to) : d.cash.get(to)) ?? -1;
  const values = d.offers.map(at).sort((a, b) => a - b);
  const rank = values.indexOf(at(d.chosen)) + 1;
  const same = values.filter(v => v === at(d.chosen)).length;
  const low = values.indexOf(at(d.chosen));
  return { raw: (rank - 0.5) / values.length,
    midrank: (low + (same - 1) / 2 + 0.5) / values.length };
});
const mean = key => percentiles.length
  ? percentiles.reduce((a, p) => a + p[key], 0) / percentiles.length : null;
const mean_cash_percentile = mean('raw');
const mean_cash_percentile_midrank = mean('midrank');

// Structural HHI: expected recipient shares under uniform-within-offer
// target picks on the same recorded offer sets.
const expected = new Map();
for (const d of decisions) for (const to of d.offers) expected.set(to, (expected.get(to) ?? 0) + 1 / d.offers.length);
const expectedTotal = [...expected.values()].reduce((a, b) => a + b, 0);
const shares = [...expected.values()].map(v => v / expectedTotal);
const structural_hhi = expectedTotal ? shares.reduce((a, v) => a + v * v, 0) : 0;
// E[HHI] under the same null includes multinomial variance at this N.
const expected_hhi = expectedTotal
  ? structural_hhi + shares.reduce((a, v) => a + v * (1 - v), 0) / expectedTotal : 0;

// Deterministic permutation null: uniform-within-offer re-picks.
const rngState = { s: BigInt('0x' + createHash('sha256').update(dir + frozen.digest).digest('hex').slice(0, 16)) };
const rand = () => { rngState.s = (rngState.s * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
  return Number(rngState.s >> 11n) / 2 ** 53; };
let geShare = 0, geHhi = 0;
const pick = () => decisions.map(d => d.offers[Math.floor(rand() * d.offers.length) % d.offers.length]);
for (let i = 0; i < B; i++) { const s = statsOf(countsOf(pick()));
  if (s.largest_share >= observed.largest_share) geShare++;
  if (s.hhi >= observed.hhi) geHhi++; }

const report = { digest: frozen.digest, seed: frozen.config.init.seed,
  model: frozen.config.profile.model, source: frozen.config.profile.source,
  n_bribe_decisions: observed.n_bribe_decisions, distinct_recipients: observed.distinct,
  largest_share: observed.largest_share, hhi: observed.hhi,
  structural_hhi_uniform_within_offer: structural_hhi,
  expected_hhi_uniform_within_offer: expected_hhi,
  choice_excess_hhi: observed.hhi - structural_hhi,
  mean_cash_percentile_within_offer: mean_cash_percentile,
  mean_cash_percentile_midrank_within_offer: mean_cash_percentile_midrank,
  perm_p_share: geShare / B, perm_p_hhi: geHhi / B, B };
process.stdout.write(JSON.stringify(report, null, 2) + '\n');

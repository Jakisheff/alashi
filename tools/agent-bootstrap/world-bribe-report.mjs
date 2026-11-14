#!/usr/bin/env node
// Bribe concentration report for a world-run directory. Counts offered and
// accepted bribes from the recorded phase receipts, then reports recipient
// counts, the largest recipient share and the recipient HHI the same way the
// world-100 bribe-fix comparison did (share by accepted transfer count; the
// epoch-0 bribe price is fixed, so count and amount carry the same shape).
// Usage: node world-bribe-report.mjs <world-dir>
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const dir = process.argv[2];
if (!dir) { console.error('usage: node world-bribe-report.mjs <world-dir>'); process.exit(1); }
const summary = JSON.parse(readFileSync(join(dir, 'summary.json'), 'utf8'));
const offered = new Map(), accepted = new Map();
let offeredCount = 0, acceptedCount = 0, rejectedCount = 0;
for (const name of readdirSync(join(dir, 'phases')).filter(n => /^[0-9]{4}\.json$/.test(n)).sort()) {
  const phase = JSON.parse(readFileSync(join(dir, 'phases', name), 'utf8'));
  const byActor = new Map((phase.response?.receipts ?? []).map(r => [r.actor, r]));
  for (const decision of phase.request.decisions) {
    if (decision.action !== 'bribe') continue;
    offeredCount++;
    offered.set(decision.params.to, (offered.get(decision.params.to) ?? 0) + 1);
    const receipt = byActor.get(decision.actor);
    if (receipt?.accepted && !receipt.skipped) {
      acceptedCount++;
      accepted.set(decision.params.to, (accepted.get(decision.params.to) ?? 0) + 1);
    } else if (receipt && !receipt.accepted) rejectedCount++;
  }
}
const share = map => { const total = [...map.values()].reduce((a, b) => a + b, 0);
  return { total, distinct: map.size,
    largest: total ? Math.max(...map.values()) : 0,
    largest_share: total ? Math.max(...map.values()) / total : 0,
    hhi: total ? [...map.values()].reduce((a, b) => a + (b / total) ** 2, 0) : 0 }; };
const report = { digest: JSON.parse(readFileSync(join(dir, 'frozen.json'), 'utf8')).digest,
  status: summary.status, calls: summary.calls, accepted_actions: summary.accepted_actions,
  rejected_actions: summary.rejected_actions, skipped_decisions: summary.skipped_decisions,
  bribes_offered: offeredCount, bribes_accepted: acceptedCount, bribes_rejected: rejectedCount,
  offered_by_recipient: Object.fromEntries([...offered.entries()].sort((a, b) => b[1] - a[1])),
  accepted_by_recipient: Object.fromEntries([...accepted.entries()].sort((a, b) => b[1] - a[1])),
  offered_concentration: share(offered), accepted_concentration: share(accepted) };
process.stdout.write(JSON.stringify(report, null, 2) + '\n');

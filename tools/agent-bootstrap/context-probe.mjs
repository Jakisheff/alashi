#!/usr/bin/env node
// Context probe: a standing detector of context blindness in recorded
// world runs. Replays recorded prompts for the focal agent with a
// deterministic control arm and an advisory-note arm. The control arm
// must reproduce the recorded answers bit for bit (the run profile is
// temperature 0); otherwise the probe is inconclusive. If the control
// reproduces and the advisory arm still reproduces every recorded
// answer, the decision layer provably ignored information that looked
// relevant. Advisory: a blind verdict does not invalidate the run.
//
// Usage: node context-probe.mjs <world-dir> [--focal N] [--limit K]
//          [--note "text"] [--phase market]
// Writes <world-dir>/context-probe.json and prints a summary.

import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { bitllmProvider } from './world-run.mjs';
import { DEFAULT_NOTE, withNoteProbe, verdictOf } from './context-probe-lib.mjs';

const args = process.argv.slice(2);
const dir = args.shift();
const opt = { focal: null, limit: 6, note: null, phase: 'market' };
while (args.length) {
  const a = args.shift();
  if (a === '--focal') opt.focal = Number(args.shift());
  else if (a === '--limit') opt.limit = Number(args.shift());
  else if (a === '--note') opt.note = args.shift();
  else if (a === '--phase') opt.phase = args.shift();
  else { console.error(`unknown option ${a}`); process.exit(2); }
}
if (!dir) { console.error('usage: context-probe.mjs <world-dir> [--focal N] [--limit K] [--note text] [--phase p]'); process.exit(2); }

const j = (p) => JSON.parse(readFileSync(join(dir, p), 'utf8'));
const frozen = j('frozen.json');
const provider = bitllmProvider(frozen.config.profile, {});

const focal = opt.focal ?? frozen.config.comparison?.focal_actor ?? null;
if (focal === null) { console.error('no focal actor: pass --focal N'); process.exit(2); }

const calls = readdirSync(join(dir, 'calls'))
  .map(n => j(join('calls', n)))
  .filter(c => c.actor === focal && c.status === 'completed' && (opt.phase === 'all' || c.phase === opt.phase))
  .sort((a, b) => a.step - b.step)
  .slice(0, opt.limit);
if (!calls.length) { console.error(`no completed ${opt.phase} calls for actor ${focal}`); process.exit(2); }

const rows = [];
for (const c of calls) {
  const control = await provider.complete(c.messages);
  const probe = await provider.complete(withNoteProbe(c.messages, opt.note ?? DEFAULT_NOTE));
  rows.push({ step: c.step, round: c.round, phase: c.phase,
    control_equal: control.content === c.result.content,
    probe_equal: probe.content === c.result.content });
}
const verdict = verdictOf(rows);
const report = {
  schema: 'alashi-context-probe-v1',
  generated_at: new Date().toISOString(),
  world_digest: frozen.digest,
  focal_actor: focal, phase: opt.phase, checked: rows.length,
  note: opt.note ?? DEFAULT_NOTE,
  control_equal: rows.filter(r => r.control_equal).length,
  probe_equal: rows.filter(r => r.probe_equal).length,
  verdict, rows,
};
writeFileSync(join(dir, 'context-probe.json'), JSON.stringify(report, null, 2) + '\n');
console.log(`context probe: ${verdict} (control ${report.control_equal}/${rows.length}, probe ${report.probe_equal}/${rows.length}, steps ${rows.map(r => r.step).join(',')})`);
console.log(verdict === 'blind_to_advisory'
  ? 'the decision layer reproduced every recorded answer while ignoring the advisory note; the run is valid but context-blind'
  : verdict === 'sensitive_to_advisory'
    ? 'the decision layer changed at least one answer under the advisory note'
    : 'the model did not reproduce recorded answers; probe inconclusive');
process.exit(verdict === 'inconclusive_control_mismatch' ? 1 : 0);

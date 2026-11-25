#!/usr/bin/env node
// ES experiment reward harness: one episode = one small world (6 agents:
// focal via the composite provider -> model server, five scripted via
// uniform policy inside the same provider). Prints a JSON reward record.
// Usage: node es-reward.mjs <world-dir> [rounds-config...]
// Requires: world_worker binary built; mixed-provider.mjs and a model
// server (stub or real) already running on loopback.
import { spawnSync } from 'node:child_process';
import { readFileSync, rmSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const [dir, seedArg, agentsArg] = process.argv.slice(2);
const worker = resolve(process.env.ES_WORKER ?? '../../arena/target/debug/world_worker');
const seed = Number(seedArg ?? 20261125);
const agents = Number(agentsArg ?? 6);
const run = (args) => {
  const result = spawnSync('node', [resolve('world-run.mjs'), ...args], { encoding: 'utf8' });
  if (result.status !== 0) {
    console.error('CMD:', 'node world-run.mjs', args.join(' '));
    console.error('STDOUT:', result.stdout);
    console.error('STDERR:', result.stderr);
    throw new Error(`world-run failed: ${result.stderr}`);
  }
  return result.stdout;
};

const dirAbs = resolve(dir);
rmSync(dirAbs, { recursive: true, force: true });
run(['init', '--dir', dirAbs, '--worker', worker, '--agents', String(agents), '--seed', String(seed),
  '--game-id', String(seed), '--epoch', '0', '--payout-policy', 'winner_takes_all',
  '--url', 'http://127.0.0.1:7780/v1/chat/completions', '--model', 'mixed-focal',
  '--source', 'test_provider', '--parallelism', '3', '--max-calls', '500',
  '--max-seconds', '600']);
run(['run', '--dir', dirAbs, '--worker', worker, '--allow-provider', 'yes']);
const summary = JSON.parse(run(['report', '--dir', dirAbs]));
const snapshot = JSON.parse(readFileSync(`${dirAbs}/final-snapshot.json`, 'utf8'));
const result = summary.result ?? {};

// Pathology counters for the focal actor (actor 0) from the phase journals.
let invalidFocal = 0, callsFocal = 0, bribesFocal = 0;
const phaseDir = `${dirAbs}/phases`;
const phaseFiles = readdirSync(phaseDir).sort();
for (const f of phaseFiles) {
  const p = JSON.parse(readFileSync(`${phaseDir}/${f}`, 'utf8'));
  const decisions = p.request?.decisions ?? [];
  for (const d of decisions) {
    if (d.actor !== 0) continue;
    callsFocal++;
    if (d.error) invalidFocal++;
    if (d.action?.action === 'bribe') bribesFocal++;
  }
}
const focalRow = (result.leaderboard ?? []).find(r => r.actor === 0);
const reward = {
  episode_seed: seed, agents, rounds: snapshot.round,
  focal_cash: focalRow?.cash ?? null, focal_payout: focalRow?.payout ?? null, focal_rank: focalRow?.rank ?? null,
  winner_actor: result.winner_actor ?? (result.leaderboard ?? []).find(r => r.rank === 1)?.actor ?? null,
  focal_calls: callsFocal, focal_invalid: invalidFocal, focal_bribes: bribesFocal,
  accepted: summary.accepted_actions ?? null, rejected: summary.rejected_actions ?? null,
  calls: summary.calls ?? null, status: summary.status,
};
console.log(JSON.stringify(reward));

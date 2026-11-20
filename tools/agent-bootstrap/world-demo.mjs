#!/usr/bin/env node
/** One-command deterministic demo world (issue #1451 follow-up artifact).
 * Runs a complete N-agent ALASHI game through the real Rust worker with the
 * uniform-legal null arm: no model keys, no network beyond loopback, no chain.
 * Same seed + same worker + same driver => bit-identical state_hash.
 *
 * Usage:
 *   cargo build --manifest-path arena/Cargo.toml --locked --bin world_worker
 *   node tools/agent-bootstrap/world-demo.mjs \
 *     --worker "$PWD/arena/target/debug/world_worker" \
 *     --dir "$(mktemp -d)/world" [--agents 8] [--seed 20261121]
 */
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { initWorld, runWorld } from './world-run.mjs';
import { serve, UNIFORM_MODEL } from './uniform-legal-provider.mjs';

const fail = code => { throw new Error(code); };
const opts = {};
for (let i = 2; i < process.argv.length; i += 2) {
  const key = process.argv[i];
  if (!/^--[a-z-]+$/.test(key) || !process.argv[i + 1] || Object.hasOwn(opts, key.slice(2)))
    fail('usage_world_demo');
  opts[key.slice(2)] = process.argv[i + 1];
}
const allowed = ['worker', 'dir', 'agents', 'seed'];
if (!opts.worker || !opts.dir || Object.keys(opts).some(k => !allowed.includes(k))) fail('usage_world_demo');

const dir = resolve(opts.dir), worker = resolve(opts.worker);
if (!existsSync(worker)) fail('world_worker_missing_build_it_first');
if (existsSync(dir)) fail('world_demo_dir_must_be_fresh');
const agents = Number(opts.agents ?? 8), seed = Number(opts.seed ?? 20261121);

const { server, port } = await serve(0);
try {
  initWorld(dir, {
    worker, agents, seed, gameId: seed, epoch: 0, payoutPolicy: 'winner_takes_all',
    url: `http://127.0.0.1:${port}/v1/chat/completions`, model: UNIFORM_MODEL, source: 'test_provider',
    parallelism: 2, maxCalls: 400, maxSeconds: 600,
  });
  const summary = await runWorld({ dir, allowProvider: true, workerPath: worker });
  const top = summary.result.leaderboard.slice(0, 3)
    .map(a => `${a.name} cash=${a.cash}`).join(' | ');
  process.stdout.write(JSON.stringify({
    status: summary.status, agents: summary.agents, seed,
    calls: summary.calls, accepted_actions: summary.accepted_actions,
    rejected_actions: summary.rejected_actions, skipped_decisions: summary.skipped_decisions,
    winner: `agent-${String(summary.result.winner_actor).padStart(3, '0')}`,
    payout: summary.result.payouts[summary.result.winner_actor], rake: summary.result.rake,
    state_hash: summary.state_hash, digest: summary.digest, evidence_dir: dir,
    top3: top,
  }, null, 2) + '\n');
} finally { server.close(); }

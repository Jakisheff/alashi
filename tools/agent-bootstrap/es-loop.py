#!/usr/bin/env python3
"""ES training loop for the alashi-rules reward experiment.

Design (pre-registered 2026-11-25, alashi docs/research/preprint-notes):
- Population N=30, sigma=0.001, alpha=5e-4 — the ES-at-scale fixed set.
- Reward: mean focal payoff over K episodes of a 6-agent world
  (focal = the model served by es-model-server.py; five scripted
  uniform opponents inside mixed-provider.mjs).
- Loop: for each member: perturb in place (seeded) -> run K episodes
  (node es-reward.mjs) -> restore; aggregate update weighted by
  z-scored rewards. Episodes per member share the member's seed bank.

Control channel: this script talks to es-model-server.py over a small
JSON control port (perturb/restore) instead of importing it, so the
server keeps a single model instance in GPU memory.
"""
import json
import subprocess
import sys
import urllib.request

import torch

CONTROL = "http://127.0.0.1:7781/control"
REWARD_CMD = ["node", "es-reward.mjs"]


def control(action, sigma, seed):
    body = json.dumps({"action": action, "sigma": sigma, "seed": seed}).encode()
    req = urllib.request.Request(CONTROL, data=body, headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.load(response)


def episodes(member_seed, k, base_dir):
    payoffs, invalids = [], []
    for k_i in range(k):
        seed = member_seed * 1000 + k_i
        out = subprocess.run(REWARD_CMD + [f"{base_dir}/m{member_seed}-e{k_i}", str(seed)],
                             capture_output=True, text=True, check=True)
        record = json.loads(out.stdout.strip().splitlines()[-1])
        payoffs.append(record["focal_payout"] + record["focal_cash"] / 10_000_000_000)
        invalids.append(record["focal_invalid"])
    return sum(payoffs) / len(payoffs), sum(invalids) / len(invalids)


def zscore(values):
    mean = sum(values) / len(values)
    var = sum((v - mean) ** 2 for v in values) / len(values)
    std = var ** 0.5 or 1.0
    return [(v - mean) / std for v in values]


def train(iterations=100, population=30, sigma=0.001, alpha=5e-4, episodes_per_member=2,
          base_dir="/tmp/es-worlds", model_path=None):
    # The update is applied in the server process; this loop drives
    # perturb/evaluate/restore cycles and computes the aggregate update,
    # then submits it once per iteration (seeded, layer-by-layer).
    for t in range(iterations):
        rewards = []
        for n in range(population):
            seed = t * population + n + 1
            control("perturb", sigma, seed)
            reward, _ = episodes(seed, episodes_per_member, base_dir)
            control("restore", sigma, seed)
            rewards.append(reward)
            print(f"iter {t} member {n}: reward={reward:.2f}", file=sys.stderr)
        weights = zscore(rewards)
        # Aggregate update: theta += alpha * mean(w_n * eps_n); the server
        # regenerates eps_n from the same seeds and applies the weighted sum.
        control("update", alpha, seeds=list(range(t * population + 1, t * population + population + 1)),
                weights=weights, sigma=sigma)
        print(f"iter {t}: mean={sum(rewards)/len(rewards):.2f} best={max(rewards):.2f}", file=sys.stderr)


if __name__ == "__main__":
    train(iterations=int(sys.argv[1]) if len(sys.argv) > 1 else 100)

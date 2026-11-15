#!/usr/bin/env python3
"""Generate protocol.json: seeds, jobs, hashes, limits. Deterministic (PRNG 20261116)."""

import datetime
import hashlib
import json
import random
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SEED_PRNG = 20261116
N_SEEDS = 36
ARMS = ['mid', 'front']
OPPONENTS = ['greedy', 'tactical']
SCRIPTS = ['common.py', 'model_api.py', 'collect.py']


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    rng = random.Random(SEED_PRNG)
    seeds = [rng.getrandbits(63) for _ in range(N_SEEDS)]
    jobs = [
        {
            'id': f't{seed_index:02d}-seat0-{arm}',
            'seed_index': seed_index,
            'seed': seed,
            'seat': 0,
            'arm': arm,
            'opponents': list(OPPONENTS),
            'partition': 'test',
        }
        for seed_index, seed in enumerate(seeds)
        for arm in ARMS
    ]
    old = json.loads((ROOT / '../harness-learning-20261020/protocol.json').read_text())
    commit = subprocess.run(
        ['git', 'rev-parse', '--short', 'HEAD'], cwd=ROOT.parents[2], capture_output=True, text=True
    ).stdout.strip()
    protocol = {
        'schema': 1,
        'created_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds'),
        'status': 'frozen_before_collection',
        'study': 'prefix-order-ab',
        'question': 'Does moving the stable identity+menu block ahead of the dynamic state raise provider prefix-cache hits and cut billed prompt tokens, without shifting behavior?',
        'model': 'glm-5.3',
        'executor_settings': old['executor_settings'],
        'collection_limits': {'calls': 3200, 'prompt_tokens': 5600000, 'completion_tokens': 800000, 'wall_s': 7200},
        'workers': 6,
        'seed_prng': SEED_PRNG,
        'seeds': seeds,
        'arms': {
            'mid': 'user = OWNER + state + identity + menu (current agent.rs order, agent.rs:1394-1399)',
            'front': 'user = OWNER + identity + menu + state (cache-friendly candidate from prefix-audit.md)',
        },
        'primary': 'cached prompt tokens per call (usage.prompt_tokens_details.cached_tokens) and billed prompt tokens per call (usage.prompt_tokens), front vs mid, paired by seed',
        'secondary': 'payout paired exact Wilcoxon front-mid per seed cluster; parse-error and illegal-action games via McNemar',
        'engine_source_commit': commit,
        'engine_binary_sha256': sha(ROOT / 'bin/evalgame'),
        'jobs': jobs,
        'hashes': {name: sha(ROOT / name) for name in SCRIPTS},
    }
    path = ROOT / 'protocol.json'
    tmp = path.with_suffix('.json.tmp')
    tmp.write_text(json.dumps(protocol, ensure_ascii=False, indent=2, allow_nan=False) + '\n', encoding='utf-8')
    tmp.replace(path)
    print(f'protocol written: {len(jobs)} jobs, {N_SEEDS} seeds, engine {protocol["engine_binary_sha256"][:16]}..., commit {commit}')
    return 0


if __name__ == '__main__':
    sys.exit(main())

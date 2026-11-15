#!/usr/bin/env python3
"""Generate protocol.json: seeds, jobs, hashes, limits. Deterministic (PRNG 20261115)."""

import datetime
import hashlib
import json
import random
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SEED_PRNG = 20261115
N_SEEDS = 36
ARMS = ['naked', 'quickstart', 'skill']
OPPONENTS = ['greedy', 'tactical']
SCRIPTS = ['common.py', 'model_api.py', 'discover.py', 'safe_runtime.py', 'collect.py']


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
        'study': 'agent-context-ab',
        'question': 'Does a compact procedural context (skill) change payout versus documentation (quickstart) and no addition (naked), paired by seed?',
        'model': 'glm-5.3',
        'executor_settings': old['executor_settings'],
        'collection_limits': {'calls': 3600, 'prompt_tokens': 9000000, 'completion_tokens': 900000, 'wall_s': 10800},
        'workers': 6,
        'seed_prng': SEED_PRNG,
        'seeds': seeds,
        'arms': {
            'naked': 'base RULES only; memory.md empty',
            'quickstart': 'docs/QUICKSTART_JURY.md appended to system prompt verbatim',
            'skill': 'compact procedural memory derived from castdev post-mortems (inbox/Agent3, 2026-09-02)',
        },
        'arm_surface': {
            'naked': 0, 'quickstart': (ROOT / 'arms/quickstart/memory.md').stat().st_size,
            'skill': (ROOT / 'arms/skill/memory.md').stat().st_size,
        },
        'primary': 'mean skill payout minus mean naked payout per seed cluster (36 clusters, seat 0, opponents greedy+tactical); two-sided exact Wilcoxon via tools/eval_report.py; bootstrap CI seed 20260907',
        'protocol_errors': 'first-attempt validation errors per arm from game-calls/*.validation-error.json; expected near ceiling (0 errors in ~1400 prior glm-5.3 responses)',
        'engine_source_commit': commit,
        'engine_binary_sha256': sha(ROOT / 'bin/evalgame'),
        'jobs': jobs,
        'hashes': {name: sha(ROOT / name) for name in SCRIPTS}
                 | {f'arms/{arm}/{fn}': sha(ROOT / 'arms' / arm / fn) for arm in ARMS for fn in ['memory.md', 'helper.py']},
    }
    path = ROOT / 'protocol.json'
    tmp = path.with_suffix('.json.tmp')
    tmp.write_text(json.dumps(protocol, ensure_ascii=False, indent=2, allow_nan=False) + '\n', encoding='utf-8')
    tmp.replace(path)
    print(f'protocol written: {len(jobs)} jobs, {N_SEEDS} seeds, engine {protocol["engine_binary_sha256"][:16]}..., commit {commit}')
    return 0


if __name__ == '__main__':
    sys.exit(main())

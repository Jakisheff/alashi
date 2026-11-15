#!/usr/bin/env python3
"""Analyze the prefix-order series: tokens per call (primary), behavior (secondary).

Writes results.json + game-metrics.json (passk_analysis.py compatible),
prints headline numbers. Reproduction: python3 analysis.py
"""

import glob
import json
import math
import random
import statistics as stats
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
ARMS = ['mid', 'front']
PHASES = ['market', 'action', 'law']
BOOTSTRAP_SEED = 20260907
BOOTSTRAP_REPS = 10000


def read(path):
    return json.loads(Path(path).read_text())


def exact_wilcoxon_two_sided(diffs):
    vals = [d for d in diffs if d != 0]
    n = len(vals)
    if n == 0:
        return 1.0
    order = sorted(range(n), key=lambda i: abs(vals[i]))
    ranks = [0.0] * n
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and abs(vals[order[j + 1]]) == abs(vals[order[i]]):
            j += 1
        avg = (i + j) / 2 + 1
        for k in range(i, j + 1):
            ranks[order[k]] = avg
        i = j + 1
    total = sum(ranks)
    if n > 16:
        mean_w = total / 2
        sd_w = math.sqrt(sum(r * r for r in ranks) / 4)
        z = (sum(r for r, v in zip(ranks, vals) if v > 0) - mean_w) / sd_w if sd_w else 0.0
        return min(1.0, 2 * (1 - 0.5 * (1 + math.erf(abs(z) / math.sqrt(2)))))
    observed = sum(r for r, v in zip(ranks, vals) if v > 0)
    extreme = observed if observed <= total / 2 else total - observed
    count_le = 0
    for mask in range(1 << n):
        w = sum(ranks[i] for i in range(n) if mask & (1 << i))
        if w <= extreme + 1e-9:
            count_le += 1
    return min(1.0, 2 * count_le / (1 << n))


def bootstrap_ci(diffs, seed=BOOTSTRAP_SEED, reps=BOOTSTRAP_REPS):
    rng = random.Random(seed)
    n = len(diffs)
    means = sorted(sum(rng.choice(diffs) for _ in range(n)) / n for _ in range(reps))
    return means[int(0.025 * (reps - 1))], means[int(0.975 * (reps - 1))]


def phase_map():
    out = {}
    for trace in glob.glob(str(ROOT / 'runs/*/trace.jsonl')):
        arm = Path(trace).parent.name.rsplit('-', 1)[-1]
        for line in Path(trace).read_text().splitlines():
            row = json.loads(line)
            out[row['request_tag']] = row.get('phase')
    return out


def arm_usage(arm, phases):
    calls = prompt = completion = cached = 0
    per_phase = {p: {'calls': 0, 'prompt': 0, 'cached': 0} for p in PHASES}
    for path in glob.glob(str(ROOT / 'game-calls' / f'*-{arm}-s*.response.json')):
        raw = read(path)['raw']
        u = raw.get('usage', {})
        tag = Path(path).name.replace('.response.json', '')
        prompt += u.get('prompt_tokens', 0)
        completion += u.get('completion_tokens', 0)
        cached += (u.get('prompt_tokens_details') or {}).get('cached_tokens', 0)
        calls += 1
        phase = phases.get(tag)
        if phase in per_phase:
            per_phase[phase]['calls'] += 1
            per_phase[phase]['prompt'] += u.get('prompt_tokens', 0)
            per_phase[phase]['cached'] += (u.get('prompt_tokens_details') or {}).get('cached_tokens', 0)
    return {'calls': calls, 'prompt_tokens': prompt, 'completion_tokens': completion, 'cached_tokens': cached, 'per_phase': per_phase}


def main():
    results = [read(p) for p in sorted(glob.glob(str(ROOT / 'runs/*/result.json')))]
    completed = [r for r in results if r['status'] == 'completed']
    failed = len(results) - len(completed)
    seeds = sorted({r['seed_index'] for r in completed})
    protocol = read(ROOT / 'protocol.json')
    phases = phase_map()

    game_metrics = {}
    for r in completed:
        game = read(ROOT / 'runs' / r['id'] / 'game.json')
        license_won = (game.get('license') or {}).get('holder_idx') == r['seat']
        game_metrics[r['id']] = {
            'wealth_first': r['rank'] == 1,
            'license_won': license_won,
            'payout': r['payout'],
            'rank': r['rank'],
            'parse_errors': len(r.get('parse_errors', [])),
            'illegal_actions': r.get('illegal_actions', 0),
            'decisions': r['decisions'],
            'arm': r['arm'],
            'seed_index': r['seed_index'],
        }

    usage = {arm: arm_usage(arm, phases) for arm in ARMS}
    for u in usage.values():
        u['saved_responses'] = u['calls']
    tokens = {}
    for arm, u in usage.items():
        calls = max(1, u['calls'])
        tokens[arm] = {
            'prompt_per_call': u['prompt_tokens'] / calls,
            'cached_per_call': u['cached_tokens'] / calls,
            'cached_share': u['cached_tokens'] / max(1, u['prompt_tokens']),
            'completion_per_call': u['completion_tokens'] / calls,
        }

    def seed_calls(arm):
        out = {}
        for trace in glob.glob(str(ROOT / 'runs/*/trace.jsonl')):
            if not Path(trace).parent.name.endswith('-' + arm):
                continue
            seed_index = int(Path(trace).parent.name.split('-')[0][1:])
            prompt = cached = n = 0
            for line in Path(trace).read_text().splitlines():
                row = json.loads(line)
                tag = row.get('request_tag')
                if not tag:
                    continue
                u = read(ROOT / 'game-calls' / (tag + '.response.json'))['raw'].get('usage', {})
                prompt += u.get('prompt_tokens', 0)
                cached += (u.get('prompt_tokens_details') or {}).get('cached_tokens', 0)
                n += 1
            if n:
                out[seed_index] = {'prompt_per_call': prompt / n, 'cached_per_call': cached / n}
        return out

    mid_calls, front_calls = seed_calls('mid'), seed_calls('front')
    common_seeds = sorted(set(mid_calls) & set(front_calls) & set(seeds))
    d_prompt = [front_calls[s]['prompt_per_call'] - mid_calls[s]['prompt_per_call'] for s in common_seeds]
    d_cached = [front_calls[s]['cached_per_call'] - mid_calls[s]['cached_per_call'] for s in common_seeds]

    clusters = []
    for seed_index in common_seeds:
        arms_here = {r['arm']: r for r in completed if r['seed_index'] == seed_index}
        if len(arms_here) != len(ARMS):
            continue
        clusters.append({'seed_index': seed_index, 'payout_mid': arms_here['mid']['payout'], 'payout_front': arms_here['front']['payout'], 'difference': arms_here['front']['payout'] - arms_here['mid']['payout']})
    d_payout = [c['difference'] / 1e6 for c in clusters]

    paired = {
        'prompt_per_call (front - mid)': {'n_seeds': len(d_prompt), 'mean': stats.mean(d_prompt) if d_prompt else None, 'wilcoxon_p': exact_wilcoxon_two_sided(d_prompt)},
        'cached_per_call (front - mid)': {'n_seeds': len(d_cached), 'mean': stats.mean(d_cached) if d_cached else None, 'wilcoxon_p': exact_wilcoxon_two_sided(d_cached)},
        'payout_millions (front - mid)': {'n_seeds': len(d_payout), 'mean': stats.mean(d_payout) if d_payout else None, 'sd': stats.stdev(d_payout) if len(d_payout) > 1 else 0.0, 'wilcoxon_p': exact_wilcoxon_two_sided(d_payout), 'ci95_millions': bootstrap_ci(d_payout) if d_payout else None},
    }

    def flag(r, field):
        value = r.get(field, 0)
        return (len(value) if isinstance(value, list) else value) > 0

    def mcnemar(field):
        from math import comb
        b01 = b10 = both = neither = 0
        for c in clusters:
            arms_here = [r for r in completed if r['seed_index'] == c['seed_index']]
            ea = next(flag(r, field) for r in arms_here if r['arm'] == 'mid')
            eb = next(flag(r, field) for r in arms_here if r['arm'] == 'front')
            if ea and eb:
                both += 1
            elif ea:
                b10 += 1
            elif eb:
                b01 += 1
            else:
                neither += 1
        n, k = b01 + b10, min(b01, b10)
        p = min(1.0, 2 * sum(comb(n, i) for i in range(k + 1)) / 2 ** n) if n else 1.0
        return {'only_front': b01, 'only_mid': b10, 'both': both, 'neither': neither, 'exact_p': p}

    error_tests = {'parse_error_games': mcnemar('parse_errors'), 'illegal_action_games': mcnemar('illegal_actions')}

    out = {
        'planned_games': len(protocol['jobs']),
        'completed_games': len(completed),
        'failed_games': failed,
        'seed_clusters': clusters,
        'usage': usage,
        'tokens': tokens,
        'paired': paired,
        'error_tests': error_tests,
        'primary': protocol['primary'],
        'engine_binary_sha256': protocol['engine_binary_sha256'],
        'engine_source_commit': protocol['engine_source_commit'],
        'study': 'prefix-order-ab',
        'created_utc': __import__('datetime').datetime.now(__import__('datetime').timezone.utc).isoformat(timespec='seconds'),
    }
    tmp = ROOT / 'results.json.tmp'
    tmp.write_text(json.dumps(out, ensure_ascii=False, indent=2, allow_nan=False) + '\n', encoding='utf-8')
    tmp.replace(ROOT / 'results.json')
    tmp = ROOT / 'game-metrics.json.tmp'
    tmp.write_text(json.dumps(game_metrics, ensure_ascii=False, indent=2, allow_nan=False) + '\n', encoding='utf-8')
    tmp.replace(ROOT / 'game-metrics.json')

    print(f"games: {len(completed)} completed, {failed} failed, clusters {len(clusters)}")
    for arm in ARMS:
        t = tokens[arm]
        print(f"{arm:6s} prompt/call {t['prompt_per_call']:7.1f} cached/call {t['cached_per_call']:7.1f} share {t['cached_share']:.3f} completion/call {t['completion_per_call']:6.1f}")
    for label, s in paired.items():
        mean = s['mean']
        extra = f" CI [{s['ci95_millions'][0]:+.2f}, {s['ci95_millions'][1]:+.2f}]" if s.get('ci95_millions') else ''
        print(f"{label}: {mean:+.2f} p={s['wilcoxon_p']:.4f}{extra}")
    return 0

if __name__ == '__main__':
    sys.exit(main())

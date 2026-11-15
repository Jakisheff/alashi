#!/usr/bin/env python3
"""Analyze the agent-context-ab series: arm table, paired tests, error metrics.

Writes results.json + game-metrics.json (compatible with tools/eval_report.py),
prints the headline numbers. Reproduction: python3 analysis.py
"""

import glob
import json
import math
import random
import statistics as stats
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
ARMS = ['naked', 'quickstart', 'skill']
BOOTSTRAP_SEED = 20260907
BOOTSTRAP_REPS = 10000
Z95, Z80 = 1.959964, 0.8416


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


def arm_tokens(arm):
    prompt = completion = calls = 0
    for path in glob.glob(str(ROOT / 'game-calls' / f'*-{arm}-s*.response.json')):
        usage = json.loads(Path(path).read_text())['raw'].get('usage', {})
        prompt += usage.get('prompt_tokens', 0)
        completion += usage.get('completion_tokens', 0)
        calls += 1
    return {'prompt_tokens': prompt, 'completion_tokens': completion, 'calls': calls}


def main():
    results = [read(p) for p in sorted(glob.glob(str(ROOT / 'runs/*/result.json')))]
    completed = [r for r in results if r['status'] == 'completed']
    failed = len(results) - len(completed)
    seeds = sorted({r['seed_index'] for r in completed})
    protocol = read(ROOT / 'protocol.json')

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

    clusters = []
    for seed_index in seeds:
        arms_here = {r['arm']: r for r in completed if r['seed_index'] == seed_index}
        if len(arms_here) != len(ARMS):
            continue
        clusters.append({
            'seed_index': seed_index,
            'seed': arms_here[ARMS[0]]['seed'],
            'opponents': protocol['jobs'][0]['opponents'],
            'complete': True,
            'arm_means': {arm: arms_here[arm]['payout'] for arm in ARMS},
            'difference': arms_here['skill']['payout'] - arms_here['naked']['payout'],
            'difference_quickstart': arms_here['quickstart']['payout'] - arms_here['naked']['payout'],
        })

    table = {}
    for arm in ARMS:
        rows = [m for m in game_metrics.values() if m['arm'] == arm]
        tokens = arm_tokens(arm)
        table[arm] = {
            'games': len(rows),
            'mean_payout': stats.mean(m['payout'] for m in rows),
            'mean_rank': stats.mean(m['rank'] for m in rows),
            'wealth_first_rate': sum(m['wealth_first'] for m in rows) / len(rows),
            'games_with_parse_error': sum(m['parse_errors'] > 0 for m in rows),
            'parse_errors_total': sum(m['parse_errors'] for m in rows),
            'illegal_actions_total': sum(m['illegal_actions'] for m in rows),
            'games_with_illegal': sum(m['illegal_actions'] > 0 for m in rows),
            'tokens_per_game': (tokens['prompt_tokens'] + tokens['completion_tokens']) / max(1, len(rows)),
            'prompt_tokens_per_game': tokens['prompt_tokens'] / max(1, len(rows)),
        }

    diffs = [c['difference'] / 1e6 for c in clusters]
    diffs_q = [c['difference_quickstart'] / 1e6 for c in clusters]
    paired = {}
    for label, values in [('skill - naked', diffs), ('quickstart - naked', diffs_q)]:
        n = len(values)
        mean = stats.mean(values)
        sd = stats.stdev(values) if n > 1 else 0.0
        lo, hi = bootstrap_ci(values)
        paired[label] = {
            'n_seeds': n,
            'mean_millions': mean,
            'sd_millions': sd,
            'wilcoxon_p': exact_wilcoxon_two_sided(values),
            'ci95_millions': [lo, hi],
            'mde80_millions': (Z95 + Z80) * sd / math.sqrt(n) if n else None,
        }

    def mcnemar(field):
        from math import comb
        def flag(r):
            value = r.get(field, 0)
            return (len(value) if isinstance(value, list) else value) > 0
        out = {}
        for B in ['skill', 'quickstart']:
            b01 = b10 = both = neither = 0
            for c in clusters:
                arms_here = [r for r in completed if r['seed_index'] == c['seed_index']]
                ea = next(flag(r) for r in arms_here if r['arm'] == 'naked')
                eb = next(flag(r) for r in arms_here if r['arm'] == B)
                if ea and eb: both += 1
                elif ea: b10 += 1
                elif eb: b01 += 1
                else: neither += 1
            n, k = b01 + b10, min(b01, b10)
            p = min(1.0, 2 * sum(comb(n, i) for i in range(k + 1)) / 2 ** n) if n else 1.0
            out[f'{B} - naked'] = {'only_arm': b01, 'only_naked': b10, 'both': both, 'neither': neither, 'exact_p': p}
        return out

    error_tests = {'parse_error_games': mcnemar('parse_errors'), 'illegal_action_games': mcnemar('illegal_actions')}

    out = {
        'planned_games': len(protocol['jobs']),
        'completed_games': len(completed),
        'failed_games': failed,
        'seed_clusters': clusters,
        'usage': {arm: arm_tokens(arm) for arm in ARMS},
        'contrast': {'label': 'skill - naked', 'arms': ['skill', 'naked']},
        'arm_table': table,
        'paired': paired,
        'error_tests': error_tests,
        'primary': protocol['primary'],
        'engine_binary_sha256': protocol['engine_binary_sha256'],
        'engine_source_commit': protocol['engine_source_commit'],
        'study': 'agent-context-ab',
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
        t = table[arm]
        print(f"{arm:11s} payout {t['mean_payout']/1e6:6.2f}M rank {t['mean_rank']:.2f} "
              f"first {t['wealth_first_rate']:.2f} parseErr {t['games_with_parse_error']}/{t['games']} "
              f"illegal {t['games_with_illegal']}/{t['games']} tok/game {t['tokens_per_game']:.0f}")
    for label, s in paired.items():
        print(f"{label}: {s['mean_millions']:+.2f}M SD {s['sd_millions']:.2f} p={s['wilcoxon_p']:.3f} "
              f"CI [{s['ci95_millions'][0]:+.2f}, {s['ci95_millions'][1]:+.2f}] MDE {s['mde80_millions']:.2f}")
    for metric, tests in error_tests.items():
        for label, s in tests.items():
            print(f"{metric} [{label}]: arm-only {s['only_arm']} naked-only {s['only_naked']} both {s['both']} exact p={s['exact_p']:.4f}")
    return 0


if __name__ == '__main__':
    sys.exit(main())

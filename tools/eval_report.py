#!/usr/bin/env python3
"""Regenerate a study's headline numbers from its raw artifacts in one command.

Any auditor runs:
    python3 tools/eval_report.py [study_dir] [--write out.md]

Sources (all committed raw artifacts of the study):
  results.json        planned/completed games, seed_clusters (paired arm means),
                      per-arm token usage
  game-metrics.json   per-game wealth_first / license_won / action counts
  analysis-reconstruction.json  (optional) recorded audit numbers; the report
                      cross-checks the paired difference against them

Statistics: exact Wilcoxon signed-rank (n <= 16 seeds, sign enumeration),
seeded bootstrap CI (10000 resamples), MDE at 80% power. Pure stdlib,
deterministic (bootstrap_seed default 20260907).

Exit 0 with the report on stdout; exit 1 if the reproduction check fails.
"""

import argparse
import json
import math
import os
import random
import re
import sys

BOOTSTRAP_SEED = 20260907
BOOTSTRAP_REPS = 10000
Z95 = 1.959964
Z80 = 0.8416


def load(path: str):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def exact_wilcoxon_two_sided(diffs: list) -> float:
    """Exact two-sided Wilcoxon signed-rank p via sign enumeration (n <= 16)."""
    vals = [d for d in diffs if d != 0]
    n = len(vals)
    if n == 0:
        return 1.0
    if n > 16:
        # normal approximation with continuity and tie correction
        ranks = _average_ranks([abs(v) for v in vals])
        w_plus = sum(r for r, v in zip(ranks, vals) if v > 0)
        mean = sum(ranks) / 2
        sd = math.sqrt(sum(r * r for r in ranks) / 4)
        z = (w_plus - mean) / sd if sd else 0.0
        return 2 * (1 - 0.5 * (1 + math.erf(abs(z) / math.sqrt(2))))
    ranks = _average_ranks([abs(v) for v in vals])
    total = sum(ranks)
    observed = sum(r for r, v in zip(ranks, vals) if v > 0)
    extreme = observed if observed <= total / 2 else total - observed
    count_le = 0
    for mask in range(1 << n):
        w = 0.0
        for i in range(n):
            if mask & (1 << i):
                w += ranks[i]
        if w <= extreme + 1e-9:
            count_le += 1
    return min(1.0, 2 * count_le / (1 << n))


def _average_ranks(values: list) -> list:
    order = sorted(range(len(values)), key=lambda i: values[i])
    ranks = [0.0] * len(values)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and values[order[j + 1]] == values[order[i]]:
            j += 1
        avg = (i + j) / 2 + 1
        for k in range(i, j + 1):
            ranks[order[k]] = avg
        i = j + 1
    return ranks


def bootstrap_ci(diffs: list, reps: int = BOOTSTRAP_REPS, seed: int = BOOTSTRAP_SEED):
    rng = random.Random(seed)
    n = len(diffs)
    means = sorted(
        sum(rng.choice(diffs) for _ in range(n)) / n for _ in range(reps)
    )
    lo = means[int(0.025 * (reps - 1))]
    hi = means[int(0.975 * (reps - 1))]
    return lo, hi


def arm_table(results: dict, game_metrics: dict) -> list:
    arms = {}
    for key, metric in game_metrics.items():
        match = re.match(r".*-seat\d-([A-Za-z0-9]+)$", key)
        if not match:
            continue
        arm = match.group(1)
        slot = arms.setdefault(arm, {"games": 0, "wealth_first": 0, "license_won": 0})
        slot["games"] += 1
        slot["wealth_first"] += bool(metric.get("wealth_first"))
        slot["license_won"] += bool(metric.get("license_won"))
    usage = results.get("usage", {})
    clusters = results.get("seed_clusters", [])
    rows = []
    for arm in sorted(arms):
        payout = None
        if clusters:
            values = [c["arm_means"][arm] for c in clusters if arm in c.get("arm_means", {})]
            if values:
                payout = sum(values) / len(values)
        tokens = usage.get(arm, {})
        total_tokens = tokens.get("prompt_tokens", 0) + tokens.get("completion_tokens", 0)
        games = arms[arm]["games"]
        rows.append(
            {
                "arm": arm,
                "games": games,
                "mean_payout": payout,
                "wealth_first_rate": arms[arm]["wealth_first"] / games,
                "license_won_rate": arms[arm]["license_won"] / games,
                "tokens_per_game": total_tokens / games if games else None,
            }
        )
    return rows


def main() -> int:
    default_study = os.path.join(
        os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
        "docs/research/harness-learning-20261020",
    )
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("study_dir", nargs="?", default=default_study)
    parser.add_argument("--write", help="also write the markdown report to this path")
    parser.add_argument("--bootstrap-seed", type=int, default=BOOTSTRAP_SEED)
    args = parser.parse_args()

    results = load(os.path.join(args.study_dir, "results.json"))
    game_metrics = load(os.path.join(args.study_dir, "game-metrics.json"))
    reconstruction_path = os.path.join(args.study_dir, "analysis-reconstruction.json")
    reconstruction = load(reconstruction_path) if os.path.isfile(reconstruction_path) else None

    lines = []
    emit = lines.append
    emit(f"# Eval report: {os.path.basename(os.path.normpath(args.study_dir))}")
    emit("")
    emit(f"- games: planned {results.get('planned_games')}, completed "
         f"{results.get('completed_games')}, failed {results.get('failed_games')}")
    emit(f"- bootstrap seed: {args.bootstrap_seed}, reps: {BOOTSTRAP_REPS}")
    emit("")

    emit("| arm | games | mean payout | wealth_first | license_won | tokens/game |")
    emit("|---|---|---|---|---|---|")
    for row in arm_table(results, game_metrics):
        payout = f"{row['mean_payout']/1e6:.2f}M" if row["mean_payout"] is not None else "n/a"
        tokens = f"{row['tokens_per_game']:.0f}" if row["tokens_per_game"] else "n/a"
        emit(f"| {row['arm']} | {row['games']} | {payout} | "
             f"{row['wealth_first_rate']:.2f} | {row['license_won_rate']:.2f} | {tokens} |")
    emit("")

    clusters = results.get("seed_clusters", [])
    diffs = [c["difference"] for c in clusters if c.get("complete")]
    if len(diffs) < 3:
        emit("paired test: not enough complete seed clusters")
    else:
        n = len(diffs)
        mean = sum(diffs) / n
        sd = math.sqrt(sum((d - mean) ** 2 for d in diffs) / (n - 1))
        p_value = exact_wilcoxon_two_sided(diffs)
        ci_lo, ci_hi = bootstrap_ci(diffs, seed=args.bootstrap_seed)
        mde = (Z95 + Z80) * sd / math.sqrt(n)
        arm_pair = [c for c in clusters if c.get("complete")][:1]
        contrast = results.get("contrast", {})
        label = contrast.get("label", "A1 - A0")
        emit(f"## Paired test ({label}, n={n} seeds)")
        emit("")
        emit(f"- mean difference: {mean/1e6:+.2f}M (SD {sd/1e6:.2f}M)")
        emit(f"- exact Wilcoxon signed-rank, two-sided p = {p_value:.3f}")
        emit(f"- bootstrap 95% CI of the mean difference: "
             f"[{ci_lo/1e6:+.2f}M, {ci_hi/1e6:+.2f}M]")
        emit(f"- MDE at 80% power with n={n}: {mde/1e6:.2f}M")
        emit(f"- seeds needed per effect size: 1M->{math.ceil(((Z95+Z80)*sd/1e6)**2)}, "
             f"2M->{math.ceil(((Z95+Z80)*sd/2e6)**2)}, 5M->{math.ceil(((Z95+Z80)*sd/5e6)**2)}")
        if reconstruction:
            recorded = reconstruction.get("difference")
            if recorded is not None:
                ok = abs(recorded - mean) < 1.0
                emit("")
                emit(f"- reproduction check vs analysis-reconstruction.json: "
                     f"recorded {recorded/1e6:+.2f}M vs computed {mean/1e6:+.2f}M -> "
                     f"{'MATCH' if ok else 'MISMATCH'}")
                if not ok:
                    print("\n".join(lines))
                    print("REPRODUCTION CHECK FAILED", file=sys.stderr)
                    return 1

    report = "\n".join(lines) + "\n"
    sys.stdout.write(report)
    if args.write:
        with open(args.write, "w", encoding="utf-8") as handle:
            handle.write(report)
    return 0


if __name__ == "__main__":
    sys.exit(main())

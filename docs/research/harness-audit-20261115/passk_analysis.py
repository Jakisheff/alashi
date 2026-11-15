import argparse
import json
from itertools import combinations
from math import comb
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
DEFAULT_SRC = SCRIPT_DIR.parent / "harness-learning-20261020"
MIN_PAIR = 10


def parse_key(k):
    game, seat, arm = k.rsplit("-", 2)
    return f"{game}:{seat}", arm


def binom_ci(k, n, z=1.96):
    if n == 0:
        return None
    p = k / n
    d = z * (p * (1 - p) / n) ** 0.5
    return [round(max(0.0, p - d), 4), round(min(1.0, p + d), 4)]


def bool_fields(records):
    keys = set.intersection(
        *(set(v.keys()) for v in records.values() if isinstance(v, dict))
    )
    return sorted(
        k
        for k in keys
        if all(isinstance(v.get(k), bool) for v in records.values())
    )


def paired_stats(pairs, field):
    n = len(pairs)
    a_wins = sum(1 for _, g0, _ in pairs if g0[field])
    b_wins = sum(1 for _, _, g1 in pairs if g1[field])
    both = sum(1 for _, g0, g1 in pairs if g0[field] and g1[field])
    either = sum(1 for _, g0, g1 in pairs if g0[field] or g1[field])
    p_hat = (a_wins + b_wins) / (2 * n)
    return {
        "n_pairs": n,
        "arm_a_wins": a_wins,
        "arm_b_wins": b_wins,
        "pass_hat_1": round(p_hat, 4),
        "pass_at_2_empirical": round(either / n, 4),
        "pass_hat_2_empirical": round(both / n, 4),
        "pass_hat_2_ci95": binom_ci(both, n),
        "pass_hat_2_if_independent": round(p_hat**2, 4),
        "pass_hat_3_if_independent": round(p_hat**3, 4),
        "pass_hat_8_if_independent": round(p_hat**8, 4),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--src", type=Path, default=DEFAULT_SRC)
    ap.add_argument("--out", type=Path, default=SCRIPT_DIR / "passk_results.json")
    args = ap.parse_args()

    games = json.loads((args.src / "game-metrics.json").read_text())
    results = json.loads((args.src / "results.json").read_text())

    by_pos = {}
    for k, v in games.items():
        pos, arm = parse_key(k)
        by_pos.setdefault(pos, {})[arm] = v

    arms = sorted({arm for a in by_pos.values() for arm in a})
    fields = bool_fields(games)

    out = {"source": str(args.src), "arms": arms, "bool_fields": fields, "pairs": {}}
    for fa, fb in combinations(arms, 2):
        pairs = [
            (p, a[fa], a[fb]) for p, a in sorted(by_pos.items()) if fa in a and fb in a
        ]
        if len(pairs) < MIN_PAIR:
            continue
        out["pairs"][f"{fa}-{fb}"] = {f: paired_stats(pairs, f) for f in fields}

    usage = results.get("usage")
    if usage:
        out["tokens_per_saved_call"] = {
            arm: {
                "calls": u.get("saved_responses"),
                "input": round(u["prompt_tokens"] / u["saved_responses"], 1),
                "output": round(u["completion_tokens"] / u["saved_responses"], 1),
            }
            for arm, u in usage.items()
            if u.get("saved_responses")
        }

    out["reliability"] = {
        "planned_games": results.get("planned_games"),
        "completed_games": results.get("completed_games"),
        "failed_games": results.get("failed_games"),
    }

    args.out.write_text(json.dumps(out, indent=2, ensure_ascii=False))
    print(json.dumps(out, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()

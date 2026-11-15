import json
from math import comb
from pathlib import Path

BASE = Path(__file__).resolve().parent
SRC = BASE.parent / "harness-learning-20261020"

games = json.loads((SRC / "game-metrics.json").read_text())
results = json.loads((SRC / "results.json").read_text())

def parse_key(k):
    game, seat, arm = k.rsplit("-", 2)
    seed = game.replace("test-", "").replace("dev-", "")
    return f"{seed}:{seat}", arm

by_pos = {}
for k, v in games.items():
    pos, arm = parse_key(k)
    by_pos.setdefault(pos, {})[arm] = v

pairs = [(p, a.get("A0"), a.get("A1")) for p, a in sorted(by_pos.items()) if "A0" in a and "A1" in a]

def won(g, field):
    return bool(g[field]) if g else None

def binom_ci(k, n, z=1.96):
    if n == 0:
        return None
    p = k / n
    d = z * (p * (1 - p) / n) ** 0.5
    return [round(max(0.0, p - d), 4), round(min(1.0, p + d), 4)]

def passk_exact(k_success, n_tasks, k):
    total = comb(n_tasks, k)
    favorable = comb(k_success, k)
    return favorable / total if total else None

out = {"paired_positions": len(pairs), "arms": {}}

for field in ["license_won", "wealth_first"]:
    a0_wins = sum(1 for _, g0, _ in pairs if won(g0, field))
    a1_wins = sum(1 for _, _, g1 in pairs if won(g1, field))
    both = sum(1 for _, g0, g1 in pairs if won(g0, field) and won(g1, field))
    either = sum(1 for _, g0, g1 in pairs if won(g0, field) or won(g1, field))
    p_hat = (a0_wins + a1_wins) / (2 * len(pairs))
    n = len(pairs)
    out["arms"][field] = {
        "n_pairs": n,
        "A0_wins": a0_wins,
        "A1_wins": a1_wins,
        "pass_hat_1": round(p_hat, 4),
        "pass_at_2_empirical": round(either / n, 4),
        "pass_at_2_independent_model": round(1 - (1 - p_hat) ** 2, 4),
        "pass_hat_2_empirical": round(both / n, 4),
        "pass_hat_2_ci95": binom_ci(both, n),
        "pass_hat_2_if_independent": round(p_hat ** 2, 4),
        "pass_hat_3_if_independent": round(p_hat ** 3, 4),
    }
    if field == "license_won":
        out["arms"][field]["pass_hat_8_if_independent"] = round(p_hat ** 8, 4)

decisions_total = results.get("planned_games") and 2422
tok = {
    "A0": {"games": 36, "input": 2190393, "output": 115650},
    "A1": {"games": 36, "input": 2145194, "output": 111861},
    "H": {"games": 36, "input": 2516970, "output": 85317},
}
decisions_per_arm = 2422 * 36 / 108
out["tokens_per_decision"] = {}
for arm, t in tok.items():
    out["tokens_per_decision"][arm] = {
        "input": round(t["input"] / decisions_per_arm, 1),
        "output": round(t["output"] / decisions_per_arm, 1),
    }
a_in = (tok["A0"]["input"] + tok["A1"]["input"]) / 2
a_out = (tok["A0"]["output"] + tok["A1"]["output"]) / 2
out["memory_layer_tax"] = {
    "input_delta_pct": round((tok["H"]["input"] / a_in - 1) * 100, 1),
    "output_delta_pct": round((tok["H"]["output"] / a_out - 1) * 100, 1),
}
out["reliability"] = {
    "planned_games": results.get("planned_games"),
    "completed_games": results.get("completed_games"),
    "failed_games": results.get("failed_games"),
}

(BASE / "passk_results.json").write_text(json.dumps(out, indent=2, ensure_ascii=False))
print(json.dumps(out, indent=2, ensure_ascii=False))

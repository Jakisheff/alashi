#!/usr/bin/env python3
"""Метрики фазы 1 (без LLM) поверх off-chain датасетов alashi (SIM_TEST_PLAN).

1. OpenSkill (Plackett-Luce) по рангам партий — рейтинг стратегий.
2. Rationality Gap недоступен: архив не задаёт независимый оптимум.
   Старый перебор только до выбранного числа товаров был тавтологией.
3. Shapley Fairness — вклад в прошедшие законы (вес голоса = влияние)
   против фактической доли выплаты; exploitation_delta = доля − вклад.

Запуск: python3 tools/sim_metrics.py data/sim/games.jsonl [data/sim/games5.jsonl ...]
Выход: таблица в stdout + data/sim/metrics.json
"""

import itertools
import json
import sys
from collections import defaultdict
from fractions import Fraction

from openskill.models import PlackettLuce

def load(path):
    with open(path) as f:
        return [json.loads(l) for l in f]


# ---------- 1. OpenSkill ----------

def run_openskill(games):
    model = PlackettLuce()
    # rating_id -> индекс в модели; стратегия = сущность (обе копии в 5p вместе)
    ids = sorted({s for g in games for s in g["strategies"]})
    idx = {s: i for i, s in enumerate(ids)}
    ratings = [model.rating(name=s) for s in ids]
    for g in games:
        strats = g["strategies"]
        order = g["ranks"]  # позиции 0..n-1 по убыванию силы
        # порядок для модели: команды в порядке мест, ranks = 0,1,2...
        teams = [[ratings[idx[strats[fi]]]] for fi in order]
        ranks = list(range(len(order)))
        result = model.rate(teams=teams, ranks=ranks)
        for place, team in enumerate(result):
            fi = order[place]
            ratings[idx[strats[fi]]] = team[0]
    out = []
    for s, i in idx.items():
        r = ratings[i]
        out.append({
            "strategy": s,
            "mu": round(float(r.mu), 2),
            "sigma": round(float(r.sigma), 3),
        })
    out.sort(key=lambda x: -x["mu"])
    return out


# ---------- 2. Rationality Gap ----------

def run_rationality(games):
    """No rationality estimate without an independent feasible-action baseline."""
    return [{
        "strategy": strategy,
        "mean_gap": None,
        "status": "unavailable",
        "reason": "No independent action baseline; the old metric optimized only up to the chosen quantity.",
    } for strategy in sorted({s for g in games for s in g["strategies"]})]


# ---------- 3. Shapley Fairness ----------

def pivotal_shapley(weights_yes, weights_no):
    """Shapley-вклад каждого YES-голоса в прохождение закона (yes > no)."""
    yes = [(i, w) for i, w in enumerate(weights_yes)]
    total_no = sum(weights_no)
    n = len(yes)
    sh = {i: Fraction(0) for i, _ in yes}
    if n == 0:
        return sh
    for perm in itertools.permutations(range(n)):
        running = 0
        for pos, i in enumerate(perm):
            w = yes[i][1]
            passes = (running + w) > total_no
            would_pass_without = running > total_no
            if passes and not would_pass_without:
                sh[yes[i][0]] += Fraction(1, 1)
            running += w
    for i in sh:
        sh[i] /= math_factorial(n)
    return sh


def math_factorial(n):
    return 1 if n <= 1 else n * math_factorial(n - 1)


def run_shapley(games):
    """exploitation_delta = фактическая доля выплаты − доля вклада в законы."""
    contrib = defaultdict(Fraction)
    contrib_games = defaultdict(set)
    payout = defaultdict(int)
    law_count = 0
    for gi, g in enumerate(games):
        strats = g["strategies"]
        n = g["n_factions"]
        payouts = g["payouts"]
        bank = g["bank"] - g["rake"]
        for s, p in zip(strats, payouts):
            payout[s] += p
        # трекаем влияние и кэш по ходу (приближение: последнее известное)
        influence = [1] * n
        cash = [0] * n
        alive = [True] * n
        for ph in g["phases"]:
            for a in ph["actions"]:
                if not a["ok"]:
                    continue
                if a["action"] == "bribe":
                    gain = a["detail"].get("influence_after")
                    if gain is not None:
                        influence[a["actor"]] = gain
                if a["cash_after"] is not None:
                    cash[a["actor"]] = a["cash_after"]
            if ph["phase"] == "law" and ph.get("law_passed"):
                law_count += 1
                yes_w, no_w, yes_i, voted = [], [], [], set()
                for a in ph["actions"]:
                    if a["action"] == "vote_yes":
                        yes_w.append(influence[a["actor"]])
                        yes_i.append(a["actor"])
                        voted.add(a["actor"])
                    elif a["action"] == "vote_no":
                        no_w.append(influence[a["actor"]])
                        voted.add(a["actor"])
                sh = pivotal_shapley(yes_w, no_w)
                for k, i in enumerate(yes_i):
                    contrib[strats[i]] += sh[k]
                    contrib_games[strats[i]].add((gi, law_count))
    # нормируем: сумма Shapley по каждому закону = 1, если закон прошёл
    # R3 (REVIEW_EXTERNAL): оба знаменателя — суммарный банк всех партий;
    # прежний bank_total[s] делил выплату на число мест (две копии стратегии
    # считались за одну) и сумма дельт не сходилась в 0.
    payout_total = sum(payout.values())
    total = sum(contrib.values()) or 1
    out = []
    deltas = []
    for s in sorted({s for g in games for s in g["strategies"]}):
        share = payout[s] / payout_total if payout_total else 0
        cshare = float(contrib[s] / total) if total else 0.0
        deltas.append(share - cshare)
        out.append({
            "strategy": s,
            "payout_share": round(share, 4),
            "shapley_share": round(cshare, 4),
            "exploitation_delta": round(share - cshare, 4),
            "raw_shapley": round(float(contrib[s]), 3),
        })
    # закон сохранения (L3 LESSONS_LEARNED): сумма дельт обязана быть 0
    assert abs(sum(deltas)) < 1e-9, f"shapley deltas sum != 0: {sum(deltas)}"
    return out, law_count


def main():
    paths = sys.argv[1:] or ["data/sim/games.jsonl"]
    all_metrics = {}
    for path in paths:
        games = load(path)
        key = path.split("/")[-1].replace(".jsonl", "")
        ratings = run_openskill(games)
        rat = run_rationality(games)
        shap, laws = run_shapley(games)
        all_metrics[key] = {
            "games": len(games),
            "openskill": ratings,
            "rationality_gap": rat,
            "shapley": shap,
            "laws_passed": laws,
        }
        print(f"\n=== {path} ({len(games)} партий, законов прошло {laws}) ===")
        print("OpenSkill (Plackett-Luce), μ±σ:")
        for r in ratings:
            print(f"  {r['strategy']:>8}: {r['mu']:7.2f} ± {r['sigma']}")
        print("Rationality Gap (Market, продажи):")
        for r in rat:
            print(f"  {r['strategy']:>8}: unavailable — {r['reason']}")
        print("Shapley Fairness (доля выплаты vs вклад в законы):")
        for r in shap:
            print(
                f"  {r['strategy']:>8}: выплата {r['payout_share']:.3f}  "
                f"вклад {r['shapley_share']:.3f}  exploitation {r['exploitation_delta']:+.3f}"
            )
    with open("data/sim/metrics.json", "w") as f:
        json.dump(all_metrics, f, ensure_ascii=False, indent=1)
    print("\nзаписано: data/sim/metrics.json")


if __name__ == "__main__":
    main()

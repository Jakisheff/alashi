#!/usr/bin/env python3
"""Политическая карта агентов (по Баладжи, «вычислительная нация»):
близость фракций по совместным голосованиям и сделкам на data/sim/*.jsonl.

Метрика: для каждой пары стратегий (a,b) считаем долю law-фаз, где обе
голосовали и голоса совпали, минус базовая частота совпадений случайной
пары (нормировка). Плюс экономическая близость: сколько раз a продавал
непосредственно перед/после b в одном раунде (очередь рынка как
координация). Вывод: матрица и блоки агломеративной кластеризацией."""
import json, glob, os, sys
from collections import defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def load(path):
    return [json.loads(l) for l in open(path)]

def analyse(paths):
    votes_together = defaultdict(lambda: [0, 0])   # (a,b) -> [совпало, всего]
    votes_all = defaultdict(int)                   # голосов вообще
    sells_order = defaultdict(int)                 # продавали в одном раунде рядом
    for path in paths:
        for g in load(path):
            strats = g["strategies"]
            for ph in g["phases"]:
                if ph["phase"] == "law":
                    vv = {}
                    for a in ph["actions"]:
                        if not a["ok"]:
                            continue
                        act = a["action"]
                        if act in ("vote_yes", "vote_no", "vote_abstain"):
                            vv[a["actor"]] = act
                    names = sorted(vv)
                    for i in names:
                        votes_all[strats[i]] += 1
                        for j in names:
                            if i < j:
                                key = (strats[i], strats[j])
                                votes_together[key][1] += 1
                                if vv[i] == vv[j]:
                                    votes_together[key][0] += 1
                if ph["phase"] == "market":
                    sellers = [a["actor"] for a in ph["actions"]
                               if a["ok"] and a["action"] == "sell"]
                    for x in range(len(sellers) - 1):
                        a, b = sellers[x], sellers[x + 1]
                        if a != b:
                            key = tuple(sorted((strats[a], strats[b])))
                            sells_order[key] += 1
    return votes_together, votes_all, sells_order

def blocks(names, sim):
    """Простая агломерация: merge ближайших, пока sim > 0."""
    clusters = [[n] for n in names]
    def csim(c1, c2):
        return max(sim.get((a, b), 0.0) for a in c1 for b in c2)
    merged = True
    while merged and len(clusters) > 1:
        merged = False
        best = None
        for i in range(len(clusters)):
            for j in range(i + 1, len(clusters)):
                s = csim(clusters[i], clusters[j])
                if s > 0 and (best is None or s > best[0]):
                    best = (s, i, j)
        if best:
            _, i, j = best
            clusters[i] = clusters[i] + clusters[j]
            clusters.pop(j)
            merged = True
    return clusters

def main():
    paths = sorted(glob.glob(os.path.join(ROOT, "data/sim/games*.jsonl")))
    paths = [p for p in paths if "contrib" not in p]  # классические серии
    vt, va, so = analyse(paths)
    names = sorted(va)
    sim = {}
    for (a, b), (same, total) in vt.items():
        if total >= 20:
            sim[(a, b)] = round(same / total - 0.34, 3)  # 0.34 ~ база случайных
    print("=== Политическая близость (совпадение голосов, минус база) ===")
    for (a, b), s in sorted(sim.items(), key=lambda x: -x[1]):
        tot = vt[(a, b)][1]
        print(f"  {a:>8} × {b:<8}: {s:+.3f}  (n={tot})")
    print("\n=== Экономическая близость (продажи подряд в раунде) ===")
    for (a, b), n in sorted(so.items(), key=lambda x: -x[1])[:6]:
        print(f"  {a:>8} × {b:<8}: {n} раз")
    cl = blocks(names, sim)
    print("\n=== Блоки голосования ===")
    for c in cl:
        print("  блок:", " + ".join(c) if len(c) > 1 else f"{c[0]} (одиночка)")

if __name__ == "__main__":
    main()

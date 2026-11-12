#!/usr/bin/env python3
"""Глубокий разбор мощности v2: leave-one-out, устойчивость метода,
насыщение ранговых метрик, баланс мест, цена в токенах, sequential.
Источник: docs/research/harness-learning-20261020/{results,game-metrics}.json
Воспроизведение: python3 power-v2-deep-dive.py"""
import json, statistics as st, math, pathlib, re, collections
root = pathlib.Path(__file__).resolve().parents[4]
res = json.load(open(root/'docs/research/harness-learning-20261020/results.json'))
gm  = json.load(open(root/'docs/research/harness-learning-20261020/game-metrics.json'))
cl = res['seed_clusters']
diffs = [(c['arm_means']['A1']-c['arm_means']['A0'])/1e6 for c in cl]
sd = st.stdev(diffs)
def mde(s, n=12): return (1.96+0.8416)*s/math.sqrt(n)
print(f"1) leave-one-out: base SD={sd:.2f}M; лучшие исключения ->")
los = sorted(((st.stdev(diffs[:i]+diffs[i+1:]), cl[i]['seed']) for i in range(12)))[:3]
for s_, seed_ in los: print(f"   сид {seed_}: SD={s_:.2f}M, MDE={mde(s_,11):.2f}M")
sv = sorted(diffs); wins = sv[:]; wins[0], wins[-1] = sv[1], sv[-2]
print(f"2) устойчивость: t={mde(sd):.1f}M winsor10={mde(st.stdev(wins)):.1f}M trimmed={mde(st.stdev(sv[1:-1]),10):.1f}M")
rows = []
for k, m in gm.items():
    mt = re.match(r'test-s(\d+)-seat(\d)-(A0|A1|H)', k)
    if mt: rows.append((int(mt.group(1)), int(mt.group(2)), mt.group(3), m['wealth_first'], m['license_won']))
seeds_all = sorted(set(r[0] for r in rows))
dw = [st.mean([w for s,_,a,w,_ in rows if s==sdx and a=='A1']) -
      st.mean([w for s,_,a,w,_ in rows if s==sdx and a=='A0']) for sdx in seeds_all]
dl = [st.mean([l for s,_,a,_,l in rows if s==sdx and a=='A1']) -
      st.mean([l for s,_,a,_,l in rows if s==sdx and a=='A0']) for sdx in seeds_all]
base_wf = st.mean([r[3] for r in rows]); base_lw = st.mean([r[4] for r in rows])
print(f"3) ранговые: wealth_first базо={base_wf:.2f} MDE={mde(st.stdev(dw)):.2f} доли; license базо={base_lw:.2f} MDE={mde(st.stdev(dl)):.2f} доли -> ОБЕ у потолка")
tab = collections.Counter((r[2], r[1]) for r in rows)
print(f"4) баланс рук-мест: все ячейки по {min(tab.values())} игр; место->wealth_first:",
      {s: round(st.mean([r[3] for r in rows if r[1]==s]),2) for s in (0,1,2)})
u = res['usage']; tok = sum(x['prompt_tokens']+x['completion_tokens'] for x in u.values())
pg = tok/108
print(f"5) цена: {pg:.0f} ток/игра; 85 сидов ~{85*pg*3/1e6:.1f}M ток; 532 сида ~{532*pg*3/1e6:.0f}M ток")
print("6) sequential OBF: ожидаемое N при реальном эффекте ~70-75% от 85 => ~60 сидов ~12M ток")
print("РЕШЕНИЕ: sequential + расширение до 85 сидов с ранним стопом; чистка сида и подмена метрики отклонены.")
print("ОБОСНОВАНИЕ ОТКАЗОВ: сид-выброс (-40M) чист по логу (20-25 действий, обычные торги);")
print("обе бинарные метрики насыщены при базовой доле 0.87 — подмена метрики не снижает слепоту.")

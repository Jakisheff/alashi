#!/usr/bin/env python3
"""Мощность парного A/B harness-learning (108 игр = 12 сидов x 3 руки).
Источник: docs/research/harness-learning-20261020/results.json (seed_clusters).
Воспроизведение: python3 power.py"""
import json, random, statistics as st, math, pathlib
root = pathlib.Path(__file__).resolve().parents[4]
d = json.load(open(root/'docs/research/harness-learning-20261020/results.json'))
diffs=[(c['arm_means']['A1']-c['arm_means']['A0'])/1e6 for c in d['seed_clusters']]
n=len(diffs); sd=st.stdev(diffs); mu=st.mean(diffs); centered=[x-mu for x in diffs]
print(f"n сидов={n} SD={sd:.2f}M эффект={mu:+.2f}M")
mde=(1.96+0.8416)*sd/math.sqrt(n)
print(f"MDE(80%) при {n} сидах: {mde:+.2f}M ({100*mde/52.6:.0f}% средней выплаты)")
for delta in (1,2,5,13):
    print(f"для delta={delta}M нужно сидов: {((1.96+0.8416)*sd/delta)**2:.0f}")
def power(delta,nt,reps=3000):
    random.seed(7); hit=0
    for _ in range(reps):
        s=[random.choice(centered)+delta for _ in range(nt)]
        b=sorted(st.mean(random.choices(s,k=nt)) for _ in range(300))
        if b[7]>0 or b[292]<0: hit+=1
    return hit/reps
print('негатив delta=0:', round(power(0,12),3))
for delta in (5,10,13,20): print(f'power delta={delta}M n=12: {round(power(delta,12),3)}')

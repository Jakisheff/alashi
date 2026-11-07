#!/usr/bin/env python3
"""Independently reconstruct the primary contrast using exact integer arithmetic."""
import itertools,json,math,random,statistics
from pathlib import Path
import common as c
ROOT=Path(__file__).resolve().parent
def read(p):return json.loads(p.read_text())
def main():
 protocol=read(ROOT/'protocol.json');reported=read(ROOT/'results.json');groups=[];numerators=[]
 for i in range(len(protocol['test_seeds'])):
  paths=[[ROOT/'runs/test'/f'test-s{i+1}-seat{seat}-{arm}'/'game.json' for seat in range(3)] for arm in ['A0','A1','H']]
  if not all(p.exists() for row in paths for p in row):continue
  sums=[sum(read(path)['payouts'][seat] for seat,path in enumerate(row)) for row in paths]
  groups.append(sums);numerators.append(2*sums[2]-sums[0]-sums[1])
 k=len(groups);observed=sum(numerators);contrast=observed/(6*k)
 alternatives=[[3*x-sum(row) for x in row] for row in groups]
 count=0
 for values in itertools.product(*alternatives):count+=abs(sum(values))>=abs(observed)
 pvalue=count/(3**k);assert abs(reported['primary']['difference']-contrast)<1e-6 and abs(reported['primary']['p_two_sided']-pvalue)<1e-12
 rng=random.Random(2026102003);boot=sorted(sum(rng.choice(numerators) for _ in range(k))/(6*k) for _ in range(10000))
 interval=[]
 for q in [.025,.975]:
  pos=(len(boot)-1)*q;lo=math.floor(pos);hi=math.ceil(pos);interval.append(boot[lo]*(hi-pos)+boot[hi]*(pos-lo) if lo!=hi else boot[lo])
 assert all(abs(a-b)<1e-6 for a,b in zip(interval,reported['primary']['ci95']))
 for arm in ['A0','A1','H']:
  vals=[read(ROOT/'runs/test'/j['id']/'game.json')['payouts'][j['seat']] for j in protocol['test_jobs'] if j['arm']==arm and (ROOT/'runs/test'/j['id']/'game.json').exists()]
  assert abs(statistics.mean(vals)-reported['groups'][arm]['mean_payout'])<1e-6
 c.write_json(ROOT/'analysis-reconstruction.json',{'independent_raw_game_payouts':True,'integer_permutation_test':True,'seed_clusters':k,'observed_integer_numerator':observed,'scale':6*k,'difference':contrast,'exact_p_two_sided':pvalue,'assignments':3**k,'extreme_assignments':count,'ci95':interval,'matches_reported_results':True})
 print('Raw-payout integer contrast, exact p and bootstrap interval match reported results')
if __name__=='__main__':main()

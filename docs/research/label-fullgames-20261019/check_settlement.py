#!/usr/bin/env python3
"""Independent payout arithmetic given recorded final assets and license rent."""
import argparse,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent

def check(folder):
 game=json.loads((folder/'game.json').read_text());trace=[json.loads(x) for x in (folder/'trace.jsonl').read_text().splitlines()]
 assert game['epoch']==1 and game['n_factions']==3
 assert all(p['hard']==0 for step in trace for p in step['state']['participants']), 'This arithmetic assumes the frozen interface without currency exchange'
 tie_rank={idx:rank for rank,idx in enumerate(trace[0]['state']['tie_order'])}
 cash=game['final_cash'];influence=game['final_influence']
 order=sorted(range(3),key=lambda i:(-cash[i],tie_rank[i]));assert order==game['ranks']
 license=game['license'];bank=game['entry_fee']*3+(license['bid'] if license else 0);assert bank==game['bank']
 rake=bank*5//100;distributable=bank-rake
 place2=distributable*30//95;place3=distributable*15//95;by_place=[distributable-place2-place3,place2,place3]
 expected=[0,0,0]
 for place,actor in enumerate(order):expected[actor]=by_place[place]
 factory=min(range(3),key=lambda i:(-influence[i],-cash[i],tie_rank[i]));bonus=bank*5//100
 expected[factory]+=bonus;rake-=bonus
 if license:expected[license['holder_idx']]+=license['rent']
 assert expected==game['payouts'],(folder.name,expected,game['payouts'])
 assert rake==game['rake'] and sum(expected)+rake==bank+(license['rent'] if license else 0)
 return {'id':folder.name,'payouts_match':True,'ranks_match':True,'recorded_license_rent_used':True}
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--pilot',action='store_true');args=parser.parse_args()
 if args.pilot:folders=[ROOT/'pilot-fullgame/cases/pilot']
 else:
  protocol=json.loads((ROOT/'protocol.json').read_text());folders=[]
  for job in protocol['jobs']:
   folder=ROOT/'cases'/job['id'];assert (folder/'result.json').exists(),'Collection incomplete'
   if json.loads((folder/'result.json').read_text())['status']=='completed':folders.append(folder)
 checks=[check(f) for f in folders]
 name='pilot-settlement-check.json' if args.pilot else 'settlement-validation.json'
 (ROOT/name).write_text(json.dumps({'games_checked':len(checks),'checks':checks,'scope':'Independent terminal allocation arithmetic; rent is the recorded exogenous game component.'},indent=2)+'\n')
 print('Independent payout arithmetic passed for',len(checks),'games')
if __name__=='__main__':main()

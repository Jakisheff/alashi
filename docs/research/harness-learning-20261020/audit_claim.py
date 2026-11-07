#!/usr/bin/env python3
"""Descriptive audits of model statements, without feedback to the learner."""
import collections,json,statistics
from pathlib import Path
import common as c
ROOT=Path(__file__).resolve().parent

def main():
 training=json.loads((ROOT/'training.json').read_text());rivals=[];rent=[];conditional={True:[],False:[]};selected={}
 for game in training:
  subject=game['subject_idx'];license=game['outcome']['license'];rent.append(license['rent']);won=license['holder_idx']==subject;conditional[won].append(game['outcome']['payouts'][subject])
  bids=[{'actor':a['actor'],'amount':a['detail']['amount']} for phase in game['phase_actions'] if phase['round']==4 and phase['phase']=='action' for a in phase['actions'] if a['action']=='bid_license']
  rivals.extend(x['amount'] for x in bids if x['actor']!=subject)
  if game['id'] in ['g06','g08','g13','g00','g32']:selected[game['id']]={'subject':subject,'bids':bids,'license':license,'subject_payout':game['outcome']['payouts'][subject]}
 out={'training':{'games':len(training),'distinct_seed_clusters':8,'opponent_bids':len(rivals),'maximum_opponent_bid':max(rivals),'opponent_bid_counts':dict(collections.Counter(rivals)),'minimum_license_rent':min(rent),'maximum_license_rent':max(rent),'subject_license_wins':len(conditional[True]),'mean_subject_payout_if_won':statistics.mean(conditional[True]),'mean_subject_payout_if_not_won':statistics.mean(conditional[False]),'cited_cases_observed_records':selected},'heldout':{}}
 for arm in ['A0','A1','H']:
  rows=[];recommended=0;nonbid=0;overcap=0;licensewins=0
  for job in json.loads((ROOT/'protocol.json').read_text())['test_jobs']:
   if job['arm']!=arm:continue
   folder=ROOT/'runs/test'/job['id'];result=json.loads((folder/'result.json').read_text())
   if result['status']!='completed':continue
   game=json.loads((folder/'game.json').read_text());trace=[json.loads(s) for s in (folder/'trace.jsonl').read_text().splitlines()];rival=[a['detail']['amount'] for phase in game['phases'] if phase['round']==4 and phase['phase']=='action' for a in phase['actions'] if a['action']=='bid_license' and a['actor']!=job['seat']];overcap+=any(x>8000000 for x in rival);won=bool(game['license'] and game['license']['holder_idx']==job['seat']);licensewins+=won
   for t in trace:
    auction=t['features'].get('auction',{});choice=auction.get('recommended_choice')
    if choice is not None:
     recommended+=1;candidate=t['state']['candidates'][choice];nonbid+=candidate['action']!='bid'
   rows.append({'id':job['id'],'opponents':job['opponents'],'maximum_opponent_bid':max(rival,default=0),'opponent_cap_8_exceeded':any(x>8000000 for x in rival),'subject_license_won':won,'own_bids':[t['action']['amount'] for t in trace if t['action']['action']=='bid']})
  out['heldout'][arm]={'games':len(rows),'games_with_rival_bid_above_8':overcap,'license_wins':licensewins,'helper_recommendations':recommended,'helper_recommendations_pointing_to_nonbid':nonbid,'per_game':rows}
 c.write_json(ROOT/'claim-audit.json',out);print(json.dumps({k:v for k,v in out['training'].items() if k!='cited_cases_observed_records'},indent=2));print({a:{k:v for k,v in x.items() if k!='per_game'} for a,x in out['heldout'].items()})
if __name__=='__main__':main()

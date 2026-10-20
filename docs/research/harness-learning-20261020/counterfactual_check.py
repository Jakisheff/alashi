#!/usr/bin/env python3
"""Fixed-action counterfactual on an old game; never fed to the discovery model."""
import json,subprocess
from pathlib import Path
import common as c
ROOT=Path(__file__).resolve().parent
OLD=ROOT.parent/'alashi-label-fullgames-20261019'

def run(extra):
 folder=OLD/'cases/s1-seat0-A0';job=json.loads((folder/'job.json').read_text());trace=[json.loads(s) for s in (folder/'trace.jsonl').read_text().splitlines()];proc=subprocess.Popen([str(ROOT/'bin/evalgame')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,bufsize=1);index=0;changes=[]
 try:
  proc.stdin.write(json.dumps({'seed':job['seed'],'seat':job['seat'],'opponents':['greedy','tactical'],'builtin':None,'epoch':1,'vote_weight_mode':1})+'\n');proc.stdin.flush()
  while True:
   msg=json.loads(proc.stdout.readline())
   if msg['type']=='request':
    state=c.observation(msg['observation'],msg['phase']);original=trace[index];assert (state['round'],state['phase'],state['stage'])==(original['state']['round'],original['state']['phase'],original['state']['stage']);action=dict(original['action'])
    if action['action']=='bid' and state['round']==4:
     action['amount']+=extra
     if extra:changes.append({'index':index,'before':original['action'],'after':action})
    assert action in [{k:v for k,v in x.items() if k not in ['choice','after_own_action']} for x in state['candidates']],'Recorded command no longer legal'
    proc.stdin.write(json.dumps(action)+'\n');proc.stdin.flush();index+=1
   elif msg['type']=='decision':assert msg['parse_error'] is None
   elif msg['type']=='result':game=msg['game'];break
  proc.wait(timeout=10);assert proc.returncode==0 and index==len(trace)
 finally:
  if proc.poll() is None:proc.kill();proc.wait(timeout=10)
  proc.stdin.close();proc.stdout.close();proc.stderr.close()
 assert all(a['ok'] for phase in game['phases'] for a in phase['actions'] if a['actor']==job['seat'])
 return game,changes

def main():
 assert (ROOT/'candidate-final/claim.json').exists(),'Only after the final learner submission'
 original,changes=run(0);assert not changes and original==json.loads((OLD/'cases/s1-seat0-A0/game.json').read_text())
 modified,changes=run(1000000);assert len(changes)==1;out={'old_case':'s1-seat0-A0','subject':0,'change':changes[0],'same_seed_and_recorded_subject_actions':True,'opponents':'Original reactive greedy/tactical','original_payout':original['payouts'][0],'modified_payout':modified['payouts'][0],'difference':modified['payouts'][0]-original['payouts'][0],'original_ranks':original['ranks'],'modified_ranks':modified['ranks'],'original_bank':original['bank'],'modified_bank':modified['bank'],'license_holder_unchanged':original['license']['holder_idx']==modified['license']['holder_idx'],'license_rent_unchanged':original['license']['rent']==modified['license']['rent'],'scope':'One fixed recorded subject action sequence; not an estimate of a model policy or of average effect. The final learner never receives this check.'}
 c.write_json(ROOT/'counterfactual-check.json',out);print(json.dumps(out,ensure_ascii=False,indent=2))
if __name__=='__main__':main()

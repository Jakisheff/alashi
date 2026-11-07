#!/usr/bin/env python3
"""Run the frozen replay validator as games finish; do not inspect aggregate payoffs."""
import json,time
from pathlib import Path
import common as c
import validate_games as v
ROOT=Path(__file__).resolve().parent

def read(p):return json.loads(p.read_text())
def main():
 freeze=read(ROOT/'harness-freeze.json')
 for name,h in freeze['hashes'].items():assert c.sha(ROOT/name)==h,name
 protocol=read(ROOT/'protocol.json');jobs=protocol['test_jobs'];path=ROOT/'test-incremental-validation.json'
 saved=read(path) if path.exists() else {'checks':{},'fingerprints':{}}
 while True:
  for job in jobs:
   key=job['id'];folder=ROOT/'runs/test'/key
   if not (folder/'result.json').exists():continue
   if key in saved['checks']:
    for name,h in saved['fingerprints'][key].items():assert c.sha(folder/name)==h,(key,name)
    continue
   check=v.validate_job(job,'test');saved['checks'][key]=check
   saved['fingerprints'][key]={n:c.sha(folder/n) for n in ['result.json','trace.jsonl','game.json'] if (folder/n).exists()}
   saved.update(utc=c.utc(),network_calls=0,validator_sha256=c.sha(ROOT/'validate_games.py'),completed_checked=sum(x['status']=='completed' for x in saved['checks'].values()),decisions=sum(x.get('decisions',0) for x in saved['checks'].values()))
   c.write_json(path,saved);print('Verified',saved['completed_checked'],'/',len(jobs),'heldout games',flush=True)
  if len(saved['checks'])==len(jobs):break
  time.sleep(10)
 checks=[saved['checks'][j['id']] for j in jobs]
 c.write_json(ROOT/'test-validation.json',{'checks':checks,'network_calls':0,'completed_checked':saved['completed_checked'],'decisions':saved['decisions'],'incremental':True,'validator_sha256':saved['validator_sha256'],'all_record_fingerprints_unchanged':True})
 print('All scheduled heldout results verified without new model calls',flush=True)
if __name__=='__main__':main()

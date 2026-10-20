#!/usr/bin/env python3
"""Deterministically replay recorded choices. No provider calls or credentials."""
import argparse,hashlib,importlib.util,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('collector',ROOT/'run_experiment.py');collector=importlib.util.module_from_spec(spec);spec.loader.exec_module(collector)
def replay(folder):
 job=json.loads((folder/'job.json').read_text());expected=json.loads((folder/'game.json').read_text());trace=[json.loads(line) for line in (folder/'trace.jsonl').read_text().splitlines()]
 config={'seed':job['seed'],'seat':job['seat'],'opponents':['greedy','tactical'],'builtin':None,'epoch':1,'vote_weight_mode':1}
 proc=subprocess.Popen([str(ROOT/'bin/evalgame')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,bufsize=1)
 index=0;history=[];requests_verified=0
 try:
  proc.stdin.write(json.dumps(config)+'\n');proc.stdin.flush()
  while True:
   line=proc.stdout.readline();assert line,'Replay closed before result';msg=json.loads(line)
   if msg['type']=='request':
    state=collector.observation(msg['observation'],msg['phase']);saved=trace[index];assert state==saved['state'],(job['id'],index,'state')
    choice=state['candidates'][saved['choice']];chosen={k:v for k,v in choice.items() if k not in ('choice','after_own_action')};assert chosen==saved['action']
    role,entity=collector.LABELS[job['arm']];messages=collector.render(state,history,role,entity)
    attempts=sorted(folder.glob(f'call-{index:02d}-*.request.json'));assert len(attempts)<=3
    if saved['source']=='forced':assert len(state['candidates'])==1 and not attempts
    else:
     matching=[]
     for path in attempts:
      request=json.loads(path.read_text());actual=request['body']['messages'];assert actual[:2]==messages
      assert len(actual)<=3
      if len(actual)==3:assert actual[-1]['role']=='user' and actual[-1]['content'].startswith('Ошибка формата')
      assert str(job['seed']) not in json.dumps(actual,ensure_ascii=False)
      if job['arm']=='N':assert 'президент' not in json.dumps(actual,ensure_ascii=False).lower() and 'фракци' not in json.dumps(actual,ensure_ascii=False).lower()
      if request['serial']==saved['request_serial']:matching.append(path)
      requests_verified+=1
     assert len(matching)==1
     raw=json.loads(matching[0].with_name(matching[0].name.replace('.request.json','.response.json')).read_text())['raw']
     parsed=collector.parse(raw,len(state['candidates']));assert parsed['choice']==saved['choice']
    history.append({'round':state['round'],'phase':state['phase'],'stage':state['stage'],'seen':[[p['cash'],p['goods'],p['influence'],p['promissory']] for p in state['participants']],'holder':state['role']['holder_idx'],'law_id':state['law_id'],'action':chosen})
    proc.stdin.write(json.dumps(chosen)+'\n');proc.stdin.flush();index+=1
   elif msg['type']=='decision':assert msg['parse_error'] is None
   elif msg['type']=='result':assert msg['game']==expected,job['id'];break
  proc.wait(timeout=10);assert proc.returncode==0 and index==len(trace)
 finally:
  if proc.poll() is None:proc.kill();proc.wait(timeout=10)
 return {'id':job['id'],'decisions_replayed':index,'requests_verified':requests_verified,'exact_game_record_equal':True}
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--pilot',action='store_true');parser.add_argument('--allow-rebuilt-binary',action='store_true');args=parser.parse_args();binary_sha=hashlib.sha256((ROOT/'bin/evalgame').read_bytes()).hexdigest();expected_sha=json.loads((ROOT/'protocol.json').read_text())['runtime_binary_sha256'];binary_match=binary_sha==expected_sha
 if args.pilot:folders=[ROOT/'pilot-fullgame/cases/pilot']
 else:
  p=json.loads((ROOT/'protocol.json').read_text());assert binary_match or args.allow_rebuilt_binary,'Rebuilt binary differs; explicitly use --allow-rebuilt-binary for source-verified semantic replay'
  for name,h in p['hashes'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==h,name
  folders=[]
  for job in p['jobs']:
   folder=ROOT/'cases'/job['id'];assert (folder/'result.json').exists(),'Collection incomplete'
   if json.loads((folder/'result.json').read_text())['status']=='completed':folders.append(folder)
 checks=[replay(f) for f in folders]
 out=ROOT/('pilot-replay.json' if args.pilot else 'replay-validation.json');out.write_text(json.dumps({'games_verified':len(checks),'checks':checks,'network_calls':0,'binary_sha256':binary_sha,'original_binary_match':binary_match,'rebuilt_binary_explicitly_allowed':args.allow_rebuilt_binary},indent=2)+'\n')
 print('Exact replay passed:',len(checks),'games;',sum(c['decisions_replayed'] for c in checks),'decisions;',sum(c['requests_verified'] for c in checks),'request bindings; no model calls')
if __name__=='__main__':main()

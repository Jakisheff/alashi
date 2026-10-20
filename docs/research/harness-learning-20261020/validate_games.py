#!/usr/bin/env python3
"""Exact command/state/harness replay and independently derived terminal payouts."""
import json,subprocess,sys
from pathlib import Path
import common as c
import collect
from settlement_check import check as check_settlement
ROOT=Path(__file__).resolve().parent

def read(p):return json.loads(p.read_text())
def validate_job(job,stage):
 folder=ROOT/'runs'/stage/job['id'];result=read(folder/'result.json')
 if result['status']!='completed':return {'id':job['id'],'status':result['status'],'error':result.get('error')}
 expected=read(folder/'game.json');trace=[json.loads(s) for s in (folder/'trace.jsonl').read_text().splitlines()];harness=collect.harness_for(stage,job['arm']);assert result['harness']=={n:c.sha(harness/n) for n in ['memory.md','helper.py']}
 proc=subprocess.Popen([str(ROOT/'bin/evalgame')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,bufsize=1);index=0;history=[];requests=0
 try:
  proc.stdin.write(json.dumps({'seed':job['seed'],'seat':job['seat'],'opponents':job['opponents'],'builtin':None,'epoch':1,'vote_weight_mode':1})+'\n');proc.stdin.flush()
  while True:
   line=proc.stdout.readline();assert line,'Engine closed early';msg=json.loads(line)
   if msg['type']=='request':
    row=trace[index];state=c.observation(msg['observation'],msg['phase']);assert state==row['state'] and row['index']==index
    messages,features=collect.render(state,history,harness);assert features==row['features'] and collect.content_hash(messages)==row['messages_sha256']
    candidates=state['candidates'];action={k:v for k,v in candidates[row['choice']].items() if k not in ['choice','after_own_action']};assert action==row['action']
    attempts=sorted((ROOT/'game-calls').glob(f"{job['id']}-s{index:02d}-a*.request.json"));assert len(attempts)<=3;first_valid=None
    for path in attempts:
     request=read(path);actual=request['body']['messages'];assert actual[:2]==messages and len(actual)<=3
     if len(actual)==3:assert actual[2]['content'].startswith('Ошибка формата')
     assert str(job['seed']) not in json.dumps(actual,ensure_ascii=False),'Hidden seed entered model context';requests+=1
     response=path.with_name(path.name.replace('.request.json','.response.json'))
     if response.exists() and first_valid is None:
      try:value=c.parse(read(response)['raw'],len(candidates));first_valid=(request['serial'],value['choice'])
      except (AssertionError,ValueError,KeyError,TypeError):pass
    if row['source']=='forced':assert len(candidates)==1 and not attempts
    else:assert first_valid==(row['request_serial'],row['choice'])
    history.append(collect.history_row(state,action));proc.stdin.write(json.dumps(action)+'\n');proc.stdin.flush();index+=1
   elif msg['type']=='decision':assert msg['parse_error'] is None
   elif msg['type']=='result':assert expected==msg['game'];break
  proc.wait(timeout=10);assert proc.returncode==0 and index==len(trace)
 finally:
  if proc.poll() is None:proc.kill();proc.wait(timeout=10)
  proc.stdin.close();proc.stdout.close();proc.stderr.close()
 check_settlement(folder)
 return {'id':job['id'],'status':'completed','exact_replay':True,'independent_settlement':True,'first_valid_choices_verified':True,'decisions':index,'requests':requests}
def main(stage):
 protocol=read(ROOT/'protocol.json');checks=[validate_job(j,stage) for j in protocol[stage+'_jobs']];c.write_json(ROOT/(stage+'-validation.json'),{'checks':checks,'network_calls':0,'completed_checked':sum(x['status']=='completed' for x in checks),'decisions':sum(x.get('decisions',0) for x in checks)});print(stage,'verified',sum(x['status']=='completed' for x in checks),'games;',sum(x.get('decisions',0) for x in checks),'decisions')
if __name__=='__main__':main(sys.argv[1])

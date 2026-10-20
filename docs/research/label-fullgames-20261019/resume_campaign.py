#!/usr/bin/env python3
"""Resume the frozen schedule, preserving all previously accepted decisions."""
import concurrent.futures, hashlib, json, os, re, subprocess, time, urllib.error, urllib.request
from pathlib import Path
import run_experiment as c
ROOT=c.ROOT

def read(path):return json.loads(path.read_text())
def history_row(state,chosen):
 return {'round':state['round'],'phase':state['phase'],'stage':state['stage'],'seen':[[p['cash'],p['goods'],p['influence'],p['promissory']] for p in state['participants']],'holder':state['role']['holder_idx'],'law_id':state['law_id'],'action':chosen}
def prefix(folder):
 p=folder/'trace.jsonl'
 return [json.loads(line) for line in p.read_text().splitlines()] if p.exists() else []
def verify_saved(folder,saved,state,messages):
 assert saved['state']==state and saved['index']>=0
 chosen={k:v for k,v in state['candidates'][saved['choice']].items() if k not in ('choice','after_own_action')}
 assert chosen==saved['action']
 if saved['source']=='forced':assert len(state['candidates'])==1
 else:
  matching=[p for p in folder.glob(f"call-{saved['index']:02d}-*.request.json") if read(p)['serial']==saved['request_serial']]
  assert len(matching)==1
  request=read(matching[0]);assert request['body']['messages'][:2]==messages
  raw=read(matching[0].with_name(matching[0].name.replace('.request.json','.response.json')))['raw']
  assert c.parse(raw,len(state['candidates']))['choice']==saved['choice']
 return chosen

def check_prefix(job):
 folder=ROOT/'cases'/job['id'];rows=prefix(folder)
 if not rows:return {'id':job['id'],'prefix_rows':0}
 proc=subprocess.Popen([str(c.BINARY)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,bufsize=1)
 history=[];index=0
 try:
  proc.stdin.write(json.dumps(config(job))+'\n');proc.stdin.flush()
  while index<len(rows):
   msg=json.loads(proc.stdout.readline())
   if msg['type']=='request':
    state=c.observation(msg['observation'],msg['phase']);messages=c.render(state,history,*c.LABELS[job['arm']]);chosen=verify_saved(folder,rows[index],state,messages)
    history.append(history_row(state,chosen));proc.stdin.write(json.dumps(chosen)+'\n');proc.stdin.flush();index+=1
   elif msg['type']=='decision':assert msg['parse_error'] is None
   else:raise AssertionError('Result before end of saved prefix')
 finally:
  if proc.poll() is None:proc.kill()
  proc.communicate(timeout=10)
 return {'id':job['id'],'prefix_rows':index,'prefix_sha256':c.sha(folder/'trace.jsonl')}
def config(job):return {'seed':job['seed'],'seat':job['seat'],'opponents':['greedy','tactical'],'builtin':None,'epoch':1,'vote_weight_mode':1}

class Resume(c.Campaign):
 def __init__(self,root,protocol):
  super().__init__(root,protocol)
  original=read(root/'interruptions/20261020-recovery/original-progress.json')
  self.started=time.monotonic()-original['elapsed_s'];self.calls=original['calls'];self.prompt_tokens=original['prompt_tokens'];self.completion_tokens=original['completion_tokens'];self.good=original['provider_responses'];self.errors=original['request_errors']
  self.completed=sum((root/'cases'/j['id']/'result.json').exists() and read(root/'cases'/j['id']/'result.json').get('status')=='completed' for j in protocol['jobs'])
 def ask(self,messages,n,case,index):
  body={**self.protocol['settings'],'messages':messages};format_repaired=False
  for attempt in range(3):
   self.guard();request_path=case/f'call-{index:02d}-{attempt}.request.json';response_path=case/f'call-{index:02d}-{attempt}.response.json'
   if request_path.exists():
    request=read(request_path);assert request['body']==body;serial=request['serial']
    if response_path.exists():raw=read(response_path)['raw']
    else:
     # A missing response is an already consumed attempt, never regenerated.
     continue
   else:
    with self.lock:
     if self.calls>=self.protocol['limits']['calls'] or self.completion_tokens>=self.protocol['limits']['completion_tokens'] or self.prompt_tokens>=self.protocol['limits']['prompt_tokens']:
      self.stop.set();raise RuntimeError('Inference resource limit reached')
     self.calls+=1;serial=self.calls
    c.write_json(request_path,{'serial':serial,'started_utc':c.utc(),'body':body})
    started=time.monotonic();req=urllib.request.Request(c.ENDPOINT,data=json.dumps(body,ensure_ascii=False).encode(),headers={'Authorization':'Bearer '+self.key,'Content-Type':'application/json'},method='POST')
    try:
     with urllib.request.urlopen(req,timeout=90) as response:raw=json.load(response)
     c.write_json(response_path,{'elapsed_s':round(time.monotonic()-started,3),'raw':raw})
     with self.lock:
      u=raw.get('usage',{});self.prompt_tokens+=u.get('prompt_tokens',0);self.completion_tokens+=u.get('completion_tokens',0);self.good+=1;self.consecutive=0;c.write_json(self.root/'progress.json',self.progress())
    except (urllib.error.URLError,TimeoutError) as error:
     detail=error.read().decode(errors='replace')[:2000] if isinstance(error,urllib.error.HTTPError) else str(error)
     c.write_json(case/f'call-{index:02d}-{attempt}.error.json',{'type':type(error).__name__,'status':getattr(error,'code',None),'detail':detail,'elapsed_s':round(time.monotonic()-started,3)})
     with self.lock:
      self.errors+=1;self.consecutive+=1
      if self.consecutive>=12 or getattr(error,'code',None) in [401,403] or 'usage limit' in detail.lower() or 'quota exceeded' in detail.lower():self.stop.set()
     retry=not isinstance(error,urllib.error.HTTPError) or error.code==429 or error.code>=500
     if attempt==2 or not retry or self.stop.is_set():raise RuntimeError('Provider failure; no game action substituted') from error
     time.sleep([2,6][attempt]);continue
   try:value=c.parse(raw,n)
   except (AssertionError,ValueError,KeyError,TypeError) as invalid:
    vp=case/f'call-{index:02d}-{attempt}.validation-error.json'
    if not vp.exists():c.write_json(vp,{'error':type(invalid).__name__+': '+str(invalid),'legal_choices':list(range(n))})
    if format_repaired or attempt==2 or raw.get('model')!='glm-5.3':raise RuntimeError('Invalid structured choice; no action substituted') from invalid
    format_repaired=True
    correction={'role':'user','content':'Ошибка формата предыдущего ответа. Верни JSON с полем choice, скопированным из одного из текущих candidates. Допустимые choice: '+json.dumps(list(range(n)))+'. Не придумывай номер. Другие условия и кандидаты прежние.'}
    body={**self.protocol['settings'],'messages':messages+[correction]};continue
   return value,serial
  raise RuntimeError('Three attempts exhausted before technical recovery; no action substituted')
 def game(self,job):
  case=ROOT/'cases'/job['id'];assert read(case/'job.json')==job
  old=read(case/'result.json') if (case/'result.json').exists() else None
  if old and old.get('status')=='completed':return old
  if old and not (case/'interrupted-result.json').exists():c.write_json(case/'interrupted-result.json',old)
  saved=prefix(case);original_bytes=(case/'trace.jsonl').read_bytes() if (case/'trace.jsonl').exists() else b''
  trace=[];history=[];proc=None;started=time.monotonic();result={**job,'started_utc':c.utc(),'status':'failed','recovery_segment':1,'reused_prefix_rows':len(saved)}
  try:
   self.guard()
   with (case/'engine-recovery.stderr').open('w') as err,(case/'trace.jsonl').open('a') as logfile:
    proc=subprocess.Popen([str(c.BINARY)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True,bufsize=1)
    proc.stdin.write(json.dumps(config(job))+'\n');proc.stdin.flush();role,entity=c.LABELS[job['arm']]
    while True:
     self.guard();line=proc.stdout.readline();assert line,'Engine closed without result';msg=json.loads(line)
     if msg['type']=='request':
      state=c.observation(msg['observation'],msg['phase']);candidates=state['candidates'];assert candidates
      messages=c.render(state,history,role,entity);canonical=c.render(state,history,'ROLE','ENTITY')
      for shown,canon in zip(messages,canonical):assert shown['content'].replace(role,'ROLE').replace(entity,'ENTITY')==canon['content']
      if job['arm']=='N':assert not re.search('президент|фракци',messages[0]['content']+'\n'+messages[1]['content'],re.I)
      if len(trace)<len(saved):
       row=saved[len(trace)];assert row['index']==len(trace);chosen=verify_saved(case,row,state,messages)
      else:
       if len(candidates)==1:value={'choice':0,'reason':'Only one legal candidate'};serial=None;source='forced'
       else:value,serial=self.ask(messages,len(candidates),case,len(trace));source='glm-5.3'
       chosen={k:v for k,v in candidates[value['choice']].items() if k not in ('after_own_action','choice')}
       row={'index':len(trace),'round':state['round'],'phase':state['phase'],'stage':state['stage'],'source':source,'request_serial':serial,'state':state,'choice':value['choice'],'action':chosen,'reason':value.get('reason')}
       logfile.write(json.dumps(row,ensure_ascii=False)+'\n');logfile.flush();os.fsync(logfile.fileno())
      trace.append(row);history.append(history_row(state,chosen));proc.stdin.write(json.dumps(chosen)+'\n');proc.stdin.flush()
     elif msg['type']=='decision':assert msg['parse_error'] is None,msg['parse_error']
     elif msg['type']=='result':
      game=msg['game'];assert len(game['phases'])==18 and len(game['payouts'])==3
      own=[a for phase in game['phases'] for a in phase['actions'] if a['actor']==job['seat']];assert all(a['ok'] for a in own)
      if (case/'game.json').exists():assert read(case/'game.json')==game
      else:c.write_json(case/'game.json',game)
      result.update(status='completed',payout=game['payouts'][job['seat']],net_profit=game['payouts'][job['seat']]-game['entry_fee'],rank=game['ranks'].index(job['seat']),decisions=len(trace),model_decisions=sum(x['source']=='glm-5.3' for x in trace));break
    proc.wait(timeout=10);assert proc.returncode==0
  except Exception as error:result.update(error=type(error).__name__+': '+str(error),decisions=len(trace))
  finally:
   if proc is not None and proc.poll() is None:proc.kill();proc.wait(timeout=10)
   assert (case/'trace.jsonl').read_bytes().startswith(original_bytes) if (case/'trace.jsonl').exists() else not original_bytes
   result['elapsed_s']=round(time.monotonic()-started,3);c.write_json(case/'result.json',result)
   with self.lock:
    self.completed+=result['status']=='completed';self.failed+=result['status']!='completed'
    with (ROOT/'runs.jsonl').open('a') as log:log.write(json.dumps(result,ensure_ascii=False)+'\n');log.flush();os.fsync(log.fileno())
    c.write_json(ROOT/'progress.json',self.progress());print(f"Games {self.completed}/72 completed; failures {self.failed}; {job['id']} {result['status']}; calls {self.calls}",flush=True)
  return result

def main():
 import sys
 p=read(ROOT/'protocol.json')
 for name,h in p['hashes'].items():assert c.sha(ROOT/name)==h,name
 assert c.sha(c.BINARY)==p['runtime_binary_sha256']
 jobs=[j for j in p['jobs'] if not (ROOT/'cases'/j['id']/'result.json').exists() or read(ROOT/'cases'/j['id']/'result.json').get('status')!='completed']
 if sys.argv[1]=='preflight':
  checks=[check_prefix(j) for j in jobs];c.write_json(ROOT/'recovery-preflight.json',{'checked_utc':c.utc(),'remaining_jobs':len(jobs),'checks':checks,'network_calls':0});print('Preflight passed',len(jobs),'jobs;',sum(x['prefix_rows'] for x in checks),'preserved decisions');return
 assert sys.argv[1]=='run'
 registration=read(ROOT/'recovery-registration.json');assert registration['push_confirmed_before_new_calls']
 camp=Resume(ROOT,p)
 with concurrent.futures.ThreadPoolExecutor(max_workers=p['workers']) as pool:results=list(pool.map(camp.game,jobs))
 c.write_json(ROOT/'campaign-finished.json',{'finished_utc':c.utc(),**camp.progress(),'recovery_registration':registration,'technical_pause':True,'statuses':{'completed':camp.completed,'failed':camp.failed}})
 print('Collection finished',json.dumps(camp.progress()),flush=True)
if __name__=='__main__':main()

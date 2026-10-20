#!/usr/bin/env python3
"""Complete simulated games with a frozen context harness and durable resumption."""
import concurrent.futures,hashlib,json,os,subprocess,sys,threading,time
from pathlib import Path
import common as c
from model_api import API
from discover import execute
ROOT=Path(__file__).resolve().parent

def read(path):return json.loads(path.read_text())
def content_hash(messages):return hashlib.sha256(json.dumps(messages,ensure_ascii=False,separators=(',',':')).encode()).hexdigest()
def history_row(state,action):return {'round':state['round'],'phase':state['phase'],'stage':state['stage'],'seen':[[p['cash'],p['goods'],p['influence'],p['promissory']] for p in state['participants']],'holder':state['role']['holder_idx'],'law_id':state['law_id'],'action':action}
def harness_for(stage,arm):return ROOT/('baseline' if arm in ['A0','A1'] else 'candidate-initial' if stage=='dev' else 'candidate-final')
def render(state,history,harness):
 messages=c.render(state,history,'президент','фракция');memory=(harness/'memory.md').read_text();helper=(harness/'helper.py').read_text()
 features=execute(helper,'helper',state=state,history=history)
 if not features['ok']:raise RuntimeError('Harness execution failed: '+features.get('error',''))
 if memory.strip():messages[0]['content']+='\n\nДополнение проверяемой версии обвязки:\n'+memory
 user=json.loads(messages[1]['content']);user['harness_context']=features['value'];messages[1]['content']=json.dumps(user,ensure_ascii=False,separators=(',',':'))
 return messages,features['value']

class Collector:
 def __init__(self,stage,protocol):
  self.stage=stage;self.protocol=protocol;self.jobs=protocol[stage+'_jobs'];self.root=ROOT/'runs'/stage;self.root.mkdir(parents=True,exist_ok=True);self.api=API(ROOT/'game-calls',protocol['collection_limits']);self.lock=threading.Lock();self.completed=self.failed=0
  for job in self.jobs:
   p=self.root/job['id']/'result.json'
   if p.exists() and read(p)['status']=='completed':self.completed+=1
 def progress(self):return {'stage':self.stage,'completed':self.completed,'failed':self.failed,'planned':len(self.jobs),'api':self.api.stats()}
 def ask(self,messages,n,job,index):
  body={**self.protocol['executor_settings'],'messages':messages};repaired=False
  for attempt in range(3):
   tag=f"{job['id']}-s{index:02d}-a{attempt}"
   try:raw,serial=self.api.call(tag,body)
   except RuntimeError:
    if self.api.stop.is_set() or attempt==2:raise
    time.sleep(2);continue
   try:return c.parse(raw,n),serial,tag
   except (AssertionError,ValueError,KeyError,TypeError) as error:
    c.write_json(ROOT/'game-calls'/(tag+'.validation-error.json'),{'error':str(error),'legal_choices':list(range(n))})
    if repaired or attempt==2 or raw.get('model')!='glm-5.3':raise RuntimeError('Invalid model choice; no action substituted') from error
    repaired=True;body={**self.protocol['executor_settings'],'messages':messages+[{'role':'user','content':'Ошибка формата предыдущего ответа. Верни JSON с целочисленным choice из текущих candidates. Допустимые номера: '+json.dumps(list(range(n)))+'. Состояние и кандидаты прежние.'}]}
  raise RuntimeError('Provider attempts exhausted')
 def game(self,job):
  folder=self.root/job['id'];folder.mkdir(exist_ok=True)
  if (folder/'job.json').exists():assert read(folder/'job.json')==job
  else:c.write_json(folder/'job.json',job)
  if (folder/'result.json').exists() and read(folder/'result.json')['status']=='completed':return read(folder/'result.json')
  if (folder/'result.json').exists():
   with (folder/'interruptions.jsonl').open('a') as f:f.write(json.dumps(read(folder/'result.json'),ensure_ascii=False)+'\n')
  saved=[json.loads(s) for s in (folder/'trace.jsonl').read_text().splitlines()] if (folder/'trace.jsonl').exists() else []
  saved_bytes=(folder/'trace.jsonl').read_bytes() if (folder/'trace.jsonl').exists() else b''
  trace=[];history=[];proc=None;started=time.monotonic();harness=harness_for(self.stage,job['arm']);fingerprint={n:c.sha(harness/n) for n in ['memory.md','helper.py']};result={**job,'status':'failed','started_utc':c.utc(),'harness':fingerprint}
  try:
   self.api.guard()
   with (folder/'engine.stderr').open('a') as err,(folder/'trace.jsonl').open('a') as logfile:
    proc=subprocess.Popen([str(ROOT/'bin/evalgame')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True,bufsize=1)
    proc.stdin.write(json.dumps({'seed':job['seed'],'seat':job['seat'],'opponents':job['opponents'],'builtin':None,'epoch':1,'vote_weight_mode':1})+'\n');proc.stdin.flush()
    while True:
     self.api.guard();line=proc.stdout.readline();assert line,'Engine closed without result';msg=json.loads(line)
     if msg['type']=='request':
      state=c.observation(msg['observation'],msg['phase']);candidates=state['candidates'];messages,features=render(state,history,harness)
      if len(trace)<len(saved):
       row=saved[len(trace)];assert row['state']==state and row['features']==features and row['messages_sha256']==content_hash(messages)
       chosen={k:v for k,v in candidates[row['choice']].items() if k not in ['choice','after_own_action']};assert chosen==row['action']
       if row['source']!='forced':
        request=read(ROOT/'game-calls'/(row['request_tag']+'.request.json'));raw=read(ROOT/'game-calls'/(row['request_tag']+'.response.json'))['raw'];assert request['body']['messages'][:2]==messages and c.parse(raw,len(candidates))['choice']==row['choice'] and request['serial']==row['request_serial']
      else:
       if len(candidates)==1:value={'choice':0,'reason':'Only one legal candidate'};serial=None;tag=None;source='forced'
       else:value,serial,tag=self.ask(messages,len(candidates),job,len(trace));source='glm-5.3'
       chosen={k:v for k,v in candidates[value['choice']].items() if k not in ['choice','after_own_action']};row={'index':len(trace),'state':state,'features':features,'messages_sha256':content_hash(messages),'choice':value['choice'],'action':chosen,'reason':value.get('reason'),'source':source,'request_tag':tag,'request_serial':serial}
       logfile.write(json.dumps(row,ensure_ascii=False)+'\n');logfile.flush();os.fsync(logfile.fileno())
      trace.append(row);history.append(history_row(state,chosen));proc.stdin.write(json.dumps(chosen)+'\n');proc.stdin.flush()
     elif msg['type']=='decision':assert msg['parse_error'] is None,msg['parse_error']
     elif msg['type']=='result':
      game=msg['game'];assert len(game['phases'])==18 and len(game['payouts'])==3
      own=[a for phase in game['phases'] for a in phase['actions'] if a['actor']==job['seat']];assert all(a['ok'] for a in own),'Illegal action'
      if (folder/'game.json').exists():assert read(folder/'game.json')==game
      else:c.write_json(folder/'game.json',game)
      proc.wait(timeout=10);assert proc.returncode==0
      result.update(status='completed',payout=game['payouts'][job['seat']],net_profit=game['payouts'][job['seat']]-game['entry_fee'],rank=game['ranks'].index(job['seat'])+1,decisions=len(trace),model_decisions=sum(t['source']=='glm-5.3' for t in trace));break
  except Exception as error:result.update(error=type(error).__name__+': '+str(error),decisions=len(trace))
  finally:
   if proc is not None and proc.poll() is None:proc.kill();proc.wait(timeout=10)
   if proc is not None:
    proc.stdin.close();proc.stdout.close()
   assert (folder/'trace.jsonl').read_bytes().startswith(saved_bytes) if (folder/'trace.jsonl').exists() else not saved_bytes
   result['elapsed_s']=round(time.monotonic()-started,3);c.write_json(folder/'result.json',result)
   with self.lock:
    self.completed+=result['status']=='completed';self.failed+=result['status']!='completed'
    c.write_json(ROOT/(self.stage+'-progress.json'),self.progress())
    with (ROOT/'collection-events.jsonl').open('a') as log:log.write(json.dumps(result,ensure_ascii=False)+'\n');log.flush();os.fsync(log.fileno())
    print(self.stage,self.completed,'/',len(self.jobs),'complete;',self.failed,'failed;',job['id'],result['status'],'calls',self.api.calls,flush=True)
  return result

def main(stage):
 p=read(ROOT/'protocol.json')
 for name,h in p['hashes'].items():assert c.sha(ROOT/name)==h,name
 assert c.sha(ROOT/'bin/evalgame')==p['engine_binary_sha256']
 if stage=='test':
  assert read(ROOT/'freeze-registration.json')['push_confirmed_before_heldout']
  for name,h in read(ROOT/'harness-freeze.json')['hashes'].items():assert c.sha(ROOT/name)==h,name
 assert __import__('shutil').disk_usage(ROOT).free>512*1024*1024,'Need at least 512 MiB free before starting'
 campaign=Collector(stage,p)
 with concurrent.futures.ThreadPoolExecutor(max_workers=p['workers']) as pool:results=list(pool.map(campaign.game,campaign.jobs))
 c.write_json(ROOT/'game-calls/usage.json',campaign.api.stats());c.write_json(ROOT/(stage+'-finished.json'),{'finished_utc':c.utc(),**campaign.progress(),'statuses':{s:sum(r['status']==s for r in results) for s in ['completed','failed']}})
if __name__=='__main__':main(sys.argv[1])

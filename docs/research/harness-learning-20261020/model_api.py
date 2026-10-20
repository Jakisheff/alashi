#!/usr/bin/env python3
"""Bounded, durable model calls; credentials never enter stored requests."""
import json,os,shutil,threading,time,urllib.request,urllib.error
from pathlib import Path
from common import utc,write_json,ENDPOINT
class API:
 def __init__(self,root,limits):
  self.root=Path(root);self.root.mkdir(parents=True,exist_ok=True);self.limits=limits;self.lock=threading.Lock();prior=json.loads((self.root/'usage.json').read_text()).get('active_s',0) if (self.root/'usage.json').exists() else 0;self.started=time.monotonic()-prior;self.stop=threading.Event()
  self.key=os.environ.get('ALASHI_ZAI_CODING_KEY') or json.loads((Path.home()/'.local/share/opencode/auth.json').read_text())['zai-coding-plan']['key']
  requests=[json.loads(p.read_text()) for p in self.root.glob('*.request.json')];self.calls=max([p['serial'] for p in requests],default=0)
  responses=[json.loads(p.read_text())['raw'] for p in self.root.glob('*.response.json')];self.responses=len(responses);self.prompt=sum(p.get('usage',{}).get('prompt_tokens',0) for p in responses);self.completion=sum(p.get('usage',{}).get('completion_tokens',0) for p in responses);self.errors=len(list(self.root.glob('*.error.json')))
 def guard(self):
  if self.stop.is_set():raise RuntimeError('Campaign stopped')
  if shutil.disk_usage(self.root).free<256*1024*1024 or time.monotonic()-self.started>self.limits['wall_s']:
   self.stop.set();raise RuntimeError('Free-space or active-time guard; durable calls retained')
 def stats(self):return {'utc':utc(),'calls':self.calls,'responses':self.responses,'errors':self.errors,'prompt_tokens':self.prompt,'completion_tokens':self.completion,'active_s':round(time.monotonic()-self.started,2),'stopped':self.stop.is_set()}
 def call(self,tag,body):
  self.guard();request_path=self.root/(tag+'.request.json');response_path=self.root/(tag+'.response.json');error_path=self.root/(tag+'.error.json')
  if response_path.exists():assert json.loads(request_path.read_text())['body']==body;return json.loads(response_path.read_text())['raw'],json.loads(request_path.read_text())['serial']
  if request_path.exists():
   assert json.loads(request_path.read_text())['body']==body
   raise RuntimeError('Prior attempt has no saved response; consumed attempt '+tag)
  with self.lock:
   if self.calls>=self.limits['calls'] or self.prompt>=self.limits['prompt_tokens'] or self.completion>=self.limits['completion_tokens']:self.stop.set();raise RuntimeError('Inference limit reached')
   self.calls+=1;serial=self.calls;write_json(request_path,{'serial':serial,'utc':utc(),'body':body})
  req=urllib.request.Request(ENDPOINT,data=json.dumps(body,ensure_ascii=False).encode(),headers={'Authorization':'Bearer '+self.key,'Content-Type':'application/json'},method='POST');started=time.monotonic()
  try:
   with urllib.request.urlopen(req,timeout=150) as response:raw=json.load(response)
   write_json(response_path,{'elapsed_s':round(time.monotonic()-started,3),'raw':raw})
   with self.lock:
    self.responses+=1;u=raw.get('usage',{});self.prompt+=u.get('prompt_tokens',0);self.completion+=u.get('completion_tokens',0);write_json(self.root/'usage.json',self.stats())
   return raw,serial
  except (urllib.error.URLError,TimeoutError) as error:
   detail=error.read().decode(errors='replace')[:1500] if isinstance(error,urllib.error.HTTPError) else str(error)
   write_json(error_path,{'type':type(error).__name__,'status':getattr(error,'code',None),'detail':detail,'elapsed_s':round(time.monotonic()-started,3)})
   with self.lock:self.errors+=1
   if getattr(error,'code',None) in [401,403] or 'usage limit' in detail.lower() or 'quota exceeded' in detail.lower():self.stop.set()
   raise RuntimeError('Provider failure; attempt consumed') from error

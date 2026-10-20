#!/usr/bin/env python3
"""Offline audit of preserved evidence, first valid choices and call accounting."""
import hashlib,json,zipfile
from pathlib import Path
import run_experiment as c
ROOT=c.ROOT

def load(p):return json.loads(p.read_text())
def main():
 protocol=load(ROOT/'protocol.json');progress=load(ROOT/'campaign-finished.json')
 for name,digest in protocol['hashes'].items():assert c.sha(ROOT/name)==digest,name
 preserved=0;backup=ROOT/'interruptions/20261020-recovery/original-interrupted-state.zip'
 assert c.sha(backup)=='24f6b827b216a9dd39e9d876bfa9b3d7fa8f1683b83b29e7d85b459b5071fbd2'
 with zipfile.ZipFile(backup) as z:
  for name in z.namelist():
   original=z.read(name);path=ROOT/name
   if name=='progress.json' or name.endswith('/result.json'):continue
   if name=='runs.jsonl' or name.endswith('/trace.jsonl'):assert path.read_bytes().startswith(original),name
   else:assert path.read_bytes()==original,name
   preserved+=1
 requests=[];responses=[];errors=[];decisions=0;forced=0;unknown=[]
 for job in protocol['jobs']:
  folder=ROOT/'cases'/job['id'];assert load(folder/'job.json')==job
  result=load(folder/'result.json');assert result['id']==job['id']
  if result['status']!='completed':continue
  rows=[json.loads(x) for x in (folder/'trace.jsonl').read_text().splitlines()]
  for index,row in enumerate(rows):
   assert row['index']==index;paths=sorted(folder.glob(f'call-{index:02d}-*.request.json'));assert len(paths)<=3
   if row['source']=='forced':assert len(row['state']['candidates'])==1 and not paths;forced+=1;continue
   first_valid=None
   for p in paths:
    response=p.with_name(p.name.replace('.request.json','.response.json'))
    if response.exists():
     try:value=c.parse(load(response)['raw'],len(row['state']['candidates']))
     except (AssertionError,ValueError,KeyError,TypeError):continue
     first_valid=(load(p)['serial'],value['choice']);break
   assert first_valid==(row['request_serial'],row['choice']),(job['id'],index)
   decisions+=1
  assert result['decisions']==len(rows)
 for path in (ROOT/'cases').glob('*/*.request.json'):
  request=load(path);requests.append(request)
  response=path.with_name(path.name.replace('.request.json','.response.json'));error=path.with_name(path.name.replace('.request.json','.error.json'))
  if response.exists():responses.append(load(response)['raw'])
  elif error.exists():errors.append(load(error))
  else:unknown.append(str(path.relative_to(ROOT)))
 serials=sorted(x['serial'] for x in requests);assert serials==list(range(1,len(serials)+1)) and len(serials)==progress['calls']
 assert len(responses)==progress['provider_responses'] and len(errors)==progress['request_errors']
 for key in ['prompt_tokens','completion_tokens']:assert sum(x.get('usage',{}).get(key,0) for x in responses)==progress[key]
 ids=[x.get('id') for x in responses];assert len(ids)==len(set(ids)) and all(ids)
 assert {x['model'] for x in responses}=={'glm-5.3'}
 assert len(unknown)==2 and all('call-15-1' in x or 'call-05-0' in x for x in unknown)
 output={'completed_games':progress['completed_games'],'model_choices_verified':decisions,'forced_choices':forced,'original_files_or_prefixes_preserved':preserved,'unique_request_serials':len(serials),'unique_provider_responses':len(ids),'usage_totals_match_saved_responses':True,'first_valid_choice_always_applied':True,'unknown_outcome_requests':unknown,'network_calls':0}
 c.write_json(ROOT/'integrity-validation.json',output);print(json.dumps(output,ensure_ascii=False,indent=2))
if __name__=='__main__':main()

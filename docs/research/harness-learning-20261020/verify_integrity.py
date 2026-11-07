#!/usr/bin/env python3
"""Archive provenance, budget and schedule checks after collection finishes."""
import collections,json
from pathlib import Path
import common as c
ROOT=Path(__file__).resolve().parent
OLD=ROOT.parent/'alashi-label-fullgames-20261019'
def read(p):return json.loads(p.read_text())
def main():
 p=read(ROOT/'protocol.json');freeze=read(ROOT/'harness-freeze.json')
 assert (ROOT/'test-finished.json').exists() and (ROOT/'test-validation.json').exists()
 for name,h in freeze['hashes'].items():assert c.sha(ROOT/name)==h,name
 assert c.sha(ROOT/'bin/evalgame')==freeze['engine_binary_sha256']
 for name,h in p['hashes'].items():assert c.sha(ROOT/name)==h,name
 manifest=read(ROOT/'training-manifest.json');assert c.sha(OLD/'protocol.json')==manifest['source_protocol_sha256']
 old_seeds=set()
 for row in manifest['cases']:
  folder=OLD/'cases'/row['source_case'];old_seeds.add(read(folder/'job.json')['seed'])
  for kind in ['trace','game']:assert c.sha(folder/('trace.jsonl' if kind=='trace' else 'game.json'))==row[kind+'_sha256']
 assert len(manifest['cases'])==48 and len(old_seeds)==8
 assert not old_seeds&set(p['dev_seeds']) and not old_seeds&set(p['test_seeds']) and not set(p['dev_seeds'])&set(p['test_seeds'])
 for stage in ['initial','final']:
  author=read(ROOT/(stage+'-authorship.json'));candidate=ROOT/('candidate-'+stage);raw=read(ROOT/author['raw_submission_file'])['raw'];v=json.loads(raw['choices'][0]['message']['content'])
  for name,field in [('memory.md','memory_md'),('helper.py','helper_py')]:assert (candidate/name).read_bytes()==(v[field]+'\n').encode()
 stats={}
 for name in ['game-calls','discovery/initial/calls','discovery/final/calls']:
  folder=ROOT/name;requests=list(folder.glob('*.request.json'));responses=list(folder.glob('*.response.json'));errors=list(folder.glob('*.error.json'));serials=[read(f)['serial'] for f in requests]
  assert sorted(serials)==list(range(1,len(serials)+1)),name
  prompt=completion=0;models=collections.Counter()
  for response in responses:
   request=response.with_name(response.name.replace('.response.json','.request.json'));assert request.exists()
   raw=read(response)['raw'];models[raw['model']]+=1;assert raw['model']=='glm-5.3';u=raw.get('usage',{});prompt+=u.get('prompt_tokens',0);completion+=u.get('completion_tokens',0)
  usage=read(folder/'usage.json');assert (usage['calls'],usage['responses'],usage['prompt_tokens'],usage['completion_tokens'])==(len(requests),len(responses),prompt,completion),name
  stats[name]={'requests':len(requests),'responses':len(responses),'provider_errors':len(errors),'format_validation_errors':len(list(folder.glob('*.validation-error.json'))),'requests_without_saved_response':sum(not request.with_name(request.name.replace('.request.json','.response.json')).exists() for request in requests),'prompt_tokens':prompt,'completion_tokens':completion,'model_counts':dict(models)}
 stage_counts={}
 for stage in ['dev','test']:
  scheduled=p[stage+'_jobs'];stage_counts[stage]=collections.Counter()
  for job in scheduled:
   folder=ROOT/'runs'/stage/job['id'];assert read(folder/'job.json')==job;result=read(folder/'result.json');assert all(result[k]==v for k,v in job.items());stage_counts[stage][result['status']]+=1
  checked=read(ROOT/(stage+'-validation.json'));assert checked['completed_checked']==stage_counts[stage]['completed'];assert checked['network_calls']==0
  finished=read(ROOT/(stage+'-finished.json'));assert finished['statuses']['completed']==stage_counts[stage]['completed'] and finished['statuses']['failed']==stage_counts[stage]['failed']
 cache=read(ROOT/'test-incremental-validation.json')
 for key,values in cache['fingerprints'].items():
  for name,h in values.items():assert c.sha(ROOT/'runs/test'/key/name)==h,(key,name)
 assert set(cache['checks'])=={j['id'] for j in p['test_jobs']}
 first_test=min(read(f)['utc'] for f in (ROOT/'game-calls').glob('test-*.request.json'))
 assert first_test>freeze['utc'] and read(ROOT/'freeze-registration.json')['push_confirmed_before_heldout']
 c.write_json(ROOT/'integrity-validation.json',{'utc':c.utc(),'frozen_files_unchanged':True,'engine_unchanged':True,'model_authorship_verified':True,'old_source_hashes_unchanged':True,'seed_partitions_disjoint':True,'old_seed_clusters':len(old_seeds),'all_scheduled_jobs_accounted_for':True,'validation_fingerprints_unchanged':True,'stage_counts':{k:dict(v) for k,v in stage_counts.items()},'usage':stats,'first_heldout_request_utc':first_test,'frozen_utc':freeze['utc'],'freeze_commit':read(ROOT/'freeze-registration.json')['commit']})
 print('All frozen files, authorship, source hashes, schedules and token counters verified')
if __name__=='__main__':main()

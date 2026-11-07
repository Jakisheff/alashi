#!/usr/bin/env python3
import json,random,secrets
from pathlib import Path
import common as c
ROOT=Path(__file__).resolve().parent

def main():
 assert not (ROOT/'protocol.json').exists()
 old=json.loads((ROOT.parent/'alashi-label-fullgames-20261019/protocol.json').read_text());used=set(old['seeds'])
 def seeds(n):
  out=[]
  while len(out)<n:
   x=secrets.randbits(63)
   if x not in used:used.add(x);out.append(x)
  return out
 dev=seeds(3);test=seeds(12)
 def jobs(values,arms,kind):
  out=[]
  for i,seed in enumerate(values):
   opponents=['greedy','tactical'] if kind=='dev' or i<6 else ['random','genome']
   for seat in range(3):
    for arm in arms:out.append({'id':f'{kind}-s{i+1}-seat{seat}-{arm}','seed_index':i,'seed':seed,'seat':seat,'arm':arm,'opponents':opponents,'partition':kind})
  random.Random(2026102001 if kind=='dev' else 2026102002).shuffle(out);return out
 files=['common.py','safe_runtime.py','model_api.py','discover.py','rules.md','training.json','training-manifest.json']
 protocol={'schema':1,'created_utc':c.utc(),'status':'frozen_before_discovery','model':'glm-5.3','executor_settings':c.SETTINGS,'discovery_settings':{**c.SETTINGS,'reasoning_effort':'high','max_tokens':10000},'discovery_limits':{'calls':60,'prompt_tokens':1500000,'completion_tokens':400000,'wall_s':1800},'collection_limits':{'calls':2900,'prompt_tokens':16000000,'completion_tokens':3000000,'wall_s':5400},'workers':4,'engine_source_commit':old['engine'].split('(')[1].split(')')[0] if '(' in old['engine'] else '929e4e5','engine_binary_sha256':c.sha(ROOT/'bin/evalgame'),'training_selection':'All 48 prior A0/A1 games; remove reasons, provider messages, human analyses, labels and seed IDs; no outcome selection','initial_discovery_max_turns':12,'post_dev_max_turns':5,'candidate_surface':'memory.md <=3500 chars and helper.py augment(state,history) pure data function <=16000 chars, output <=8000 chars; same legal action menu; no rule/action overrides','discovery_isolation':'Fresh API messages only, numerical corpus through read_game/calculate tools. No parent conversation, filesystem or human reports. General bid rules are shared with all arms; initial description does not name or explain the previously identified derived mechanism.','dev_seeds':dev,'test_seeds':test,'dev_jobs':jobs(dev,['A0','H'],'dev'),'test_jobs':jobs(test,['A0','A1','H'],'test'),'arms':{'A0':'unchanged baseline','A1':'independent repeat of unchanged baseline','H':'one frozen model-authored harness after diagnostic feedback'},'adaptation':'One initial proposal, 18 diagnostic development games, then at most one model-authored final revision. Do not choose or rewrite a strategy by human judgment. Freeze final files and collection code publicly before first heldout response. No adaptation on heldout results.','primary':'Within each heldout seed, mean H payout over all three seats minus mean of both A0/A1 over all three seats; mean difference over 12 seed clusters. Two-sided exact 3^K cluster-arm permutation; 95% percentile bootstrap, 10000 samples, PRNG 2026102003. Inferential unit seed.','failures':'Only complete nine-game clusters enter primary comparison; minimum eight complete clusters for a primary conclusion. Report all failures, no replacement seeds or substituted actions.','secondary':'Net profit, wealth rank, licenses, influence and action distributions, API tokens and time; paired effects within familiar/novel opponents descriptive. Discovery claim fidelity and applicability assessed separately from payout.','calls':'At most three provider attempts per decision, first valid saved choice used, one format-only correction. Unknown outcomes consume attempts. Sole legal action forced equally in every arm. One model decision per non-forced turn in every arm with identical executor settings; learned input features add input tokens, reported separately.','recovery':'Every request and first valid response persisted before action; replay accepted prefixes after interruption; no resampling of accepted choices. Free-space guard 256 MiB and pre-run minimum 512 MiB.','hashes':{name:c.sha(ROOT/name) for name in files},'old_training_seeds':old['seeds']}
 c.write_json(ROOT/'protocol.json',protocol);print('Frozen discovery and evaluation plan: 48 old games, 18 development, 108 heldout games; 12 new seed clusters')
if __name__=='__main__':main()

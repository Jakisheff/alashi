#!/usr/bin/env python3
"""Verify model authorship and hash the evaluation surface before heldout calls."""
import json
from pathlib import Path
import common as c
ROOT=Path(__file__).resolve().parent

def main():
 protocol=json.loads((ROOT/'protocol.json').read_text())
 for name,value in protocol['hashes'].items():assert c.sha(ROOT/name)==value,name
 assert c.sha(ROOT/'bin/evalgame')==protocol['engine_binary_sha256']
 assert json.loads((ROOT/'dev-validation.json').read_text())['completed_checked']==18
 assert json.loads((ROOT/'discovery/final/result.json').read_text())['status']=='submitted'
 candidate=ROOT/'candidate-final';claim=json.loads((candidate/'claim.json').read_text());matches=[]
 for path in (ROOT/'discovery/final/calls').glob('*.response.json'):
  raw=json.loads(path.read_text())['raw']
  try:value=json.loads(raw['choices'][0]['message']['content'])
  except (KeyError,ValueError,TypeError):continue
  if value.get('tool')=='submit' and all(value.get(k)==v for k,v in claim.items()) and (value['memory_md']+'\n').encode()==(candidate/'memory.md').read_bytes() and (value['helper_py']+'\n').encode()==(candidate/'helper.py').read_bytes():matches.append(str(path.relative_to(ROOT)))
 assert len(matches)==1,matches
 initial=json.loads((ROOT/'discovery/initial/calls/t00-a0.request.json').read_text())
 initial_text=json.dumps(initial['body']['messages'],ensure_ascii=False).lower()
 assert not any(term in initial_text for term in ['эскроу','escrow','saved_tribute'])
 authorship={'candidate_exactly_matches_model_submission':True,'raw_submission_file':matches[0],'human_edits_to_memory_or_helper':False,'initial_prompt_has_no_targeted_term':True,'heldout_data_available_to_learner':False,'memory_sha256':c.sha(candidate/'memory.md'),'helper_sha256':c.sha(candidate/'helper.py')}
 c.write_json(ROOT/'final-authorship.json',authorship)
 names=['protocol.json','candidate-final/memory.md','candidate-final/helper.py','candidate-final/claim.json','final-authorship.json','baseline/memory.md','baseline/helper.py','rules.md','training.json','training-manifest.json','common.py','safe_runtime.py','model_api.py','discover.py','complete_discovery.py','collect.py','analyze.py','validate_games.py','settlement_check.py','audit_claim.py','counterfactual_check.py','freeze.py','test_runtime.py','test_collect.py','test_api.py','analysis-validation.json','controller-validation.json']
 c.write_json(ROOT/'harness-freeze.json',{'utc':c.utc(),'status':'frozen_before_heldout','hashes':{n:c.sha(ROOT/n) for n in names},'engine_binary_sha256':c.sha(ROOT/'bin/evalgame'),'development_games':18,'heldout_games_started':0,'model_authorship_verified':True})
 print('Verified raw model authorship and froze',len(names),'files before heldout games')
if __name__=='__main__':main()

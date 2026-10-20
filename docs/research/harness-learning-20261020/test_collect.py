import json,shutil,tempfile,threading,unittest
from pathlib import Path
from unittest.mock import patch
import collect,common as c
class FakeAPI:
 def __init__(self,root,limits):
  self.root=Path(root);self.root.mkdir(exist_ok=True);self.calls=len(list(self.root.glob('*.request.json')));self.stop=threading.Event();self.new=0
 def guard(self):pass
 def stats(self):return {'calls':self.calls}
 def call(self,tag,body):
  rp=self.root/(tag+'.request.json');sp=self.root/(tag+'.response.json')
  if rp.exists():
   assert json.loads(rp.read_text())['body']==body
   return json.loads(sp.read_text())['raw'],json.loads(rp.read_text())['serial']
  self.calls+=1;self.new+=1;choice=999 if self.calls==1 else 0
  raw={'model':'glm-5.3','choices':[{'finish_reason':'stop','message':{'content':json.dumps({'choice':choice,'reason':'mock'})}}]};c.write_json(rp,{'serial':self.calls,'body':body});c.write_json(sp,{'raw':raw});return raw,self.calls
class CollectorTests(unittest.TestCase):
 def test_complete_then_recover_accepted_prefix_without_resampling(self):
  with tempfile.TemporaryDirectory() as d:
   root=Path(d);(root/'bin').mkdir();shutil.copy2(collect.ROOT/'bin/evalgame',root/'bin/evalgame');shutil.copytree(collect.ROOT/'baseline',root/'baseline')
   job={'id':'dev-mock-seat0-A0','seed_index':0,'seed':987321,'seat':0,'arm':'A0','opponents':['greedy','tactical'],'partition':'dev'};protocol={'dev_jobs':[job],'collection_limits':{},'executor_settings':c.SETTINGS}
   with patch.object(collect,'ROOT',root),patch.object(collect,'API',FakeAPI):
    first=collect.Collector('dev',protocol);result=first.game(job);self.assertEqual(result['status'],'completed',result)
    folder=root/'runs/dev'/job['id'];game=json.loads((folder/'game.json').read_text());rows=(folder/'trace.jsonl').read_text().splitlines(keepends=True);prefix=''.join(rows[:5]);(folder/'trace.jsonl').write_text(prefix);(folder/'result.json').unlink();(folder/'game.json').unlink()
    resumed=collect.Collector('dev',protocol);again=resumed.game(job);self.assertEqual(again['status'],'completed',again);self.assertEqual(json.loads((folder/'game.json').read_text()),game);self.assertEqual(resumed.api.new,0);self.assertTrue((folder/'trace.jsonl').read_text().startswith(prefix));self.assertEqual(len(list((root/'game-calls').glob('*.validation-error.json'))),1)
 def test_baseline_context_is_pure(self):
  old=collect.ROOT.parent/'alashi-label-fullgames-20261019';state=json.loads((old/'cases/s1-seat0-A0/trace.jsonl').read_text().splitlines()[0])['state'];before=json.dumps(state,sort_keys=True);messages,features=collect.render(state,[],collect.ROOT/'baseline');self.assertEqual(features,{});self.assertEqual(json.dumps(state,sort_keys=True),before);self.assertNotIn('эскроу',messages[0]['content'].lower())
if __name__=='__main__':unittest.main()

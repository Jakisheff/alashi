#!/usr/bin/env python3
import contextlib,io,json,tempfile,threading,time,zipfile
from pathlib import Path
from unittest.mock import patch
import run_experiment as c
from resume_campaign import Resume
import replay
p=json.loads((c.ROOT/'protocol.json').read_text())
with zipfile.ZipFile(c.ROOT/'interruptions/20261020-recovery/original-interrupted-state.zip') as z:
 original_complete={json.loads(z.read(n))['id'] for n in z.namelist() if n.endswith('/result.json') and json.loads(z.read(n)).get('status')=='completed'}
checks=[replay.replay(c.ROOT/'cases'/j['id']) for j in p['jobs'] if j['id'] in original_complete]
c.write_json(c.ROOT/'interruption-completed-replay.json',{'games_verified':len(checks),'checks':checks,'network_calls':0})
raw={'model':'glm-5.3','choices':[{'finish_reason':'stop','message':{'content':'{"choice":0,"reason":"test"}'}}]}
with tempfile.TemporaryDirectory() as name:
 root=Path(name);obj=Resume.__new__(Resume);obj.root=root;obj.protocol=p;obj.key='test';obj.lock=threading.Lock();obj.stop=threading.Event();obj.started=time.monotonic();obj.calls=2;obj.good=obj.errors=obj.prompt_tokens=obj.completion_tokens=obj.completed=obj.failed=obj.consecutive=0
 messages=[{'role':'system','content':'test'},{'role':'user','content':'test'}];body={**p['settings'],'messages':messages}
 for i in range(2):c.write_json(root/f'call-00-{i}.request.json',{'serial':i+1,'body':body})
 with patch('urllib.request.urlopen',return_value=contextlib.closing(io.StringIO(json.dumps(raw)))) as mocked:
  value,serial=obj.ask(messages,2,root,0);assert mocked.call_count==1 and serial==3 and value['choice']==0
 with patch('urllib.request.urlopen',side_effect=AssertionError('Resampling saved response')):
  value2,serial2=obj.ask(messages,2,root,0);assert value2==value and serial2==serial
 assert len(list(root.glob('*.request.json')))==3
out={'completed_game_replays':len(checks),'partial_prefix_decisions':44,'missing_attempts_consumed':True,'first_valid_response_reused':True,'network_calls':0,'mock_requests':1}
c.write_json(c.ROOT/'recovery-selfcheck.json',out);print(json.dumps(out))

import importlib.util,json,urllib.request
from pathlib import Path
root=Path('/Users/amir/Desktop/untitled folder 3/alashi')
spec=importlib.util.spec_from_file_location('debrief',root/'tools/debrief.py');mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
k=mod.llm_key()
assert k, 'Missing existing alashi credential'
body={'model':'glm-4.5-flash','thinking':{'type':'disabled'},'temperature':0.7,'top_p':0.95,'max_tokens':100,'messages':[{'role':'user','content':'Return exactly this JSON: {"action":"pass"}'}]}
req=urllib.request.Request('https://api.z.ai/api/paas/v4/chat/completions',data=json.dumps(body).encode(),headers={'Authorization':'Bearer '+k,'Content-Type':'application/json'},method='POST')
try:
 with urllib.request.urlopen(req,timeout=45) as r:v=json.load(r)
 Path(__file__).with_name('pilot.json').write_text(json.dumps(v,ensure_ascii=False,indent=2))
 print(json.dumps(v,ensure_ascii=False))
except Exception as e: print(type(e).__name__,str(e));raise SystemExit(1)

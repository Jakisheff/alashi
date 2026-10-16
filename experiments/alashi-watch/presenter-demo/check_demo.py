"""Check task dependencies and output metadata without calling any model."""
import json,math,re
from pathlib import Path
from render_map import Map,HERE
m=Map();peak=0;phases=set()
for frame in range(8*264):
 t=9+frame/30
 states=[m.state(u,t) for u in range(8)]
 peak=max(peak,sum(s[0] in ('order','outbound','work','return','handoff') for s in states))
 for u,(phase,p,age) in enumerate(states):
  phases.add(phase);x,y=m.route(u,p)
  assert 0<x<1280 and 0<y<720
  assert 0<=p<=1 and age>=0
  if u%4==3 and phase not in ('idle','done'):
   assert all(states[j][0]=='done' for j in range(u-3,u)),(t,u,states)
assert peak>=6
assert phases=={'idle','order','outbound','work','return','handoff','done'}
if (HERE/'edit.json').exists():
 e=json.loads((HERE/'edit.json').read_text());duration=sum(c['out']-c['in'] for c in e['cuts'])
 assert 110<=duration<=120
 assert e['cuts'][0]['in']<=.66 and e['cuts'][0]['out']>=9.06
 for c in e['cuts']:assert 0<=c['in']<c['out']<=881.77
 for c in e['chapters']:assert 0<=c['start']<c['end']<=120
 for f in ['captions.ass','screen-copy.txt']:
  assert not re.search('[А-Яа-яЁё∀]',(HERE/f).read_text())
print(json.dumps({'passed':True,'peak_parallel_tasks':peak,'verifier_waits_for_reports':True,'sampled_frames':8*264,'phases':sorted(phases)},indent=2))

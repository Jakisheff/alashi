#!/usr/bin/env python3
"""Temporal acceptance: delegation, dependency gates, bounds and exact rewind."""
import hashlib
import json
from pathlib import Path
from film import Film, MAP

root=Path(__file__).resolve().parent
film=Film()
maximum=0
for tick in range(3600):
    t=tick/30
    states=[film.unit_state(i,t) for i in range(8)]
    active=sum(s[0] not in ('idle','done') for s in states)
    maximum=max(maximum,active)
    for unit,state in enumerate(states):
        phase,p,_,_=state
        x,y=film.route(unit,p)
        assert MAP[0]<=x<=MAP[0]+MAP[2] and MAP[1]<=y<=MAP[1]+MAP[3]
        if unit%4==3 and phase not in ('idle','done'):
            assert all(states[(unit//4)*4+r][0]=='done' for r in range(3)),(t,unit)
        if phase=='execute':
            assert states[(unit//4)*4+3][0]=='done',(t,unit)
    _,phase=film.cycle_phase(t)
    if phase=='executed':assert states[3][0]==states[7][0]=='done'
assert maximum==6,maximum
reference=hashlib.sha256(film.frame(25).tobytes()).hexdigest()
film.frame(119);film.frame(3)
assert reference==hashlib.sha256(film.frame(25).tobytes()).hexdigest()
report={'passed':True,'sampled_frames':3600,'maximum_parallel_workers':maximum,
        'checks':['workers finish before verifier starts','verifier finishes before execution','positions stay on the full map','reverse seek renders identical pixels'],
        'limit':'Numerical and image determinism checks do not replace motion review or a new-viewer test'}
(root/'evidence/motion-checks.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))

"""Black-box HTTP checks using the exact upstream RemoteSession client."""
import hashlib
import json
from pathlib import Path
import threading
import time
import urllib.error
import urllib.request
from runtime import World, make_server, RemoteSession, ROOT

world=World();server=make_server(world,0);port=server.server_port
threading.Thread(target=server.serve_forever,daemon=True).start()
threading.Thread(target=world.run,daemon=True).start()
a=RemoteSession(f'http://127.0.0.1:{port}/players/0',timeout=2)
b=RemoteSession(f'http://127.0.0.1:{port}/players/1',timeout=2)
checks={}
try:
    hashes=json.loads((ROOT/'evidence/vendor-hashes.json').read_text())
    checks['unmodified_upstream'] = all(hashlib.sha256((ROOT/'vendor'/name).read_bytes()).hexdigest()==expected for name,expected in hashes.items())
    obs=a.observe()
    checks['own_observation_only']=set(obs['observations'])=={0} and len(obs['observations'][0]['units'])==4
    rejected=a.step({0:[{'command':'assign','unit_id':4},{'command':'assign','unit_id':3},{'command':'accept'}]})['rejected'][0]
    checks['enemy_and_early_verification_rejected']=[r['reason'] for r in rejected]==['unit_not_owned','dependencies_not_complete','verification_not_complete']
    try:a.step({1:[{'command':'assign','unit_id':4}]});checks['other_slot_rejected']=False
    except Exception as e:checks['other_slot_rejected']=getattr(e,'status',None)==400
    for session,side in [(a,0),(b,1)]:
        result=session.step({side:[{'command':'assign','unit_id':side*4+i} for i in range(3)]},60000)
        assert not result['rejected'][side]
    t0=a.observe()['observations'][0]['game_time_seconds']
    # Both clients stop thinking/sending/observing. No spectator exists.
    time.sleep(2)
    later=a.observe()['observations'][0]
    checks['clock_runs_without_clients']=1.8<later['game_time_seconds']-t0<3
    checks['six_tasks_run_in_parallel']=sum(u['phase']=='work' for u in world.snapshot()['units'])==6
    checks['step_did_not_fast_forward']=later['game_time_seconds']<4
    time.sleep(4.1)
    state=world.snapshot()
    checks['tasks_complete_without_viewer']=sum(u['phase']=='done' for u in state['units'])==6
    checks['unassigned_verifiers_still_wait']=state['units'][3]['phase']==state['units'][7]['phase']=='idle'
    checks['reports_do_not_change_cash']=a.observe()['observations'][0]['cash']==100
    assert not a.step({0:[{'command':'assign','unit_id':3}]})['rejected'][0]
    time.sleep(6.1)
    assert not a.step({0:[{'command':'accept'}]})['rejected'][0]
    before=len([e for e in world.events if e['kind']=='accepted'])
    a.step({0:[{'command':'accept'}]})
    checks['accept_idempotent']=before==len([e for e in world.events if e['kind']=='accepted'])==1
    doc=json.load(urllib.request.urlopen(f'http://127.0.0.1:{port}/timeline'))
    checks['timeline_keeps_all_completions']=sum(e['kind']=='done' for f in doc['frames'] for e in f['events'])==7
    checks['timeline_monotonic']=all(a['t']<=b['t'] for a,b in zip(doc['frames'],doc['frames'][1:]))
    reference=urllib.request.urlopen(f'http://127.0.0.1:{port}/replay.html').read()
    checks['upstream_replay_contains_events']=b'window.RTS_DATA =' in reference and b'reports_accepted' not in reference and b'accepted' in reference
    world.reset();checks['reset_clears_reports']=not any(u['result'] for u in world.snapshot()['units'])
    checks['reset_clears_acceptance']=world.snapshot()['accepted']==[False,False]
    result={'checks':checks,'passed':all(checks.values()),'elapsed_seconds':round(time.monotonic()-world.origin,2),'source_commit':'2376c509b6b5691858df2c9d4d1f79a7b1a877d1'}
    (ROOT/'evidence/runtime-checks.json').write_text(json.dumps(result,ensure_ascii=False,indent=2))
    (ROOT/'evidence/runtime-test-timeline.json').write_text(json.dumps(doc,ensure_ascii=False))
    print(json.dumps(result,ensure_ascii=False,indent=2))
    assert result['passed']
finally:
    world.stop.set();server.shutdown();server.server_close()

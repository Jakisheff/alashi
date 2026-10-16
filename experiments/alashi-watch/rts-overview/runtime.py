"""Loopback-only experiment. Scripted tasks, no Alashi arena or model calls.

RemoteSession, Timeline and the reference viewer are unmodified upstream code.
This adapter owns demonstration task state; it is not a game-rules implementation.
"""
from __future__ import annotations

import argparse
import copy
import json
import math
from pathlib import Path
import sys
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / 'vendor'))
from agentenv_rts.session import RemoteSession, OBSERVE, STEP
from agentenv_rts.timeline import Timeline
from agentenv_rts import live

COLORS = ['#a78bfa', '#69c8d3']
ROLES = ['Рынок', 'Варианты', 'Парламент', 'Проверка']
TARGETS = [(-1.4, -2.8), (0, .5), (1.7, 3.1), (0, -4.6)]
RESULTS = ['Продажа 4 товаров: расчёт 39', 'Сравнение вариантов готово', 'Политический риск проверен', 'Три отчёта согласованы']

def smooth(x):
    x = max(0, min(1, x))
    return x * x * (3 - 2 * x)


class World:
    def __init__(self):
        self.lock = threading.RLock()
        self.origin = time.monotonic()
        self.stop = threading.Event()
        self.reset()

    def now(self):
        return time.monotonic() - self.origin

    def reset(self):
        with self.lock:
            self.cycle = getattr(self, 'cycle', 0) + 1
            self.started = self.now()
            self.accepted = [False, False]
            self.hold_until = [0., 0.]
            self.events = []
            self.event_seq = 0
            self.units = []
            for side in range(2):
                for role in range(4):
                    home = [(-6 if side == 0 else 6) + (role % 2) * .85, (-1.3 if side == 0 else 1.1) + (role // 2) * 1.2]
                    tx, tz = TARGETS[role]
                    target = [tx + (-.9 if side == 0 else .9), tz + .7]
                    self.units.append(dict(id=side * 4 + role, owner=side, role=role,
                        home=home, target=target, x=home[0], z=home[1], phase='idle',
                        started=None, duration=5.2 + role * .25, phase_t=0, yaw=0,
                        task_id=f'cycle-{self.cycle}-{side}-{role}', result=None))
            self.timeline = Timeline(dict(game=f'alashi-demo-{self.cycle}', title='Alashi · demo task runtime',
                map='Два штаба · демо', bounds=dict(min_x=-9, max_x=9, min_y=-7, max_y=7),
                players=[dict(slot=s, label=f'Chief {"АБ"[s]}', race='Degenie', controller='scripted demo', color=COLORS[s]) for s in range(2)],
                points=[dict(kind='shop', x=x, y=-z, label=ROLES[i]) for i, (x,z) in enumerate(TARGETS)],
                types={'degenie':'Degenie'}, time_limit=0))
            self.emit(None, 'cycle', 'Новое поручение: подготовить решение Chief')
            self.record()

    def emit(self, side, kind, text, unit=None):
        self.event_seq += 1
        self.events.append(dict(event_id=f'{self.cycle}:{self.event_seq}', t=round(self.now()-self.started, 3),
                                side=side, kind=kind, text=text, unit=unit, major=True))

    def command(self, side, command):
        with self.lock:
            op = command.get('command')
            if op == 'accept':
                if self.units[side*4+3]['phase'] != 'done':
                    raise ValueError('verification_not_complete')
                if not self.accepted[side]:
                    self.accepted[side] = True
                    self.emit(side, 'accepted', f'Chief {"АБ"[side]} принял проверенный расчёт 39')
                    self.record()
                return
            if op != 'assign':
                raise ValueError('unknown_command')
            uid = command.get('unit_id')
            if type(uid) is not int or uid not in range(8) or uid//4 != side:
                raise ValueError('unit_not_owned')
            u = self.units[uid]
            if u['phase'] != 'idle':
                raise ValueError('already_assigned')
            if u['role'] == 3 and not all(x['phase'] == 'done' for x in self.units[side*4:side*4+3]):
                raise ValueError('dependencies_not_complete')
            u['started'] = self.now()
            u['phase'] = 'order'
            self.emit(side, 'assigned', f'{"АБ"[side]}{u["role"]+1}: {ROLES[u["role"]]}', uid)
            self.record()

    def tick(self):
        with self.lock:
            now = self.now()
            for u in self.units:
                if u['started'] is None or u['phase'] == 'done':
                    continue
                t = now-u['started']; total=u['duration']
                old = u['phase']
                if t < .22: phase, p, pt = 'order', 0, t
                elif t < 1.65: phase, p, pt = 'outbound', smooth((t-.22)/1.43), t-.22
                elif t < total-1.7: phase, p, pt = 'work', 1, t-1.65
                elif t < total-.25: phase, p, pt = 'return', 1-smooth((t-(total-1.7))/1.45), t-(total-1.7)
                elif t < total: phase, p, pt = 'handoff', 0, t-(total-.25)
                else: phase, p, pt = 'done', 0, 0
                hx,hz=u['home']; tx,tz=u['target']
                arc = math.sin(math.pi*p) * (.45 if u['owner']==0 else -.45)
                u.update(x=hx+(tx-hx)*p, z=hz+(tz-hz)*p+arc, phase=phase, phase_t=pt)
                if phase in ('order','outbound','return'):
                    u['yaw'] = math.atan2(tx-hx, tz-hz) + (math.pi if phase=='return' else 0)
                elif phase in ('work','handoff','done'):
                    u['yaw'] = 0
                if phase != old and phase in ('work','return','done'):
                    if phase == 'return': u['result'] = RESULTS[u['role']]
                    self.emit(u['owner'], phase, f'{"АБ"[u["owner"]]}{u["role"]+1}: '+dict(work='работает',return_='несёт отчёт',done='отчёт доставлен').get(phase, 'несёт отчёт'),u['id'])
            self.record()

    def record(self):
        if self.timeline.last and self.timeline.last.get('result'):
            return
        events = copy.deepcopy(self.events[getattr(self,'recorded_seq',0):]) if getattr(self,'recorded_cycle',None)==self.cycle else copy.deepcopy(self.events)
        self.recorded_cycle, self.recorded_seq = self.cycle, len(self.events)
        self.timeline.add(dict(t=round(self.now()-self.started, 4),
            units=[[u['id'],u['owner'],'degenie',u['x'],-u['z'],100,3] for u in self.units],
            players={str(s):dict(units=4,score=sum(u['phase']=='done' for u in self.units[s*4:s*4+4])) for s in range(2)},
            events=events, notes=[], result='reports_accepted' if all(self.accepted) else ''))

    def observe(self, side):
        with self.lock:
            return dict(player_id=side, observations={str(side):dict(game_time_seconds=round(self.now()-self.started,3),
                cycle=self.cycle, units=copy.deepcopy(self.units[side*4:side*4+4]), accepted=self.accepted[side],
                cash=100, goods=4, usage=None)}, done=self.accepted[side], scenario={'mode':'realtime', 'source':'scripted-demo'})

    def snapshot(self):
        with self.lock:
            return dict(cycle=self.cycle,t=round(self.now()-self.started,3),units=copy.deepcopy(self.units),
                accepted=self.accepted[:],events=copy.deepcopy(self.events),
                holds=[max(0,round(t-self.now(),1)) for t in self.hold_until], source='scripted-demo')

    def run(self):
        while not self.stop.wait(.05): self.tick()


def policy(world, port, side):
    """Actual upstream HTTP client; deterministic policy, no language model."""
    session=RemoteSession(f'http://127.0.0.1:{port}/players/{side}',timeout=3)
    while not world.stop.wait(.1):
        try:
            if world.now() < world.hold_until[side]: continue
            obs=session.observe()['observations'][side]
            if obs['game_time_seconds'] < .5+side*.3: continue
            units=obs['units']; commands=[]
            for u in units:
                if u['phase']=='idle' and (u['role']<3 or all(v['phase']=='done' for v in units[:3])):
                    commands.append(dict(command='assign',unit_id=u['id']))
            if units[-1]['phase']=='done' and not obs['accepted']: commands.append(dict(command='accept'))
            if commands: session.step({side:commands})
        except Exception as e:
            print(f'policy {side}: {e}',file=sys.stderr)


def make_server(world, port):
    class Handler(SimpleHTTPRequestHandler):
        def __init__(self,*args,**kwargs): super().__init__(*args,directory=str(ROOT),**kwargs)
        def log_message(self,*args): pass
        def reply(self,value,status=200,ctype='application/json'):
            data=json.dumps(value,ensure_ascii=False,allow_nan=False).encode() if ctype=='application/json' else value.encode()
            self.send_response(status);self.send_header('Content-Type',ctype+'; charset=utf-8');self.send_header('Content-Length',str(len(data)))
            self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write(data)
        def player(self,path):
            parts=path.split('/')
            return int(parts[2]) if len(parts)>3 and parts[1]=='players' and parts[2] in ('0','1') else None
        def do_GET(self):
            parsed=urlparse(self.path);p=parsed.path;s=self.player(p)
            if s is not None and p.endswith('/.well-known/agent-env.json'):
                return self.reply(dict(capabilities=dict(extensions=[dict(uri=uri,params={'endpoint':'/'+name}) for uri,name in [(OBSERVE,'observe'),(STEP,'step')]])))
            if p=='/state': return self.reply(world.snapshot())
            if p=='/timeline':
                with world.lock:return self.reply(world.timeline.doc())
            if p=='/reference':return self.reply(live.page(),ctype='text/html')
            if p=='/reference/data.json':
                with world.lock:return self.reply(live.data(world.timeline,parse_qs(parsed.query).get('since',[None])[0]))
            if p=='/replay.html':
                with world.lock:return self.reply(live.standalone(world.timeline),ctype='text/html')
            if p.startswith('/agentenv-wc3-plugin') or p.startswith('/evidence'):return self.reply({'error':'not_exposed'},404)
            super().do_GET()
        def do_POST(self):
            origin=self.headers.get('Origin')
            if origin and origin!=f'http://127.0.0.1:{self.server.server_port}':return self.reply({'error':'origin_refused'},403)
            try:n=int(self.headers.get('Content-Length','0'))
            except ValueError:return self.reply({'error':'invalid_length'},400)
            if not 0<=n<=16384:return self.reply({'error':'invalid_length'},413)
            try:
                data=json.loads(self.rfile.read(n) or b'{}');p=urlparse(self.path).path;s=self.player(p)
                if not isinstance(data,dict):raise ValueError('object_required')
                if p=='/demo/reset':world.reset();return self.reply({'cycle':world.cycle})
                if p=='/demo/delay':
                    with world.lock:
                        world.hold_until[1]=world.now()+5;world.emit(1,'latency','Chief Б думает 5 с; выданные поручения продолжаются')
                    return self.reply({'delay_seconds':5})
                if s is None:return self.reply({'error':'unknown_player'},404)
                if p.endswith('/observe'):return self.reply(world.observe(s))
                if not p.endswith('/step'):return self.reply({'error':'unknown_endpoint'},404)
                actions=data.get('actions',{})
                if not isinstance(actions,dict) or any(k!=str(s) for k in actions):raise ValueError('slot_not_owned')
                batch=actions.get(str(s),[])
                if not isinstance(batch,list) or len(batch)>8:raise ValueError('invalid_batch')
                rejected=[]
                for i,cmd in enumerate(batch):
                    try:
                        if not isinstance(cmd,dict):raise ValueError('invalid_command')
                        world.command(s,cmd)
                    except ValueError as e:rejected.append({'index':i,'reason':str(e)})
                return self.reply({**world.observe(s),'rejected':{str(s):rejected},'placements':{},'elapsed_ms':0})
            except (ValueError,TypeError) as e:return self.reply({'error':{'code':'invalid','message':str(e)}},400)
    return ThreadingHTTPServer(('127.0.0.1',port),Handler)


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--port',type=int,default=8767);args=parser.parse_args()
    world=World();server=make_server(world,args.port)
    threading.Thread(target=world.run,daemon=True).start()
    for s in range(2):threading.Thread(target=policy,args=(world,args.port,s),daemon=True).start()
    print(f'http://127.0.0.1:{args.port}',flush=True)
    try:server.serve_forever()
    finally:world.stop.set();server.server_close()

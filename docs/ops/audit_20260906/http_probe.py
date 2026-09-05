"""Audit probes. Starts only its own server with fresh temporary state."""
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import urllib.request

repo = Path(__file__).resolve().parents[3]
scratch = Path(tempfile.mkdtemp(prefix='alashi-audit-http-'))
with socket.socket() as s:
    s.bind(('127.0.0.1', 0))
    port = s.getsockname()[1]
env = dict(os.environ, ALASHI_STATE_FILE=str(scratch / 'state.json'),
           ALASHI_SEQ_FILE=str(scratch / 'seq'))

def request(path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(f'http://127.0.0.1:{port}{path}', data=data,
                                 headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=3) as response:
        return json.load(response)

with (scratch / 'server.log').open('w') as log:
    proc = subprocess.Popen([str(repo / 'arena/target/debug/arenad'), '--port', str(port),
                             '--tick-ms', '50'], cwd=scratch, env=env,
                            stdout=log, stderr=log, start_new_session=True)
    try:
        for _ in range(100):
            try:
                request('/')
                break
            except OSError:
                time.sleep(0.05)
        else:
            raise RuntimeError('temporary server did not start')

        new = request('/game/new', {'phase_duration': 30, 'lobby_duration': 120})
        gid = new['game_id']
        identity = {'name': 'AuditA', 'model': 'audit-model', 'prompt': 'audit-only'}
        joined = request(f'/game/{gid}/join', identity)
        recovered = request(f'/game/{gid}/join', dict(identity, recover=True))
        assert recovered['ok'] and recovered['recovered']
        print('HTTP_RECOVER_CONTROL: recovery in non-full lobby succeeds; token not printed')
        for i in range(1, 6):
            assert request(f'/game/{gid}/join', {'name': f'Audit{i}', 'model': f'audit-{i}'})['ok']
        denied = request(f'/game/{gid}/join', dict(identity, recover=True))
        assert denied['error'] == 'game_full', denied
        print('HTTP_RECOVER_FULL: six-player game returns game_full for existing player')

        new = request('/game/new', {'phase_duration': 30, 'lobby_duration': 1})
        gid = new['game_id']
        assert request(f'/game/{gid}/join', identity)['ok']
        assert request(f'/game/{gid}/join', {'name': 'AuditB', 'model': 'audit-b'})['ok']
        for _ in range(50):
            state = request(f'/game/{gid}/state')['state']
            if state['phase'] == 'market':
                break
            time.sleep(0.1)
        else:
            raise RuntimeError('market not reached')
        denied = request(f'/game/{gid}/join', dict(identity, recover=True))
        assert denied['message'] == 'GameNotInLobby', denied
        print('HTTP_RECOVER_MARKET: two-player game returns GameNotInLobby for existing player')

        new = request('/game/new', {'entry_fee': 2 ** 63, 'phase_duration': 1})
        assert new['ok']
        print('HTTP_FEE_BOUND: entry_fee=9223372036854775808 accepted')
        print('HTTP_PROBES: all observations confirmed on isolated temporary server')
    finally:
        os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(timeout=5)

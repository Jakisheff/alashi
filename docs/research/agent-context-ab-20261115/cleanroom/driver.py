#!/usr/bin/env python3
"""Clean-room blind HTTP probe: three isolated arenas, three context arms.

Arms (the agent binary's built-in context is constant; the --prompt file adds):
  naked      - no extra file
  quickstart - docs/QUICKSTART_JURY.md
  skill      - ../arms/skill/memory.md (compact procedure)

Two --no-llm greedy opponents per game. The blind agent uses glm-5.3 via the
configured ~/.config/alashi/llm.json key. The logging proxy counts 4xx.

Run: python3 driver.py   (writes summary.json; ~6-8 minutes)
"""

import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[3]
ARENA_BIN = str(REPO / 'arena/target/release/arenad')
AGENT_BIN = str(REPO / 'arena/target/release/agent')
ARMS = {
    'naked': None,
    'quickstart': str(REPO / 'docs/QUICKSTART_JURY.md'),
    'skill': str(ROOT.parent / 'arms/skill/memory.md'),
}
BASE_PORT = 8150
PROC = []
SHIM_PORT = 8190


def http(port, method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(f'http://127.0.0.1:{port}{path}', data=data, method=method)
    if data:
        request.add_header('Content-Type', 'application/json')
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, json.loads(error.read() or b'{}')


def launch(port, tag):
    arena_port, proxy_port = port, port + 1
    log = ROOT / f'proxy-{tag}.jsonl'
    if log.exists():
        log.unlink()
    PROC.append(subprocess.Popen([ARENA_BIN, '--port', str(arena_port)],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    time.sleep(0.8)
    PROC.append(subprocess.Popen([sys.executable, str(ROOT / 'proxy.py'), str(proxy_port),
                                  str(arena_port), str(log)],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    time.sleep(0.5)
    return proxy_port, log


def spawn(args, tag, extra_env=None):
    out = open(ROOT / f'agent-{tag}.log', 'w')
    env = dict(os.environ)
    if extra_env:
        env.update(extra_env)
    proc = subprocess.Popen(args, stdout=out, stderr=subprocess.STDOUT, env=env)
    PROC.append(proc)
    return proc


def main():
    results = {}
    shim_env = {'ALASHI_TEST_MOCK_LLM_BASE': f'http://127.0.0.1:{SHIM_PORT}'}
    PROC.append(subprocess.Popen([sys.executable, str(ROOT / 'llm_shim.py'), str(SHIM_PORT)],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL))
    time.sleep(0.6)
    handles = {}
    for index, (arm, prompt) in enumerate(ARMS.items()):
        port = BASE_PORT + index * 2
        proxy_port, log = launch(port, arm)
        status, body = http(proxy_port, 'POST', '/game/new',
                            {'epoch': '90s', 'entry_fee': 10000000, 'phase_duration': 15,
                             'lobby_duration': 20, 'grace_s': 3})
        assert status == 200 and body.get('ok'), (arm, status, body)
        game = body['game_id']
        common = ['--url', f'http://127.0.0.1:{proxy_port}', '--game', str(game), '--no-llm']
        spawn([AGENT_BIN, *common, '--name', f'GreedA{index}'], f'{arm}-op1')
        spawn([AGENT_BIN, *common, '--name', f'GreedB{index}'], f'{arm}-op2')
        blind = [AGENT_BIN, '--url', f'http://127.0.0.1:{proxy_port}', '--game', str(game),
                 '--model', 'glm-5.3', '--name', f'Blind{arm[:3]}']
        if prompt:
            blind += ['--prompt', prompt]
        handles[arm] = spawn(blind, arm, extra_env=shim_env)
        results[arm] = {'game': game, 'proxy_port': proxy_port, 'log': log.name}
        print(f'{arm}: game {game} on {proxy_port}', flush=True)

    for arm, proc in handles.items():
        try:
            proc.wait(timeout=780)
        except subprocess.TimeoutExpired:
            print(f'{arm}: blind agent did not exit in time', flush=True)

    for arm, info in results.items():
        entries = [json.loads(line) for line in open(ROOT / info['log'])]
        errors4xx = [e for e in entries if 400 <= e['status'] < 500]
        arena_err = [e for e in entries if e.get('arena_error')]
        errors5xx = [e for e in entries if e['status'] >= 500]
        results[arm]['requests'] = len(entries)
        results[arm]['4xx'] = len(errors4xx)
        results[arm]['arena_rejections'] = len(arena_err)
        results[arm]['arena_rejection_kinds'] = [f"{e['method']} {e['path'].split('?')[0]} -> {e['arena_error']}" for e in arena_err][:30]
        results[arm]['5xx'] = len(errors5xx)

    (ROOT / 'summary.json').write_text(json.dumps(results, ensure_ascii=False, indent=2) + '\n')
    for arm, info in results.items():
        print(f"{arm}: requests={info['requests']} 4xx={info['4xx']} "
              f"arena_rejections={info['arena_rejections']} 5xx={info['5xx']}")
    for proc in PROC:
        proc.terminate()


if __name__ == '__main__':
    main()

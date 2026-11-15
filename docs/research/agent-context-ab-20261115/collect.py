#!/usr/bin/env python3
"""Paired-seed context-arm collection: naked vs quickstart vs skill.

Same durable game loop as harness-learning-20261020/collect.py; arms map to
arms/<name>/ (memory.md + helper.py), a single test partition, seat 0,
opponents greedy+tactical, epoch 1, vote_weight_mode 1.

Usage: python3 collect.py [max_seeds]   (max_seeds limits seed_index for the pilot)
"""
import concurrent.futures, hashlib, json, os, shutil, subprocess, sys, threading, time
from pathlib import Path
import common as c
from model_api import API
from discover import execute
ROOT = Path(__file__).resolve().parent
ARMS = ['naked', 'quickstart', 'skill']

def read(path): return json.loads(path.read_text())
def content_hash(messages): return hashlib.sha256(json.dumps(messages, ensure_ascii=False, separators=(',', ':')).encode()).hexdigest()
def history_row(state, action):
    seen = [[c, g, i] for c, g, i in zip(state['cash'], state['goods'], state['influence'])] if 'cash' in state else None
    return {'round': state['round'], 'phase': state['phase'], 'stage': state.get('decision_stage'),
            'seen': seen, 'is_president': state.get('is_president'), 'law_id': state.get('law_card'), 'action': action}
def harness_for(arm): return ROOT / 'arms' / arm
def render(state, history, harness):
    messages = c.render(state, history, 'президент', 'фракция')
    memory = (harness / 'memory.md').read_text()
    helper = (harness / 'helper.py').read_text()
    features = execute(helper, 'helper', state=state, history=history)
    if not features['ok']: raise RuntimeError('Harness execution failed: ' + features.get('error', ''))
    if memory.strip():
        messages[0]['content'] += '\n\nДополнение контекста проверяемой руки:\n' + memory
    user = json.loads(messages[1]['content']); user['harness_context'] = features['value']
    messages[1]['content'] = json.dumps(user, ensure_ascii=False, separators=(',', ':'))
    return messages, features['value']

class Collector:
    def __init__(self, protocol, jobs):
        self.protocol = protocol; self.jobs = jobs; self.root = ROOT / 'runs'; self.root.mkdir(parents=True, exist_ok=True)
        self.api = API(ROOT / 'game-calls', protocol['collection_limits']); self.lock = threading.Lock()
        self.completed = self.failed = 0
        for job in self.jobs:
            p = self.root / job['id'] / 'result.json'
            if p.exists() and read(p)['status'] == 'completed': self.completed += 1
    def progress(self):
        return {'completed': self.completed, 'failed': self.failed, 'planned': len(self.jobs), 'api': self.api.stats()}
    def ask(self, messages, job, index):
        body = {**self.protocol['executor_settings'], 'messages': messages}; repaired = False
        for attempt in range(3):
            tag = f"{job['id']}-s{index:02d}-a{attempt}"
            try: raw, serial = self.api.call(tag, body)
            except RuntimeError:
                if self.api.stop.is_set() or attempt == 2: raise
                time.sleep(2); continue
            try: return c.parse(raw), serial, tag
            except (AssertionError, ValueError, KeyError, TypeError) as error:
                c.write_json(ROOT / 'game-calls' / (tag + '.validation-error.json'), {'error': str(error), 'expected': 'one JSON object {"action": str, ...}'})
                if repaired or attempt == 2 or raw.get('model') != 'glm-5.3': raise RuntimeError('Invalid model action; no action substituted') from error
                repaired = True
                body = {**self.protocol['executor_settings'], 'messages': messages + [{'role': 'user', 'content': 'Ошибка формата предыдущего ответа. Верни один JSON-объект {"action":"<имя>","reason":"..."}. Имена действий и параметры — из системных инструкций.'}]}
        raise RuntimeError('Provider attempts exhausted')
    def game(self, job):
        folder = self.root / job['id']; folder.mkdir(exist_ok=True)
        if (folder / 'job.json').exists(): assert read(folder / 'job.json') == job
        else: c.write_json(folder / 'job.json', job)
        if (folder / 'result.json').exists() and read(folder / 'result.json')['status'] == 'completed': return read(folder / 'result.json')
        saved = [json.loads(s) for s in (folder / 'trace.jsonl').read_text().splitlines()] if (folder / 'trace.jsonl').exists() else []
        saved_bytes = (folder / 'trace.jsonl').read_bytes() if (folder / 'trace.jsonl').exists() else b''
        trace = []; history = []; errors = []; proc = None; started = time.monotonic()
        harness = harness_for(job['arm'])
        fingerprint = {n: c.sha(harness / n) for n in ['memory.md', 'helper.py']}
        result = {**job, 'status': 'failed', 'started_utc': c.utc(), 'harness': fingerprint}
        try:
            self.api.guard()
            with (folder / 'engine.stderr').open('a') as err, (folder / 'trace.jsonl').open('a') as logfile:
                proc = subprocess.Popen([str(ROOT / 'bin/evalgame')], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err, text=True, bufsize=1)
                proc.stdin.write(json.dumps({'seed': job['seed'], 'seat': job['seat'], 'opponents': job['opponents'], 'builtin': None, 'epoch': 1, 'vote_weight_mode': 1}) + '\n'); proc.stdin.flush()
                while True:
                    self.api.guard(); line = proc.stdout.readline()
                    assert line, 'Engine closed without result'; msg = json.loads(line)
                    if msg['type'] == 'request':
                        state = c.observation(msg['observation'], msg['phase'])
                        messages, features = render(state, history, harness)
                        if len(trace) < len(saved):
                            row = saved[len(trace)]
                            assert row['state'] == state and row['features'] == features and row['messages_sha256'] == content_hash(messages)
                            if row.get('request_tag'):
                                request = read(ROOT / 'game-calls' / (row['request_tag'] + '.request.json'))
                                raw = read(ROOT / 'game-calls' / (row['request_tag'] + '.response.json'))['raw']
                                assert request['body']['messages'][:2] == messages and c.parse(raw) == row['action']
                        else:
                            value, serial, tag = self.ask(messages, job, len(trace)); source = 'glm-5.3'
                            row = {'index': len(trace), 'state': state, 'features': features, 'messages_sha256': content_hash(messages), 'action': value, 'sent': value, 'reason': value.get('reason'), 'source': source, 'request_tag': tag, 'request_serial': serial}
                            logfile.write(json.dumps(row, ensure_ascii=False) + '\n'); logfile.flush(); os.fsync(logfile.fileno())
                        trace.append(row); history.append(history_row(state, row['action']))
                        proc.stdin.write(json.dumps(row['action']) + '\n'); proc.stdin.flush()
                    elif msg['type'] == 'decision':
                        if msg.get('parse_error') is not None:
                            errors.append({'phase': msg['phase'], 'decision': msg['decision'], 'parse_error': msg['parse_error']})
                    elif msg['type'] == 'result':
                        game = msg['game']; assert len(game['phases']) == 18 and len(game['payouts']) == 3
                        own = [a for phase in game['phases'] for a in phase['actions'] if a['actor'] == job['seat']]
                        illegal = [a for a in own if not a['ok']]
                        if (folder / 'game.json').exists(): assert read(folder / 'game.json') == game
                        else: c.write_json(folder / 'game.json', game)
                        proc.wait(timeout=10); assert proc.returncode == 0
                        result.update(status='completed', payout=game['payouts'][job['seat']], net_profit=game['payouts'][job['seat']] - game['entry_fee'], rank=game['ranks'].index(job['seat']) + 1, decisions=len(trace), model_decisions=len(trace), parse_errors=errors, illegal_actions=len(illegal), illegal_detail=[{'phase': a['phase'], 'err': a.get('err'), 'detail': a.get('detail')} for a in illegal[:10]]); break
        except Exception as error: result.update(error=type(error).__name__ + ': ' + str(error), decisions=len(trace))
        finally:
            if proc is not None and proc.poll() is None: proc.kill(); proc.wait(timeout=10)
            if proc is not None: proc.stdin.close(); proc.stdout.close()
            assert (folder / 'trace.jsonl').read_bytes().startswith(saved_bytes) if (folder / 'trace.jsonl').exists() else not saved_bytes
            result['elapsed_s'] = round(time.monotonic() - started, 3); c.write_json(folder / 'result.json', result)
            with self.lock:
                self.completed += result['status'] == 'completed'; self.failed += result['status'] != 'completed'
                c.write_json(ROOT / 'progress.json', self.progress())
                with (ROOT / 'collection-events.jsonl').open('a') as log:
                    log.write(json.dumps(result, ensure_ascii=False) + '\n'); log.flush(); os.fsync(log.fileno())
                print(self.completed, '/', len(self.jobs), 'complete;', self.failed, 'failed;', job['id'], result['status'], 'calls', self.api.calls, flush=True)
        return result

def main(max_seeds=None):
    protocol = read(ROOT / 'protocol.json')
    for name, h in protocol['hashes'].items(): assert c.sha(ROOT / name) == h, name
    assert c.sha(ROOT / 'bin/evalgame') == protocol['engine_binary_sha256']
    assert shutil.disk_usage(ROOT).free > 512 * 1024 * 1024, 'Need at least 512 MiB free before starting'
    jobs = protocol['jobs']
    if max_seeds is not None: jobs = [j for j in jobs if j['seed_index'] < max_seeds]
    campaign = Collector(protocol, jobs)
    with concurrent.futures.ThreadPoolExecutor(max_workers=protocol['workers']) as pool:
        results = list(pool.map(campaign.game, jobs))
    c.write_json(ROOT / 'game-calls/usage.json', campaign.api.stats())
    c.write_json(ROOT / 'finished.json', {'finished_utc': c.utc(), **campaign.progress(), 'statuses': {s: sum(r['status'] == s for r in results) for s in ['completed', 'failed']}})

if __name__ == '__main__':
    main(int(sys.argv[1]) if len(sys.argv) > 1 else None)

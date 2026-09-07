#!/usr/bin/env python3
"""Paired local Alashi evaluation. Standard library only; no live arena or network client."""
import argparse
import collections
import datetime
import hashlib
import html
import json
import math
import os
import platform
from pathlib import Path
import random
import selectors
import secrets
import signal
import statistics
import subprocess
import sys
import tempfile
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BUILTINS = {'greedy', 'random', 'tactical'}


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + '\n')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_config(path):
    c = json.loads(path.read_text())
    fields = {'schema_version', 'baseline', 'candidate', 'opponents', 'seeds', 'auto_seed_count',
              'epoch', 'vote_weight_mode', 'decision_timeout_s', 'game_timeout_s',
              'bootstrap_samples', 'bootstrap_seed'}
    if set(c) - fields or c.get('schema_version') != 1:
        raise ValueError('unknown config fields or schema_version != 1')
    for side in ('baseline', 'candidate'):
        v = c[side]
        if set(v) - {'label', 'builtin', 'command', 'artifacts'}:
            raise ValueError('unknown variant field')
        if not isinstance(v.get('label'), str) or not v['label'] or len(v['label']) > 100:
            raise ValueError('variant label must contain 1..100 characters')
        if ('builtin' in v) == ('command' in v):
            raise ValueError('each variant needs exactly one of builtin/command')
        if 'builtin' in v and v['builtin'] not in BUILTINS:
            raise ValueError('unknown builtin')
        if 'command' in v:
            if not isinstance(v['command'], list) or not v['command'] or not all(isinstance(a, str) and a for a in v['command']):
                raise ValueError('command must be a nonempty argv list (no shell)')
        for artifact in v.get('artifacts', []):
            if not isinstance(artifact, str) or not (ROOT / artifact).is_file():
                raise ValueError('artifact does not exist')
    opponents = c['opponents']
    if not isinstance(opponents, list) or not 1 <= len(opponents) <= 5 or any(v not in BUILTINS for v in opponents):
        raise ValueError('1..5 builtin opponents required')
    seeds = c['seeds']
    if seeds == 'auto':
        count = c.pop('auto_seed_count', 3)
        if type(count) is not int or not 1 <= count <= 1000:
            raise ValueError('auto_seed_count must be 1..1000')
        picked = []
        while len(picked) < count:
            s = secrets.randbits(64)
            if s not in picked:
                picked.append(s)
        c['seeds'] = picked
        c['seeds_source'] = 'auto_os_u64'
    else:
        if 'auto_seed_count' in c:
            raise ValueError('auto_seed_count requires seeds "auto"')
        if not isinstance(seeds, list) or not seeds or len(seeds) > 1000:
            raise ValueError('provide 1..1000 unique seeds or the string "auto"')
        if any(type(s) is not int or not 0 <= s < 2**64 for s in seeds) or len(set(seeds)) != len(seeds):
            raise ValueError('seeds must be unique u64 integers')
    for key, default in [('epoch', 1), ('vote_weight_mode', 1)]:
        c.setdefault(key, default)
        if type(c[key]) is not int or c[key] not in (0, 1):
            raise ValueError(f'{key} must be 0 or 1')
    for key, default, upper in [('decision_timeout_s', 1, 30), ('game_timeout_s', 55, 60)]:
        c.setdefault(key, default)
        if type(c[key]) not in (int, float) or not math.isfinite(c[key]) or not 0 < c[key] <= upper:
            raise ValueError(f'{key} must be positive and <= {upper}')
    c.setdefault('bootstrap_samples', 2000)
    c.setdefault('bootstrap_seed', 20260907)
    if type(c['bootstrap_samples']) is not int or not 100 <= c['bootstrap_samples'] <= 10000:
        raise ValueError('bootstrap_samples must be 100..10000')
    if type(c['bootstrap_seed']) is not int:
        raise ValueError('bootstrap_seed must be integer')
    return c


def kill_group(proc):
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    except PermissionError:
        # macOS may deny signalling a group whose shell already exited.
        # Reap it before deciding whether a direct child kill is needed.
        if proc.poll() is None:
            try:
                proc.kill()
            except ProcessLookupError:
                pass
    proc.wait(timeout=5)


def call_policy(variant, request, timeout):
    """Run an explicitly configured local executable. Not a sandbox. Never copy environment."""
    start = time.monotonic()
    trace = {'source': 'external_unverified', 'error': None, 'cost_usd': None}
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        proc = None
        try:
            proc = subprocess.Popen(variant['command'], cwd=ROOT, stdin=subprocess.PIPE,
                                    stdout=output, stderr=errors, start_new_session=True)
            proc.communicate(json.dumps(request).encode(), timeout=timeout)
            if proc.returncode:
                raise ValueError(f'policy_exit_{proc.returncode}')
            if output.tell() > 65536:
                raise ValueError('policy_output_too_large')
            output.seek(0)
            response = json.load(output)
            if not isinstance(response, dict) or not isinstance(response.get('action'), str):
                raise ValueError('policy_response_requires_action')
            cost = response.get('cost_usd')
            if cost is not None:
                if type(cost) not in (int, float) or not math.isfinite(cost) or cost < 0:
                    raise ValueError('invalid_reported_cost')
                trace['cost_usd'] = cost
            if response.get('source') == 'fallback':
                trace['source'] = 'declared_fallback'
            trace['response'] = response
        except subprocess.TimeoutExpired:
            kill_group(proc)
            trace.update(source='fallback', error='policy_timeout')
            response = {'action': 'pass'}
        except (OSError, ValueError) as e:
            trace.update(source='fallback', error=str(e))
            response = {'action': 'pass'}
        finally:
            if proc is not None:
                # Also stop descendants after a successful parent exits.
                kill_group(proc)
    trace['elapsed_ms'] = (time.monotonic() - start) * 1000
    trace['response'] = response
    return response, trace


class Lines:
    def __init__(self, pipe):
        self.pipe = pipe
        self.buffer = b''
        self.selector = selectors.DefaultSelector()
        self.selector.register(pipe, selectors.EVENT_READ)

    def read(self, deadline):
        while b'\n' not in self.buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not self.selector.select(remaining):
                raise TimeoutError('game_timeout')
            chunk = os.read(self.pipe.fileno(), 65536)
            if not chunk:
                raise ValueError('engine_closed_before_result')
            self.buffer += chunk
            if len(self.buffer) > 4_000_000:
                raise ValueError('engine_message_too_large')
        line, self.buffer = self.buffer.split(b'\n', 1)
        return json.loads(line)

    def close(self):
        self.selector.close()


def run_game(binary, c, side, seed, seat, folder):
    variant = c[side]
    cfg = {key: c[key] for key in ('opponents', 'epoch', 'vote_weight_mode')}
    cfg.update(seed=seed, seat=seat, builtin=variant.get('builtin'))
    start = time.monotonic()
    match_id = uuid.uuid4().hex
    result = {'side': side, 'seed': seed, 'seat': seat, 'match_id': match_id,
              'status': 'failed', 'error': None}
    trace = []
    proc = None
    lines = None
    pending = None
    with (folder / 'engine.stderr').open('wb') as stderr, (folder / 'trace.jsonl').open('w') as log:
        try:
            proc = subprocess.Popen([str(binary)], cwd=ROOT, stdin=subprocess.PIPE,
                                    stdout=subprocess.PIPE, stderr=stderr, start_new_session=True)
            proc.stdin.write((json.dumps(cfg) + '\n').encode())
            proc.stdin.flush()
            lines = Lines(proc.stdout)
            deadline = start + c['game_timeout_s']
            while True:
                event = lines.read(deadline)
                if event['type'] == 'request':
                    timeout = min(c['decision_timeout_s'], deadline - time.monotonic())
                    if timeout <= 0:
                        raise TimeoutError('game_timeout')
                    response, pending = call_policy(variant, {
                        'schema_version': 2, 'match_id': match_id,
                        'decision_index': len(trace), 'phase': event['phase'],
                        'rules': {'epoch': c['epoch'], 'vote_weight_mode': c['vote_weight_mode'],
                                  'entry_fee': 10000000, 'market_exec': 'sequential'},
                        'observation': event['observation'],
                        'history': trace,
                    }, timeout)
                    pending.update(phase=event['phase'], observation=event['observation'])
                    proc.stdin.write((json.dumps(response) + '\n').encode())
                    proc.stdin.flush()
                elif event['type'] == 'decision':
                    row = dict(event)
                    row.update(pending or {'source': 'builtin', 'error': None, 'cost_usd': 0})
                    if event.get('parse_error'):
                        row.update(source='fallback', error=event['parse_error'])
                    trace.append(row)
                    log.write(json.dumps(row, ensure_ascii=False) + '\n')
                    log.flush()
                    pending = None
                elif event['type'] == 'result':
                    game = event['game']
                    validate_game(game, seat)
                    write_json(folder / 'game.json', game)
                    result.update(status='completed', game=game)
                    proc.wait(timeout=max(0.01, deadline - time.monotonic()))
                    if proc.returncode != 0:
                        raise ValueError('engine_nonzero_exit')
                    break
                else:
                    raise ValueError('unknown_engine_event')
        except (OSError, ValueError, TimeoutError, subprocess.TimeoutExpired) as e:
            result.update(status='failed', error=str(e))
            if pending is not None:
                # The call may have incurred cost even if the engine never acknowledged it.
                pending.update(type='unacknowledged', decision=None)
                trace.append(pending)
                log.write(json.dumps(pending, ensure_ascii=False) + '\n')
                log.flush()
            print(f"[ERROR] {side} seed={seed} seat={seat}: {e}", file=sys.stderr, flush=True)
        finally:
            if proc is not None:
                kill_group(proc)
                try:
                    proc.stdin.close()
                except BrokenPipeError:
                    pass
                proc.stdout.close()
            if lines is not None:
                lines.close()
    result['elapsed_s'] = time.monotonic() - start
    result['trace'] = trace
    result['diagnostics'] = diagnostics(result)
    write_json(folder / 'result.json', {k: v for k, v in result.items() if k not in ('game', 'trace')})
    return result


def validate_game(game, seat):
    n = game['n_factions']
    if game['strategies'][seat] != 'subject' or sorted(game['ranks']) != list(range(n)):
        raise ValueError('invalid_subject_or_ranking')
    if len(game['payouts']) != n or any(type(x) is not int or x < 0 for x in game['payouts']):
        raise ValueError('invalid_payouts')
    rent = (game.get('license') or {}).get('rent', 0)
    if sum(game['payouts']) - rent + game['rake'] != game['bank']:
        raise ValueError('settlement_does_not_balance')


def diagnostics(result):
    traces = result['trace']
    errors = collections.Counter(t['error'] for t in traces if t.get('error'))
    sources = collections.Counter(t['source'] for t in traces)
    actions = [a for p in result.get('game', {}).get('phases', []) for a in p['actions'] if a['actor'] == result['seat']]
    return {'decisions': len(traces), 'sources': dict(sources), 'call_errors': dict(errors),
            'rejected_actions': sum(not a['ok'] for a in actions),
            'rule_errors': dict(collections.Counter(a['err'] for a in actions if not a['ok'])),
            'known_cost_usd': sum(t['cost_usd'] for t in traces if t.get('cost_usd') is not None),
            'unknown_cost_calls': sum(t.get('cost_usd') is None for t in traces),
            'decision_ms_median': statistics.median(t['elapsed_ms'] for t in traces) if traces else None}


def compare(results, c):
    lookup = {(r['seed'], r['seat'], r['side']): r for r in results}
    pairs, clusters = [], []
    seats = len(c['opponents']) + 1
    for seed in c['seeds']:
        deltas = []
        for seat in range(seats):
            a, b = (lookup[(seed, seat, side)] for side in ('baseline', 'candidate'))
            pair = {'seed': seed, 'seat': seat, 'included': a['status'] == b['status'] == 'completed'}
            if pair['included']:
                av, bv = a['game']['payouts'][seat], b['game']['payouts'][seat]
                pair.update(baseline_payout=av, candidate_payout=bv, delta=bv-av,
                            baseline_rank=a['game']['ranks'].index(seat)+1,
                            candidate_rank=b['game']['ranks'].index(seat)+1)
                deltas.append(bv-av)
            pairs.append(pair)
        # Missing seats cannot silently unbalance a seed cluster.
        if len(deltas) == seats:
            clusters.append(statistics.mean(deltas))
    interval = None
    if len(clusters) >= 2:
        rng = random.Random(c['bootstrap_seed'])
        samples = sorted(statistics.mean(rng.choices(clusters, k=len(clusters))) for _ in range(c['bootstrap_samples']))
        interval = [samples[int((len(samples)-1)*p)] for p in (0.025, 0.975)]
    complete = [p for p in pairs if p['included']]
    return {'pairs': pairs, 'planned_pairs': len(pairs), 'completed_pairs': len(complete),
            'complete_seed_clusters': len(clusters),
            'mean_delta_complete_clusters': statistics.mean(clusters) if clusters else None,
            'cluster_bootstrap_95': interval,
            'delta_range_completed_pairs': [min(p['delta'] for p in complete), max(p['delta'] for p in complete)] if complete else None,
            'interpretation': 'Descriptive local comparison, not proof of general model improvement or market demand.'}


def evidence(results):
    """Observed episodes only. No invented counterfactual payout or adaptation score."""
    out = []
    for r in results:
        if r['status'] != 'completed':
            continue
        phases = r['game']['phases']
        events = []
        for i, ph in enumerate(phases):
            for j, a in enumerate(ph['actions']):
                if a['actor'] == r['seat'] and (not a['ok'] or a['action'] in ('bribe', 'veto', 'bid', 'bid_license')):
                    events.append({'kind': 'action', 'pointer': f'/phases/{i}/actions/{j}', 'round': ph['round'], 'action': a})
            if ph.get('law_passed'):
                law_decision = next((t for t in r['trace'] if t['phase'] == 'law'
                                     and t['observation']['round'] == ph['round']), None)
                following = next((t for t in r['trace'] if t['observation']['round'] > ph['round']), None)
                events.append({'kind': 'after_passed_law', 'pointer': f'/phases/{i}', 'round': ph['round'],
                               'law_card': law_decision['observation']['law_card'] if law_decision else None,
                               'next_observed_decision': following})
        out.append({'side': r['side'], 'seed': r['seed'], 'seat': r['seat'], 'events': events})
    return out


def report(results, summary, c, out):
    def fmt(v):
        return 'не измерено' if v is None else f'{v / 1_000_000:.3f}'
    lines = ['Сравнение агентов Alashi', '',
             f"Вариант A: {c['baseline']['label']}. Вариант B: {c['candidate']['label']}.", '',
             f"Завершено пар: {summary['completed_pairs']} из {summary['planned_pairs']}. Полных групп seed: {summary['complete_seed_clusters']}.",
             f"Среднее изменение выплаты B - A: {fmt(summary['mean_delta_complete_clusters'])} игровых песо."]
    ci = summary['cluster_bootstrap_95']
    lines += [f"95% bootstrap-интервал по группам seed: {fmt(ci[0])} ... {fmt(ci[1])}." if ci else 'Для интервала недостаточно полных групп seed.', '',
              'Позиции внутри одного seed зависимы. Интервал строится по средним полных групп; пары с ошибками остаются в журнале. Это исследовательская оценка на выбранных соперниках, не доказательство общего улучшения модели.', '',
              'Диагностика исполнения:']
    for side in ('baseline', 'candidate'):
        rr = [r for r in results if r['side'] == side]
        errors = sum(sum(r['diagnostics']['call_errors'].values()) for r in rr)
        fallbacks = sum(sum(v for k,v in r['diagnostics']['sources'].items() if 'fallback' in k) for r in rr)
        rejects = sum(r['diagnostics']['rejected_actions'] for r in rr)
        cost = sum(r['diagnostics']['known_cost_usd'] for r in rr)
        unknown = sum(r['diagnostics']['unknown_cost_calls'] for r in rr)
        elapsed = sum(r['elapsed_s'] for r in rr)
        lines.append(f'{side}: ошибки вызовов {errors}; fallback {fallbacks}; отказы правил {rejects}; время процессов {elapsed:.2f} с; известный расход вызовов ${cost:.6f}; вызовов без цены {unknown}.')
    lines += ['', 'Стоимость внешних вызовов заявляет сам адаптер; она не сверена со счётом провайдера. Нулевой расход встроенной эвристики не включает электричество и труд. Виртуальные часы симулятора не воспроизводят гонку сетевых задержек; таймауты проверяют адаптер, а не живую доставку.', '',
              'Наблюдаемые эпизоды и сопоставление по посадке:']
    by_key = {(r['seed'],r['seat'],r['side']):r for r in results}
    for p in summary['pairs']:
        prefix = f"seed={p['seed']}, место={p['seat']}"
        if not p['included']:
            lines.append(prefix + ': незавершённая пара; результаты и ошибки сохранены.')
            continue
        lines.append(prefix + f": выплата A {fmt(p['baseline_payout'])}, B {fmt(p['candidate_payout'])}, изменение {fmt(p['delta'])}; ранг A {p['baseline_rank']}, B {p['candidate_rank']}.")
        left = by_key[(p['seed'], p['seat'], 'baseline')]['trace']
        right = by_key[(p['seed'], p['seat'], 'candidate')]['trace']
        for index, (a, b) in enumerate(zip(left, right)):
            if a['decision'] != b['decision']:
                lines.append(f"  Первое различие действий: r{a['observation']['round']} {a['phase']}: A {a['decision']}; B {b['decision']} (trace.jsonl, строка {index+1}). Состояния доступны в той же строке; дальнейшая траектория может отличаться.")
                break
        for side in ('baseline','candidate'):
            r=by_key[(p['seed'],p['seat'],side)]
            for i,ph in enumerate(r['game']['phases']):
                for j,a in enumerate(ph['actions']):
                    if a['actor']==p['seat'] and not a['ok']:
                        lines.append(f"  {side}: r{ph['round']} {a['action']} отклонено: {a['err']} (game.json /phases/{i}/actions/{j}).")
    lines += ['', 'Решения после принятых законов:']
    for game in evidence(results):
        for event in game['events']:
            if event['kind'] == 'after_passed_law' and event['next_observed_decision']:
                t = event['next_observed_decision']
                o = t['observation']
                lines.append(f"{game['side']} seed={game['seed']} место={game['seat']}: после закона {event['law_card']} в r{event['round']} следующий ход {t['decision']}; налог {o['active_tax_bps']} bps, сдвиг цены {o['active_price_shift']}, boom {o['active_boom']}. Источник: game.json {event['pointer']} и trace.jsonl.")
    lines += ['', 'После принятия закона evidence.json сохраняет следующее наблюдение и решение агента. Изменение действия само по себе не означает полезную адаптацию. Контрфактические исходы не рассчитываются.', '',
              'Файлы каждой партии: cases/<seed>-<seat>-<side>/game.json, trace.jsonl, result.json, engine.stderr. Полные данные событий: evidence.json. Условия и хэши: manifest.json; снимок исходников: source.zip.', '',
              'Пилот с людьми не проведён. Заполните pilot_ledger.csv: время привлечения и подключения учитывается отдельно от времени прогона. Цена предложения не выводится из игровых выплат.']
    text='\n'.join(lines)+'\n'
    (out/'report.txt').write_text(text)
    safe=html.escape(text)
    (out/'report.html').write_text('<!doctype html><html lang="ru"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Сравнение Alashi</title><style>body{font:16px/1.6 system-ui;max-width:1000px;margin:32px auto;padding:20px}pre{white-space:pre-wrap;overflow-wrap:anywhere}a{color:#17596d}</style><a href="manifest.json">Условия</a> · <a href="summary.json">Результаты</a> · <a href="evidence.json">Эпизоды</a><pre>'+safe+'</pre></html>')


def provenance(binary, c, out):
    paths = set()
    for directory in ('arena/src','rules/src','programs'):
        paths.update((ROOT/directory).rglob('*.rs'))
    paths.update((ROOT/'programs').rglob('Cargo.toml'))
    for name in ('Cargo.toml','Cargo.lock','arena/Cargo.toml','arena/Cargo.lock','rules/Cargo.toml','rust-toolchain.toml','tools/compare_agents.py'):
        paths.add(ROOT/name)
    for side in ('baseline','candidate'):
        for name in c[side].get('artifacts',[]):
            paths.add((ROOT/name).resolve())
        for arg in c[side].get('command',[]):
            candidate=ROOT/arg
            if candidate.is_file() and candidate.suffix in ('.py','.json','.rs','.txt'):
                paths.add(candidate.resolve())
    hashes={}
    with zipfile.ZipFile(out/'source.zip','w',zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(paths):
            if not path.is_file():
                continue
            key=os.path.relpath(path,ROOT)
            arc=key if not key.startswith('..') else 'external/'+digest(path)+'/'+path.name
            archive.write(path,arc)
            hashes[key]={'sha256':digest(path),'archive_path':arc}
    try:
        head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True,timeout=5).strip()
    except subprocess.SubprocessError:
        head=None
    return {'schema_version':1,'created_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'runtime': {'python': sys.version, 'python_executable': sys.executable,
                        'platform': platform.platform()},
            'config':c,'git_head':head,'source_files':hashes,'binary_path':str(binary),'binary_sha256':digest(binary),
            'source_archive_sha256':digest(out/'source.zip'), 'mode':'isolated_local_simulation',
            'operator': {'kind': 'local_runner', 'identity_verified': False},
            'rules':{'entry_fee':10000000,'phase_duration':10,'market_exec':'sequential','rent_in_rank':False},
            'guarantee':'Hashes identify saved artifacts; they do not attest model identity or independent operators.'}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config',type=Path)
    parser.add_argument('--out',type=Path)
    parser.add_argument('--verify',type=Path,help='verify saved result checksums without executing agents')
    parser.add_argument('--binary',type=Path,default=ROOT/'arena/target/debug/evalgame')
    args=parser.parse_args()
    if args.verify:
        root=args.verify.resolve()
        manifest=json.loads((root/'manifest.json').read_text())
        hashes=manifest.get('result_hashes')
        if not hashes:
            raise ValueError('run has no finalized result hashes')
        for name,expected in hashes.items():
            path=(root/name).resolve()
            if not path.is_relative_to(root) or not path.is_file() or digest(path)!=expected:
                raise ValueError(f'artifact mismatch: {name}')
        print(f'OK: {len(hashes)} saved artifacts match manifest (not an execution attestation)')
        return 0
    if args.config is None or args.out is None:
        parser.error('--config and --out are required for a new run')
    c=load_config(args.config.resolve())
    binary=args.binary.resolve()
    if not binary.is_file():
        raise ValueError('build evalgame first: cargo build --offline --manifest-path arena/Cargo.toml --bin evalgame')
    out=args.out.resolve()
    out.mkdir(parents=True,exist_ok=False)
    os.chmod(out,0o700)
    write_json(out/'config.json',c)
    manifest=provenance(binary,c,out)
    manifest['status']='running'
    write_json(out/'manifest.json',manifest)
    results=[]
    started=time.monotonic()
    with (out/'runs.jsonl').open('w') as log:
        for si,seed in enumerate(c['seeds']):
            for seat in range(len(c['opponents'])+1):
                order=('baseline','candidate') if (si+seat)%2==0 else ('candidate','baseline')
                for side in order:
                    folder=out/'cases'/f'{seed}-{seat}-{side}'
                    folder.mkdir(parents=True)
                    r=run_game(binary,c,side,seed,seat,folder)
                    results.append(r)
                    log.write(json.dumps({k:v for k,v in r.items() if k not in ('game','trace')})+'\n')
                    log.flush()
                    print(f"{len(results)}/{len(c['seeds'])*(len(c['opponents'])+1)*2}: {side} seed={seed} seat={seat} {r['status']}",flush=True)
    summary=compare(results,c)
    summary['total_elapsed_s']=time.monotonic()-started
    write_json(out/'summary.json',summary)
    write_json(out/'evidence.json',evidence(results))
    report(results,summary,c,out)
    (out/'pilot_ledger.csv').write_text('operator_id,previous_method,previous_minutes,recruitment_minutes,integration_minutes,review_minutes,offer_currency,offer_amount,accepted,completed,useful_report,paid_amount,repeated,notes\n')
    manifest.update(status='completed' if all(r['status']=='completed' for r in results) else 'completed_with_failures',
                    elapsed_s=summary['total_elapsed_s'],
                    result_hashes={str(p.relative_to(out)):digest(p) for p in sorted(out.rglob('*')) if p.is_file() and p.name!='manifest.json'})
    write_json(out/'manifest.json',manifest)
    print(out/'report.html')
    return 0 if manifest['status']=='completed' else 1


if __name__=='__main__':
    try:
        raise SystemExit(main())
    except (OSError,ValueError,KeyError) as e:
        print(f'[ERROR] {e}',file=sys.stderr)
        raise SystemExit(2)

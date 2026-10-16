#!/usr/bin/env python3
"""Meter full-context workers versus a paid shared scout on isolated fixtures.

Default: prepare exact inputs with oracle placeholders, without network calls.
--live: send at most 33 requests to the configured free GLM-4.5-Flash model.
No retries, model fallback, live arena, transactions, or publishing are performed.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import threading
import time

import benchmark as base

FIELDS = list(dict.fromkeys(k for _, fields in base.ROLES.values() for k in fields))
SCOUT = ('Extract the current state fields: ' + ', '.join(FIELDS) +
         '. Copy values exactly, preserving numeric units and arrays. '
         'Return only a JSON object with those fields. Do not decide a move.')
CHIEF = ('Choose the market action that maximizes cash at the end of this final '
         'market. Unsold inventory has no cash value after the last market. '
         'Return only {"action":"sell"|"buy"|"pass","units":integer}. '
         'Use units 0 for pass.')
VERIFIER = ('Check the proposed action using the reports. It must sell all owned '
            'goods if a sale is legal, its revenue is positive, and this is the '
            'final market. Return only {"valid":boolean}.')


def exact(a, b):
    return json.dumps(a, sort_keys=True) == json.dumps(b, sort_keys=True)


def payload(role, system, user):
    return {'model': base.MODEL, 'thinking': {'type': 'disabled'},
            'temperature': 0, 'max_tokens': 480 if role == 'scout' else 180,
            'messages': [{'role': 'system', 'content': system},
                         {'role': 'user', 'content': json.dumps(user, ensure_ascii=False)}]}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--live', action='store_true')
    ap.add_argument('--out', type=Path, required=True)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    cases = json.loads(subprocess.check_output([str(base.ORACLE)], text=True))
    key = None
    if args.live:
        key = json.loads((Path.home()/'.config/alashi/llm.json').read_text())['key']
        if not key or any(c in key for c in '\r\n"'):
            raise ValueError('Invalid configured credential')
    mode_label = 'provider-measured' if args.live else 'planned-oracle-placeholders'
    manifest = {'status': mode_label, 'model': base.MODEL, 'endpoint': base.ENDPOINT,
                'max_requests': 33, 'max_output_tokens_all_requests': 6840,
                'scope': '3 paired frozen final-market states; shared scout overhead included',
                'billing': {'usd_per_million_input': 0, 'usd_per_million_output': 0,
                            'source': 'https://docs.z.ai/guides/overview/pricing',
                            'checked_date': '2026-10-16'},
                'exclusions': ['strategy authoring and discovery', 'live-state refresh',
                               'independent owner reproduction', 'whole-game win rate'],
                'source_sha256': {str(p.relative_to(base.ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                                  for p in [Path(__file__), Path(base.__file__),
                                            base.ROOT/'tools/board_agents.py',
                                            base.ROOT/'rules/examples/owner_film_oracle.rs']}}
    base.write_json(args.out/'manifest.json', manifest)
    base.write_json(args.out/'cases.json', cases)
    records, runs = [], []
    lock = threading.Lock()

    def ask(run, role, system, user, expected):
        request = payload(role, system, user)
        start = time.monotonic()
        response = None
        transport_ok = True
        if args.live:
            with tempfile.NamedTemporaryFile(mode='w', suffix='.json', encoding='utf-8') as f:
                json.dump(request, f, ensure_ascii=False)
                f.flush()
                proc = subprocess.run(
                    ['curl', '-4', '--silent', '--show-error', '--fail-with-body',
                     '--connect-timeout', '5', '--max-time', '45', '-X', 'POST',
                     base.ENDPOINT, '--header', 'Content-Type: application/json',
                     '--data-binary', '@'+f.name, '--config', '-'],
                    input='header = "Authorization: Bearer '+key+'"\n',
                    text=True, capture_output=True, timeout=50)
            transport_ok = proc.returncode == 0
            try:
                response = json.loads(proc.stdout)
            except json.JSONDecodeError:
                response = {'error': 'non_json_response', 'curl_exit': proc.returncode}
        row = {'run_id': run, 'role': role, 'request': request,
               'response': response, 'expected': expected,
               'elapsed_s': time.monotonic()-start if args.live else None,
               'status': mode_label}
        with lock:
            records.append(row)
            with (args.out/'requests.jsonl').open('a') as f:
                f.write(json.dumps(row, ensure_ascii=False)+'\n')
        if not args.live:
            return expected
        if not transport_ok:
            raise RuntimeError('Provider request failed; response retained; no retry')
        usage = response.get('usage', {})
        if not all(type(usage.get(k)) is int and usage[k] >= 0
                   for k in ('prompt_tokens', 'completion_tokens', 'total_tokens')):
            raise RuntimeError('Missing provider usage; cannot claim savings')
        if usage['total_tokens'] != usage['prompt_tokens']+usage['completion_tokens']:
            raise RuntimeError('Unexpected provider accounting')
        print(f'{run} {role}: {usage["total_tokens"]} tokens', flush=True)
        try:
            if response['choices'][0]['finish_reason'] != 'stop':
                return None
            return json.loads(response['choices'][0]['message']['content'])
        except (KeyError, IndexError, TypeError, json.JSONDecodeError):
            return None

    for i, case in enumerate(cases):
        state = {k: v for k, v in case.items() if k not in ('oracle', 'id')}
        state.update({'president_idx': 1, 'influence': 2, 'active_subsidy_goods': 0,
                      'epoch': 'classic', 'price_shift': 0, 'boom': False,
                      'factions': [{'idx': j, 'cash': 100000000, 'goods': case['goods'],
                                    'influence': 2} for j in (0, 1)],
                      'available_action_types': ['sell', 'buy', 'pass']})
        expected_reports = {
            'market': {'sell_units': case['goods'], 'sell_revenue': case['oracle']['amount']},
            'legal': {'can_sell': True, 'can_buy': True},
            'horizon': {'markets_after_this': 0, 'is_final_market': True}}
        for mode in (('full', 'shared') if i % 2 == 0 else ('shared', 'full')):
            run = case['id']+'-'+mode
            context, scout_ok = state, True
            if mode == 'shared':
                expected_snapshot = {k: state[k] for k in FIELDS}
                context = ask(run, 'scout', base.FULL_RULES+'\n'+SCOUT,
                              state, expected_snapshot)
                scout_ok = exact(context, expected_snapshot)

            def worker(role):
                instruction, _ = base.ROLES[role]
                system = (base.FULL_RULES+'\n\n' if mode == 'full' else '')
                system += 'You are a specialist, not the player. Follow this output schema: '+instruction
                return ask(run, role, system, context, expected_reports[role])

            with ThreadPoolExecutor(max_workers=3) as pool:
                reports = dict(zip(base.ROLES, pool.map(worker, base.ROLES)))
            expected_action = {'action': 'sell', 'units': case['goods']}
            decision = ask(run, 'chief', CHIEF,
                           {'goal': 'maximize final cash', 'goods': case['goods'],
                            'cash': case['cash'], 'reports': reports}, expected_action)
            validation = ask(run, 'verifier', VERIFIER,
                             {'decision': decision, 'reports': reports, 'goods': case['goods']},
                             {'valid': True})
            execution = None
            if (isinstance(decision, dict) and decision.get('action') in ('sell', 'buy', 'pass')
                    and type(decision.get('units')) is int and 0 <= decision['units'] <= 65535):
                execution = json.loads(subprocess.check_output([
                    str(base.ORACLE), 'execute', str(case['goods']), str(case['sold_this_round']),
                    str(case['tax_bps']), decision['action'], str(decision['units'])], text=True))
            correct = (scout_ok and exact(reports, expected_reports)
                       and exact(decision, expected_action) and exact(validation, {'valid': True})
                       and exact(execution, case['oracle']))
            rows = [r for r in records if r['run_id'] == run]
            usage = ({k: sum(r['response']['usage'][k] for r in rows)
                      for k in ('prompt_tokens', 'completion_tokens', 'total_tokens')}
                     if args.live else None)
            runs.append({'run_id': run, 'mode': mode, 'calls': len(rows),
                         'usage': usage, 'quality_passed': correct if args.live else None,
                         'oracle_placeholder_check': correct if not args.live else None,
                         'execution': execution})
            base.write_json(args.out/'runs.json', runs)
    totals = {mode: {k: sum(r['usage'][k] for r in runs if r['mode'] == mode)
                    for k in ('prompt_tokens', 'completion_tokens', 'total_tokens')}
              for mode in ('full', 'shared')} if args.live else None
    comparable = args.live and all(r['quality_passed'] for r in runs)
    saved = totals['full']['total_tokens']-totals['shared']['total_tokens'] if comparable else None
    summary = {'status': mode_label, 'requests': len(records), 'totals': totals,
               'quality_passed': comparable if args.live else None,
               'tokens_saved': saved,
               'reduction_percent': round(saved/totals['full']['total_tokens']*100, 2) if comparable else None,
               'tariff_cost_usd': 0 if args.live else None,
               'dollar_savings': 0 if comparable else None,
               'request_text_utf8_bytes': sum(len(m['content'].encode()) for r in records for m in r['request']['messages'])}
    base.write_json(args.out/'summary.json', summary)
    print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    main()

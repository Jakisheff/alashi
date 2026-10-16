#!/usr/bin/env python3
"""Recompute token totals and quality from provider receipts and the Rust oracle."""
import argparse
import json
from pathlib import Path
import re
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ORACLE = ROOT/'target/debug/examples/owner_film_oracle'


def exact(a, b):
    return json.dumps(a, sort_keys=True) == json.dumps(b, sort_keys=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    args = parser.parse_args()
    receipts = [json.loads(line) for line in (args.directory/'requests.jsonl').read_text().splitlines()]
    manifest = json.loads((args.directory/'manifest.json').read_text())
    cases = json.loads(subprocess.check_output([str(ORACLE)], text=True))
    assert exact(cases, json.loads((args.directory/'cases.json').read_text()))
    assert 0 < len(receipts) <= 33
    assert len({(r['run_id'], r['role']) for r in receipts}) == len(receipts)
    assert all(r['status'] == 'provider-measured' for r in receipts)
    assert sum(r['request']['max_tokens'] for r in receipts) <= 6840
    runs = []
    for case in cases:
        for mode in ('full', 'shared'):
            name = case['id']+'-'+mode
            rows = [r for r in receipts if r['run_id'] == name]
            assert len(rows) <= (5 if mode == 'full' else 6)
            expected = {
                'market': {'sell_units': case['goods'], 'sell_revenue': case['oracle']['amount']},
                'legal': {'can_sell': case['goods'] > 0,
                          'can_buy': case['cash'] >= case['price_table_millions'][case['sold_this_round']]*1000000},
                'horizon': {'markets_after_this': case['total_rounds']-case['round'],
                            'is_final_market': case['round'] == case['total_rounds']},
                'chief': {'action': 'sell', 'units': case['goods']},
                'verifier': {'valid': True}}
            if mode == 'shared':
                expected['scout'] = {k: v for k, v in case.items() if k not in ('id', 'oracle')}
            totals = dict(prompt_tokens=0, completion_tokens=0, total_tokens=0)
            cached, unknown, failures, by_role = 0, [], [], {}
            failures.extend(sorted(set(expected)-{r['role'] for r in rows}))
            for row in rows:
                response = row['response']
                request = row['request']
                assert request['model'] == 'glm-4.5-flash'
                assert request['thinking'] == {'type': 'disabled'}
                usage = response.get('usage')
                if usage is None:
                    unknown.append(row['role'])
                else:
                    assert all(type(usage[k]) is int and usage[k] >= 0 for k in totals)
                    assert usage['total_tokens'] == usage['prompt_tokens']+usage['completion_tokens']
                    assert usage['completion_tokens'] <= request['max_tokens']
                    hit = usage.get('prompt_tokens_details', {}).get('cached_tokens')
                    assert type(hit) is int and 0 <= hit <= usage['prompt_tokens']
                    cached += hit
                    for k in totals:
                        totals[k] += usage[k]
                    by_role[row['role']] = {k: usage[k] for k in totals}
                try:
                    choice = response['choices'][0]
                    content = choice['message']['content']
                    if manifest.get('protocol_version', 1) >= 2:
                        fenced = re.fullmatch(r'\s*```(?:json)?\s*\n?(.*?)\n?```\s*', content, flags=re.S)
                        if fenced:
                            content = fenced.group(1)
                    answer = json.loads(content)
                    good = choice['finish_reason'] == 'stop' and exact(answer, expected[row['role']])
                except (KeyError, IndexError, TypeError, json.JSONDecodeError):
                    good = False
                if not good:
                    failures.append(row['role'])
            runs.append({'run_id': name, 'mode': mode, 'usage': totals,
                         'cached_input_tokens': cached, 'unknown_usage_roles': unknown,
                         'failed_roles': failures, 'by_role': by_role,
                         'eligible': not unknown and not failures})
    paired = []
    for case in cases:
        full, shared = [r for r in runs if r['run_id'].startswith(case['id']+'-')]
        eligible = full['eligible'] and shared['eligible']
        before, after = full['usage']['total_tokens'], shared['usage']['total_tokens']
        paired.append({'case_id': case['id'], 'eligible': eligible,
                       'full_tokens': before if full['eligible'] else None,
                       'shared_tokens': after if shared['eligible'] else None,
                       'saved_tokens': before-after if eligible else None,
                       'reduction_percent': round(100*(before-after)/before, 2) if eligible else None})
    generated_runs = json.loads((args.directory/'runs.json').read_text())
    for audited in runs:
        saved = next(r for r in generated_runs if r['run_id'] == audited['run_id'])
        assert saved['usage'] == audited['usage']
        assert saved['quality_passed'] == (not audited['failed_roles'])
        assert saved['usage_complete'] == (not audited['unknown_usage_roles'])
    observed = {k: sum(r['usage'][k] for r in runs)
                for k in ('prompt_tokens', 'completion_tokens', 'total_tokens')}
    eligible = [p for p in paired if p['eligible']]
    before = sum(p['full_tokens'] for p in eligible)
    after = sum(p['shared_tokens'] for p in eligible)
    audit = {'status': 'observed', 'requests': len(receipts), 'known_usage': observed,
             'unknown_usage_requests': sum(len(r['unknown_usage_roles']) for r in runs),
             'runs': runs, 'pairs': paired, 'comparable_pairs': len(eligible),
             'paired_reduction_percent': round(100*(before-after)/before, 2) if before else None,
             'scope': 'Only complete correct paired fixtures, not whole-game savings or latency'}
    (args.directory/'audit.json').write_text(json.dumps(audit, indent=2)+'\n')
    print(json.dumps({k: v for k, v in audit.items() if k != 'runs'}, indent=2))


if __name__ == '__main__':
    main()

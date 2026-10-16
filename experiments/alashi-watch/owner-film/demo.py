#!/usr/bin/env python3
"""Produce labelled, scripted decisions with actual offline Rust rule receipts."""
import hashlib
import json
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ORACLE = ROOT / 'target/debug/examples/owner_film_oracle'


def generate():
    cases = json.loads(subprocess.check_output([str(ORACLE)], text=True))
    runs = []
    for case in cases:
        for mode in ('full', 'scoped'):
            decision = {'action': 'sell', 'units': case['goods']}
            execution = json.loads(subprocess.check_output([
                str(ORACLE), 'execute', str(case['goods']),
                str(case['sold_this_round']), str(case['tax_bps']),
                decision['action'], str(decision['units'])], text=True))
            assert execution == case['oracle'] and execution['ok']
            runs.append({'case_id': case['id'], 'mode': mode,
                         'decision_source': 'scripted presentation',
                         'decision': decision, 'execution': execution})
    paths = ['rules/examples/owner_film_oracle.rs', 'rules/src/actions.rs',
             'rules/src/constants.rs', 'rules/src/logic.rs']
    return {'mode': 'scripted workflow concept; canonical local rule execution',
            'model_calls': 0, 'token_savings': None, 'cases': cases, 'runs': runs,
            'source_hashes': {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest()
                              for p in paths}}


if __name__ == '__main__':
    destination = HERE / 'evidence/demo.json'
    destination.parent.mkdir(exist_ok=True)
    destination.write_text(json.dumps(generate(), indent=2) + '\n')
    print('6 scripted decisions executed through canonical Rust rules; no LLM usage claim.')

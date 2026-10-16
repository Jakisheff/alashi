#!/usr/bin/env python3
"""Bounded paired context-routing experiment. Uses configured Alashi LLM access.

Three independent market fixtures, two organizations, five calls per workflow.
Only worker context routing differs. Raw public requests, responses and provider
usage are retained; credentials never enter artifacts or process arguments.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
ORACLE = ROOT / 'target/debug/examples/owner_film_oracle'
MODEL = 'glm-4.5-flash'
ENDPOINT = 'https://api.z.ai/api/paas/v4/chat/completions'
spec = importlib.util.spec_from_file_location('existing_board', ROOT / 'tools/board_agents.py')
board = importlib.util.module_from_spec(spec)
spec.loader.exec_module(board)
# The current game's rules text, without its player-specific output instructions.
FULL_RULES = '\n'.join(board.SYSTEM.splitlines()[2:-1])
ROLES = {
    'market': ('Compute net revenue from selling every owned good now. Prices in price_table_millions are millions of game cash. Advance the price index for each sold unit, clamping at the last price. Subtract floor(gross * tax_bps / 10000). Return only {"sell_units":integer,"sell_revenue":integer}.', ['goods','sold_this_round','tax_bps','price_table_millions']),
    'legal': ('For the current market phase return only {"can_sell":boolean,"can_buy":boolean}. Sell requires positive goods. Buy requires cash at least the current price in millions. Assume alive and no prior action in this phase.', ['phase','cash','goods','sold_this_round','price_table_millions']),
    'horizon': ('Alashi has total_rounds rounds, each ordered market, action, law. Return only {"markets_after_this":integer,"is_final_market":boolean}. Count market phases strictly after the current market.', ['round','total_rounds','phase']),
}


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--out', type=Path, default=HERE / 'evidence/benchmark')
    ap.add_argument('--config', type=Path, default=HERE / 'chosen-config.json')
    args = ap.parse_args()
    config=json.loads(args.config.read_text())
    if config['model']!=MODEL or config['context_fields']!={role:fields for role,(_,fields) in ROLES.items()}:
        raise ValueError('This paired experiment requires the documented model and sufficient context fields')
    args.out.mkdir(parents=True, exist_ok=False)
    cfg = json.loads((Path.home()/'.config/alashi/llm.json').read_text())
    key = os.environ.get('ALASHI_LLM_KEY') or cfg.get('key')
    if not key or any(c in key for c in '\n\r"'):
        raise ValueError('Valid configured Alashi key required')
    cases = json.loads(subprocess.check_output([str(ORACLE)], text=True))
    write_json(args.out / 'cases.json', cases)
    manifest = {'model': MODEL, 'endpoint': ENDPOINT, 'temperature': 0,
                'thinking': 'disabled', 'max_tokens_per_call': 180,
                'design': '3 paired synthetic final-market fixtures; identical model and roles; only worker context differs',
                'budget_target_tokens_per_workflow':10000,
                'context_configuration':config,
                'quality_gate':'All specialist fields, Chief action and verifier correct; real Rust action matches oracle; observed workflow tokens <= 10000',
                'baseline': 'current board_agents.py game rules plus full fixture state for each specialist',
                'treatment': 'task-specific fields and rules, with sufficient task information retained',
                'limits': 'Small context-routing probe, not whole-game win rate, general task performance, dollar savings or live latency proof.',
                'preflight_usage': {'prompt_tokens':18,'completion_tokens':10,'total_tokens':28,
                                    'scope':'Access check before this experiment; excluded equally from both workflow totals'},
                'source_hashes': {str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest()
                                  for p in [Path(__file__),ROOT/'tools/board_agents.py',ROOT/'rules/src/actions.rs',ROOT/'rules/src/logic.rs',ROOT/'rules/src/constants.rs',ROOT/'rules/examples/owner_film_oracle.rs']}}
    write_json(args.out / 'manifest.json', manifest)
    lock = threading.Lock()
    records = []
    origin = time.monotonic()

    def ask(run, role, system, user):
        payload = {'model':MODEL,'thinking':{'type':'disabled'},'temperature':0,'max_tokens':180,
                   'messages':[{'role':'system','content':system},{'role':'user','content':json.dumps(user, ensure_ascii=False)}]}
        start = time.monotonic()
        with tempfile.NamedTemporaryFile(mode='w', encoding='utf-8', prefix='alashi-film-request-', suffix='.json') as tmp:
            json.dump(payload,tmp,ensure_ascii=False);tmp.flush()
            proc = subprocess.run(['curl','-4','--silent','--show-error','--fail-with-body','--connect-timeout','5','--max-time','45',
                '-X','POST',ENDPOINT,'--header','Content-Type: application/json','--data-binary','@'+tmp.name,'--config','-'],
                input='header = "Authorization: Bearer '+key+'"\n', text=True, capture_output=True, timeout=50)
        elapsed = time.monotonic()-start
        try:
            response = json.loads(proc.stdout)
        except json.JSONDecodeError:
            response = {'error_type':'transport_or_non_json_response','curl_exit':proc.returncode}
        row = {'run_id':run,'role':role,'started_s':start-origin,'elapsed_s':elapsed,'request':payload,'response':response}
        with lock:
            records.append(row)
            with (args.out/'responses.jsonl').open('a') as f:
                f.write(json.dumps(row,ensure_ascii=False)+'\n')
        if proc.returncode:
            raise RuntimeError('Provider request failed; receipt retained; no retry or unmetered fallback')
        usage = response.get('usage',{})
        if not all(type(usage.get(k)) is int and usage[k]>=0 for k in ('prompt_tokens','completion_tokens','total_tokens')):
            raise RuntimeError('Provider usage missing; no token saving claim permitted')
        if usage['total_tokens'] != usage['prompt_tokens'] + usage['completion_tokens']:
            raise RuntimeError('Unexpected usage accounting')
        raw = response['choices'][0]['message']['content']
        try:
            answer = json.loads(raw)
        except (json.JSONDecodeError,TypeError):
            answer = None
        print(f'{run} {role}: {usage["total_tokens"]} tokens / {elapsed:.2f}s',flush=True)
        return answer

    summaries=[]
    for i, case in enumerate(cases):
        for mode in (['full','scoped'] if i%2==0 else ['scoped','full']):
            run=case['id']+'-'+mode
            state={k:v for k,v in case.items() if k not in ('oracle','id')}
            state.update({'president_idx':1,'influence':2,'active_subsidy_goods':0,'epoch':'classic',
                'factions':[{'idx':0,'name':'Owner','cash':100000000,'goods':case['goods'],'influence':2},
                            {'idx':1,'name':'Opponent','cash':100000000,'goods':case['goods'],'influence':2}],
                'available_action_types':['sell','buy','pass'], 'price_shift':0,'boom':False})
            def worker(role):
                instruction,fields=ROLES[role]
                context=state if mode=='full' else {k:state[k] for k in config['context_fields'][role]}
                rules=FULL_RULES+'\n\n' if mode=='full' else ''
                return ask(run,role,rules+'You are a specialist, not the player. Follow this output schema: '+instruction,context)
            with ThreadPoolExecutor(max_workers=3) as pool:
                reports=dict(zip(ROLES,pool.map(worker,ROLES)))
            decision=ask(run,'chief','Choose the market action that maximizes cash at the end of this final market. Unsold inventory has no cash value after the last market. Return only {"action":"sell"|"buy"|"pass","units":integer}. Use units 0 for pass.',
                         {'goal':'maximize final cash','goods':case['goods'],'cash':case['cash'],'reports':reports})
            validation=ask(run,'verifier','Check the proposed action using the reports. It must sell all owned goods if a sale is legal, its revenue is positive, and this is the final market. Return only {"valid":boolean}.',
                           {'decision':decision,'reports':reports,'goods':case['goods']})
            execution=None
            if isinstance(decision,dict) and decision.get('action') in ('sell','buy','pass') and type(decision.get('units')) is int and 0<=decision['units']<=65535:
                execution=json.loads(subprocess.check_output([str(ORACLE),'execute',str(case['goods']),str(case['sold_this_round']),str(case['tax_bps']),decision['action'],str(decision['units'])],text=True))
            expected_reports={'market':{'sell_units':case['goods'],'sell_revenue':case['oracle']['amount']},
                              'legal':{'can_sell':True,'can_buy':True},
                              'horizon':{'markets_after_this':0,'is_final_market':True}}
            passed=reports==expected_reports and decision=={'action':'sell','units':case['goods']} and validation=={'valid':True} and execution==case['oracle']
            rows=[r for r in records if r['run_id']==run]
            usage={k:sum(r['response']['usage'][k] for r in rows) for k in ('prompt_tokens','completion_tokens','total_tokens')}
            passed=passed and usage['total_tokens']<=10000
            summaries.append({'run_id':run,'mode':mode,'case_id':case['id'],'passed':passed,'reports':reports,'decision':decision,'validation':validation,'execution':execution,'usage':usage,'calls':len(rows)})
            write_json(args.out/'runs.json',summaries)
    totals={mode:{'passed':sum(r['passed'] for r in summaries if r['mode']==mode),
                  'runs':3,'calls':sum(r['calls'] for r in summaries if r['mode']==mode),
                  **{k:sum(r['usage'][k] for r in summaries if r['mode']==mode) for k in ('prompt_tokens','completion_tokens','total_tokens')}} for mode in ('full','scoped')}
    comparable=all(r['passed'] for r in summaries)
    saved=totals['full']['total_tokens']-totals['scoped']['total_tokens']
    summary={'totals':totals,'all_quality_checks_passed':comparable,
             'tokens_saved':saved if comparable else None,
             'total_token_reduction_percent':round(100*saved/totals['full']['total_tokens'],2) if comparable else None,
             'dollar_savings':None,'generalization':'Only these 3 paired fixtures; no general performance claim.'}
    write_json(args.out/'summary.json',summary)
    print(json.dumps(summary),flush=True)


if __name__=='__main__':
    main()

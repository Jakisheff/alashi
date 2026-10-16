#!/usr/bin/env python3
"""Reconcile film claims with public provider receipts and canonical rule execution."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]


def check_video(measured):
    video=HERE/'alashi-owner-film.mp4';meta=json.loads((HERE/'evidence/video.json').read_text())
    assert meta['sha256']==hashlib.sha256(video.read_bytes()).hexdigest()
    assert (meta['frames'],meta['fps'],meta['duration_s'],meta['width'],meta['height'])==(3600,30,120,1920,1080)
    assert meta['measured_llm'] is measured


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--oracle',type=Path,default=ROOT/'target/debug/examples/owner_film_oracle')
    ap.add_argument('--video',action='store_true')
    ap.add_argument('--concept',action='store_true',help='Check the explicitly scripted film, without a model or token claim')
    args=ap.parse_args()
    if args.concept:
        from demo import generate
        recorded=json.loads((HERE/'evidence/demo.json').read_text())
        assert recorded==generate(), 'Saved receipts differ from fresh canonical rule execution'
        assert recorded['model_calls']==0 and recorded['token_savings'] is None
        assert len(recorded['runs'])==6
        if args.video:check_video(False)
        report={'passed':True,'mode':'scripted concept',
                'checks':'6 scripted decisions reexecuted through canonical Rust rules; no token claim',
                'video_hash_checked':args.video}
        (HERE/'evidence/checks.json').write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps(report,indent=2))
        return
    folder=HERE/'evidence/benchmark'
    rows=[json.loads(l) for l in (folder/'responses.jsonl').read_text().splitlines()]
    runs=json.loads((folder/'runs.json').read_text())
    summary=json.loads((folder/'summary.json').read_text())
    cases=json.loads((folder/'cases.json').read_text())
    manifest=json.loads((folder/'manifest.json').read_text())
    assert len(rows)==30 and len(runs)==6 and len(cases)==3
    assert all('error' not in r['response'] for r in rows)
    assert all(r['response']['model']==manifest['model'] for r in rows)
    assert Counter(r['role'] for r in rows)==dict(market=6,legal=6,horizon=6,chief=6,verifier=6)
    assert len({r['response']['id'] for r in rows})==30
    for path,digest in manifest['source_hashes'].items():
        assert hashlib.sha256((ROOT/path).read_bytes()).hexdigest()==digest,path
    assertions=[]
    for run in runs:
        rr=[r for r in rows if r['run_id']==run['run_id']]
        assert len(rr)==5
        usage={key:sum(r['response']['usage'][key] for r in rr) for key in ('prompt_tokens','completion_tokens','total_tokens')}
        assert usage==run['usage'] and usage['total_tokens']<=10000
        assert all(r['response']['usage']['total_tokens']==r['response']['usage']['prompt_tokens']+r['response']['usage']['completion_tokens'] for r in rr)
        worker=[r for r in rr if r['role'] in ('market','legal','horizon')]
        # Real overlapping provider requests, independently of the film's edited clock.
        overlap=min(r['started_s']+r['elapsed_s'] for r in worker)-max(r['started_s'] for r in worker)
        assert overlap>0
        chief=next(r for r in rr if r['role']=='chief');verifier=next(r for r in rr if r['role']=='verifier')
        assert chief['started_s']>=max(r['started_s']+r['elapsed_s'] for r in worker)
        assert verifier['started_s']>=chief['started_s']+chief['elapsed_s']
        for r in rr:
            parsed=json.loads(r['response']['choices'][0]['message']['content'])
            expected=run['decision'] if r['role']=='chief' else run['validation'] if r['role']=='verifier' else run['reports'][r['role']]
            assert parsed==expected
        case=next(c for c in cases if c['id']==run['case_id'])
        decision=run['decision']
        fresh=json.loads(subprocess.check_output([str(args.oracle),'execute',str(case['goods']),str(case['sold_this_round']),str(case['tax_bps']),decision['action'],str(decision['units'])],text=True))
        assert fresh==run['execution']==case['oracle'] and fresh['ok']
        assert run['passed']
        assertions.append({'run_id':run['run_id'],'parallel_overlap_s':round(overlap,3),'tokens':usage['total_tokens'],'rules_reexecuted':True})
    for mode in ('full','scoped'):
        for key in ('prompt_tokens','completion_tokens','total_tokens'):
            assert summary['totals'][mode][key]==sum(r['usage'][key] for r in runs if r['mode']==mode)
        assert summary['totals'][mode]['calls']==15 and summary['totals'][mode]['passed']==3
    saved=summary['totals']['full']['total_tokens']-summary['totals']['scoped']['total_tokens']
    assert saved==summary['tokens_saved']
    assert summary['total_token_reduction_percent']==round(saved/summary['totals']['full']['total_tokens']*100,2)
    assert summary['all_quality_checks_passed'] and summary['dollar_savings'] is None
    if args.video:
        check_video(True)
    report={'passed':True,'checks':'30 raw responses reconciled; 6 workflows reexecuted through canonical Rust rules; provider concurrency and dependency order checked','workflows':assertions,'video_hash_checked':args.video}
    (HERE/'evidence/checks.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))


if __name__=='__main__':main()

#!/usr/bin/env python3
"""Example versioned policy, no model calls. Reads one JSON request from stdin."""
import argparse
import json
import sys

parser = argparse.ArgumentParser()
parser.add_argument('--bid-peso', type=int, default=4)
args = parser.parse_args()
request = json.load(sys.stdin)
o = request['observation']
me = o['my_idx']
phase = request['phase']
response = {'action': 'pass', 'cost_usd': 0}
if phase == 'market' and o['goods'][me] > 0:
    response.update(action='sell', units=o['goods'][me])
elif phase == 'action':
    if o['round'] == 4 and o['cash'][me] >= (args.bid_peso + 2) * 1_000_000:
        response.update(action='bid', amount=args.bid_peso * 1_000_000)
    else:
        response.update(action='produce')
elif phase == 'law' and o.get('decision_stage') != 'post_vote':
    response.update(action='vote_yes')
print(json.dumps(response))

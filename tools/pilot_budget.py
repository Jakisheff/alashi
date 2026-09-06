#!/usr/bin/env python3
"""Calculate a proposed pilot budget without inventing missing prices."""
import argparse
import json
import math
from pathlib import Path


def amount(value, name):
    if value is None:
        return None
    if type(value) not in (int, float) or not math.isfinite(value) or value < 0:
        raise ValueError(f'{name} must be nonnegative or null')
    return value


def calculate(c):
    n = c['operators']
    if type(n) is not int or not 1 <= n <= 100:
        raise ValueError('operators must be 1..100')
    fixed = amount(c['fixed_hours'], 'fixed_hours')
    per = amount(c['hours_per_operator'], 'hours_per_operator')
    if fixed is None or per is None:
        raise ValueError('planning hours are required')
    hours = fixed + n * per
    rate = amount(c['hourly_cost'], 'hourly_cost')
    inference = amount(c['inference_budget_per_operator'], 'inference_budget_per_operator')
    other = amount(c['other_costs'], 'other_costs')
    offer = amount(c['offer_per_operator'], 'offer_per_operator')
    labor = None if rate is None else hours * rate
    total = None if any(v is None for v in (labor, inference, other)) else labor + n * inference + other
    revenue = None if offer is None else n * offer
    missing = [key for key in ('hourly_cost', 'inference_budget_per_operator', 'other_costs', 'offer_per_operator') if c[key] is None]
    return {'status': 'hypothesis', 'currency': c['currency'], 'operators': n,
            'planned_hours': hours, 'labor_cost': labor, 'total_budget': total,
            'break_even_per_operator_if_all_pay': None if total is None else total / n,
            'revenue_if_all_pay': revenue,
            'contribution_if_all_pay': None if total is None or revenue is None else revenue-total,
            'missing_inputs_before_invitations': missing,
            'commercial_outcome': 'not_validated',
            'note': 'A price covering costs does not establish willingness to pay. Calendar limits are planning caps, not promises of participant availability.'}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('config', type=Path)
    args = p.parse_args()
    print(json.dumps(calculate(json.loads(args.config.read_text())), ensure_ascii=False, indent=2))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        raise SystemExit(f'[ERROR] {error}')

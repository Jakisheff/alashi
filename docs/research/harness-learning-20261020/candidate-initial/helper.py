def augment(state, history):
    out = {}
    try:
        rnd = state.get('round')
        phase = state.get('phase')
        own = state.get('own_idx')
        parts = state.get('participants') or []
        me = None
        for p in parts:
            if isinstance(p, dict) and p.get('idx') == own:
                me = p
        if me is None and parts and isinstance(parts[own], dict):
            me = parts[own]
        cash = (me or {}).get('cash', 0) or 0
        if rnd == 4 and phase == 'action':
            cands = state.get('candidates') or []
            bids = []
            for i, c in enumerate(cands):
                if not isinstance(c, dict):
                    continue
                ch = c.get('choice', i)
                act = c.get('action')
                amt = None
                if isinstance(act, dict) and act.get('action') in ('bid', 'bid_license'):
                    amt = act.get('amount')
                if amt is None:
                    amt = c.get('amount')
                if isinstance(amt, (int, float)):
                    bids.append((ch, int(amt)))
            cap = 8000000
            if cash >= 16000000:
                target = 12000000
            elif cash >= 9000000:
                target = 9000000
            else:
                target = None
            rec = None
            rec_amt = None
            if target is not None and bids:
                elig = [(ch, a) for ch, a in bids if a > cap and a <= target]
                if not elig:
                    elig = [(ch, a) for ch, a in bids if a <= target]
                if elig:
                    rec, rec_amt = max(elig, key=lambda t: t[1])
            out['auction'] = {
                'rival_bid_cap_observed': cap,
                'cash': cash,
                'bid_target': target,
                'recommended_choice': rec,
                'recommended_amount': rec_amt,
                'rules': [
                    'Participate in the round-4 license auction whenever cash >= 9M units.',
                    'Recommended bid: 12M if cash >= 16M, else 9M; never below 9M (rivals historically bid up to 8M).',
                    'Do not overbid far above 12M: excess goes to the bank and only ~half returns via rank share.',
                    'After bidding, still take your main action (produce/shuttle/etc.).',
                    'If you have evidence rivals bid above 8M this epoch, ignore this recommendation.'
                ]
            }
            if not bids:
                out['auction']['warning'] = 'No bid candidates found in menu; if bidding is legal, place bid above 8M up to 12M.'
    except Exception:
        pass
    return out

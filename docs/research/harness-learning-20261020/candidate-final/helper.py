def augment(state, history):
    out = {}
    try:
        rnd = state.get('round')
        ph = state.get('phase')
        own = state.get('own_idx')
        parts = state.get('participants') or []
        me = None
        for p in parts:
            if isinstance(p, dict) and p.get('idx') == own:
                me = p
        cash = (me or {}).get('cash', 0) or 0
        M = 1000000
        cap = 8 * M
        if rnd == 4 and ph == 'market':
            out['preauction'] = {
                'cash': cash,
                'rule': 'Keep cash >= 9M for the round-4 license auction: avoid large buy here; sell/sell_credit preferred.',
                'shortfall': cash < 9 * M
            }
        if rnd == 4 and ph == 'action':
            cands = state.get('candidates') or []
            bids = []
            main = []
            for i, c in enumerate(cands):
                if not isinstance(c, dict):
                    continue
                ch = c.get('choice', i)
                act = c.get('action')
                name = None
                amt = None
                if isinstance(act, dict):
                    name = act.get('action')
                    amt = act.get('amount')
                elif isinstance(act, str):
                    name = act
                if amt is None:
                    amt = c.get('amount')
                if isinstance(amt, (int, float)) and (name in ('bid', 'bid_license') or c.get('kind') == 'bid'):
                    bids.append((ch, int(amt)))
                elif name in ('produce', 'shuttle', 'donkey'):
                    main.append((ch, name))
            already = (me or {}).get('own_bid') or 0
            if already and already > 0:
                out['auction'] = {
                    'own_bid': already,
                    'recommended_choice': main[0][0] if main else None,
                    'rules': ['Bid already placed; take your main action now (produce/shuttle).']
                }
            else:
                if cash >= 16 * M:
                    target = 12 * M
                elif cash >= 9 * M:
                    target = 9 * M
                else:
                    target = None
                rec = None
                rec_amt = None
                if target is not None and bids:
                    elig = [(c2, a) for c2, a in bids if cap < a <= target]
                    if not elig:
                        elig = [(c2, a) for c2, a in bids if cap < a]
                    if elig:
                        rec, rec_amt = min(elig, key=lambda t: t[1])
                out['auction'] = {
                    'rival_bid_cap': cap,
                    'cash': cash,
                    'bid_target': target,
                    'recommended_choice': rec,
                    'recommended_amount': rec_amt,
                    'rules': [
                        'Rivals in this pool never bid above 8M; license income is always 21M+.',
                        'Bid once: 12M if cash>=16M, else 9M; never bid <=8M and never above 12M (overpay feeds the bank, only ~47.5%/28.5%/14.25% returns by rank).',
                        'After the bid, still take your main action (produce/shuttle).',
                        'Skipping the auction with cash>=9M is a clear loss.'
                    ]
                }
                if target is not None and bids and rec is None:
                    out['auction']['warning'] = 'No legal 9-12M bid candidate found; choose the smallest bid above 8M if available, else decide yourself.'
                if target is None:
                    out['auction']['note'] = 'Cash below 9M: auction rule not applicable this game; decide on general merit.'
    except Exception:
        pass
    return out


#!/usr/bin/env python3
"""A/B ренты лицензии: в ранге или поверх (текущее).

Определение (фиксировано): режим A «рент-в-ранге» — доход лицензии
прибавляется к final_cash ДО ранжирования и делится через доли;
режим B (текущий, прод) — рента приходит ПОСЛЕ дележа, только
держателю. Ставка победителя уже вычтена из кэша в обоих режимах
(как сейчас).

Вход: живые экспорты data/live/partyN_90s_export.json (поля
final_cash (cash+hard), final_promissory, payouts,
payout_breakdown.license_rent, ranks, prize_pot не хранится —
ставка видна в action_log как bid_license ok).

Выход: по каждой партии с проданной лицензией — кто держатель,
рента, ставка (из action_log), ранг и выплата в обоих режимах,
дельты; вердикт по каждой стороне свопа (держатель vs остальные).
"""
import json
import glob
import sys

PESO = 1_000_000
# доли сеттла (нормируются по числу фракций)
SHARES = [50, 30, 15, 5]
RAKE_BPS = 500
FACT = 5  # завод 5% банка из рейка


def settle(final_cash, bank, n, rent_holder, rent, factory_best):
    """Пересчёт дележа. final_cash: список (режим уже применён снаружи)."""
    order = sorted(range(n), key=lambda i: (-final_cash[i], i))
    share_n = min(n, len(SHARES))
    total_sh = sum(SHARES[:share_n])
    pot = bank * (10_000 - RAKE_BPS) // 10_000
    ranks = order
    payout = [0] * n
    for r, i in enumerate(order[:share_n]):
        payout[i] = pot * SHARES[r] // total_sh
    rank0 = order[0]
    rest = pot - sum(payout[o] for o in order[1:share_n])
    payout[rank0] = rest
    # завод из рейка
    rake = bank * RAKE_BPS // 10_000
    bonus = bank * FACT // 100
    if bonus > 0 and rake >= bonus:
        payout[factory_best] += bonus
    return ranks, payout


def main():
    files = sorted(glob.glob("data/live/party*_90s_export.json"))
    if len(sys.argv) > 1:
        files = sys.argv[1:]
    for path in files:
        d = json.load(open(path))
        n = d["n_factions"]
        # держатель и рента
        rents = d.get("payout_breakdown", [])
        holder = next((i for i, r in enumerate(rents) if r.get("license_rent", 0) > 0), None)
        if holder is None:
            continue
        rent = rents[holder]["license_rent"]
        # ставка победителя: последний ok bid_license
        bid = 0
        for a in d.get("actions", []):
            if a.get("action") == "bid_license" and a.get("ok"):
                # платит только победитель: ставим максимальный ok bid
                bid = max(bid, a.get("params", {}).get("amount", 0))
        # финальные кэши как в арене: final_cash уже cash+hard
        fc = d["final_cash"]
        bank = d["entry_fee"] * n + d.get("bank", 0) // PESO * 0  # bank в записи уже итог
        bank = d["bank"]
        # завод: индекс с макс влияния (final_influence), тай по рангу
        infl = d.get("final_influence", [0] * n)
        order_b = sorted(range(n), key=lambda i: (-fc[i], i))
        fbest = max(range(n), key=lambda i: (infl[i], -order_b.index(i)))
        # режим B (текущий): рента поверх держателю
        ranks_b, pay_b = settle(fc, bank, n, None, 0, fbest)
        pay_b[holder] += rent
        # режим A: рента в ранге — экзогенный поток входит в банк
        # и в кэш держателя ДО ранжирования (делится через доли)
        fc_a = list(fc)
        fc_a[holder] += rent
        ranks_a, pay_a = settle(fc_a, bank + rent, n, None, 0, fbest)
        place_b = ranks_b.index(holder) + 1
        place_a = ranks_a.index(holder) + 1
        print(f"\n=== {path.split('/')[-1]} (n={n}) ===")
        print(f"держатель: фракция {holder}, ставка {bid/PESO:.0f}M, "
              f"рента {rent/PESO:.0f}M, банк {bank/PESO:.0f}M")
        print(f"место держателя: B(текущее)={place_b} → A(рент-в-ранге)={place_a}")
        print(f"{'фр':>3} {'cash+hard':>10} {'B выплата':>10} {'A выплата':>10} {'дельта':>9}")
        for i in range(n):
            print(f"{i:>3} {fc[i]/PESO:>10.1f} {pay_b[i]/PESO:>10.1f} "
                  f"{pay_a[i]/PESO:>10.1f} {(pay_a[i]-pay_b[i])/PESO:>+9.1f}")
        # EV-вопрос: держатель выиграл/проиграл от режима A?
        d_holder = (pay_a[holder] - pay_b[holder]) / PESO
        others = sum(pay_a[i] - pay_b[i] for i in range(n) if i != holder) / PESO
        print(f"держатель при A: {d_holder:+.1f}M | остальные: {others:+.1f}M "
              f"(zero-sum check {d_holder + others:+.1f})")
        # жертва доли в текущем режиме: выплата без ставки против с ней
        fc_no = list(fc)
        fc_no[holder] += bid  # «как если бы не биддил»
        ranks_n, pay_n = settle(fc_no, bank, n, None, 0, fbest)
        place_n = ranks_n.index(holder) + 1
        sac = (pay_n[holder] - (pay_b[holder] - rent)) / PESO
        print(f"контрфакт без ставки: место было бы {place_n}, выплата "
              f"{pay_n[holder]/PESO:.1f}M; с аукционом (доля+рента): "
              f"{(pay_b[holder])/PESO:.1f}M; чистый эффект аукциона: "
              f"{(pay_b[holder]-pay_n[holder])/PESO:+.1f}M")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Отчёт «кто где манипулировал» по симуляционным партиям
(data/sim/games*.jsonl, формат simrun).

Определения зафиксированы (свод с expro_metric.py, поправка R13 из
REVIEW_EXTERNAL: карта 4 вынесена в отдельный класс):

  ANTI_RICH = {1 налог10, 2 налог20} — бьют по продающим, богатейший
              теряет больше всех;
  REDIST    = {4 субс_бедным} — перераспределение влияния: беднейшему
              +1 влияния, у богатейшего ничего не отбирается;
  EMBARGO   = {6} — рынок сжимается, штраф продавцам.

Экспроприация = закон класса ANTI_RICH прошёл, богатейший по cash на
момент голосования голосовал не-ЗА (против или воздержался).
Манипулятор = фракция, голосовавшая ЗА прошедший анти-богатый закон,
сама не будучи богатейшей.
Богатейший определяется по внутриигровому cash (старт 0, tracked по
sell/buy/bribe/donkey с каждого action)."""
import json, sys, collections

CN = {0: "статус-кво", 1: "налог10", 2: "налог20", 3: "субс_произв",
      4: "субс_бедным", 5: "субс_богатым", 6: "эмбарго", 7: "бум"}
ANTI = {1, 2}
REDIST = {4}
EMB = {6}

def law_class(card):
    if card in ANTI: return "анти-богатый"
    if card in REDIST: return "перераспределение"
    if card in EMB: return "эмбарго"
    return "нейтральный"

def analyse(path):
    games = [json.loads(l) for l in open(path)]
    rows = []            # каждая прошедшая манипуляция
    strat_stats = collections.defaultdict(lambda: dict(
        laws_voted_yes=0, manip=0, victim=0, vetoes=0, laws_passed_against=0))
    n_laws = n_expro = n_redist_against = n_veto_save = 0
    for g in games:
        strats = g["strategies"]; n = g["n_factions"]
        cash = [0] * n; infl = [1] * n
        for ph in g["phases"]:
            # трекаем состояние до фазы закона
            if ph["phase"] != "law":
                for a in ph["actions"]:
                    if not a["ok"]:
                        continue
                    if a["action"] == "bribe":
                        ia = a["detail"].get("influence_after")
                        if ia is not None:
                            infl[a["actor"]] = ia
                    if a.get("cash_after") is not None:
                        cash[a["actor"]] = a["cash_after"]
                continue
            card = None; votes = {}
            for a in ph["actions"]:
                if a["action"] == "vote_yes":
                    votes[a["actor"]] = "yes"; card = a["detail"].get("card", card)
                elif a["action"] == "vote_no":
                    votes[a["actor"]] = "no"; card = a["detail"].get("card", card)
                elif a["action"] == "vote_abstain":
                    votes[a["actor"]] = "abstain"; card = a["detail"].get("card", card)
                elif a["action"] == "veto":
                    votes[a["actor"]] = "veto"
            if card is None:
                continue
            cls = law_class(card)
            rich = max(range(n), key=lambda i: cash[i])
            rich_voted = votes.get(rich, "abstain")
            if ph.get("vetoed"):
                if card in ANTI and rich_voted != "yes":
                    n_veto_save += 1
                    strat_stats[strats[rich]]["vetoes"] += 1
                continue
            if not ph.get("law_passed"):
                continue
            n_laws += 1
            if card in ANTI and rich_voted != "yes":
                n_expro += 1
                yes_v = [(strats[i], infl[i]) for i, v in votes.items() if v == "yes"]
                for i, v in votes.items():
                    if v == "yes" and i != rich:
                        strat_stats[strats[i]]["manip"] += 1
                strat_stats[strats[rich]]["victim"] += 1
                rows.append(dict(game=g["game_id"], round=ph["round"],
                                 card=CN[card], rich=strats[rich],
                                 rich_voted=rich_voted,
                                 yes=", ".join(f"{s}({w})" for s, w in yes_v),
                                 yes_w=ph.get("yes_influence"), no_w=ph.get("no_influence")))
            elif card in REDIST and rich_voted != "yes":
                n_redist_against += 1
                rows.append(dict(game=g["game_id"], round=ph["round"],
                                 card=CN[card], rich=strats[rich],
                                 rich_voted=rich_voted,
                                 yes=", ".join(strats[i] for i, v in votes.items() if v == "yes"),
                                 yes_w=ph.get("yes_influence"), no_w=ph.get("no_influence")))
    return dict(games=len(games), laws=n_laws, expro=n_expro,
                redist=n_redist_against, veto_saves=n_veto_save,
                rows=rows, strat={k: v for k, v in strat_stats.items()})

def wilson(k, n, z=1.96):
    if n == 0: return (0.0, 0.0)
    p = k / n
    d = 1 + z*z/n
    c = (p + z*z/(2*n)) / d
    h = z * ((p*(1-p)/n + z*z/(4*n*n)) ** 0.5) / d
    return (max(0.0, c-h), min(1.0, c+h))

if __name__ == "__main__":
    paths = sys.argv[1:] or ["data/sim/games.jsonl", "data/sim/games5.jsonl"]
    total = dict(games=0, laws=0, expro=0, redist=0, veto_saves=0)
    agg = collections.defaultdict(lambda: dict(manip=0, victim=0, vetoes=0))
    for p in paths:
        r = analyse(p)
        tag = p.split("/")[-1]
        print(f"\n===== {tag}: партий {r['games']}, законов прошло {r['laws']}, "
              f"экспроприаций {r['expro']}, перераспределений против богатейшего {r['redist']}, "
              f"вето-спасений {r['veto_saves']} =====")
        for row in r["rows"][:12]:
            print(f"  партия {row['game']} r{row['round']} {row['card']}: богатейший {row['rich']} "
                  f"({row['rich_voted']}), ЗА: {row['yes']} | вес да {row['yes_w']} нет {row['no_w']}")
        if len(r["rows"]) > 12:
            print(f"  ... и ещё {len(r['rows'])-12}")
        for k, v in r["strat"].items():
            for kk in agg[k]:
                agg[k][kk] += v.get(kk, 0)
            for kk in ("manip", "victim", "vetoes"):
                agg[k][kk] += 0
        for k in ("games", "laws", "expro", "redist", "veto_saves"):
            total[k] += r[k]
    lo, hi = wilson(total["expro"], total["laws"])
    print(f"\n===== ИТОГО по {paths} =====")
    print(f"партий {total['games']}, прошедших законов {total['laws']}, "
          f"экспроприаций {total['expro']} ({total['expro']/max(1,total['laws'])*100:.0f}%, "
          f"Wilson95 [{lo*100:.0f}%, {hi*100:.0f}%]), "
          f"перераспределений против воли богатейшего {total['redist']}, "
          f"вето-спасений {total['veto_saves']}")
    print("по стратегиям (кто манипулировал / был жертвой / спасался вето):")
    for s, v in sorted(agg.items()):
        print(f"  {s:>8}: манипуляций {v['manip']}, жертв {v['victim']}, вето-спасений {v['vetoes']}")

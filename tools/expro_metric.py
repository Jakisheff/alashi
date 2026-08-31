#!/usr/bin/env python3
"""Метрика экспроприации по data/events_stream.jsonl (разовый расчёт,
воспроизводит число 5/9 для DECK/QA; определение зафиксировано).

Раунд экспроприации = закон класса «против богатейшего» (налог10=1,
налог20=2, субсидия бедным=4) прошёл, богатейшая по cash фракция на момент
голосования голосовала не-ЗА, перевес обеспечен остальными. Cash стартует
с entry_fee и трекается по Sold/GoodsBought/BribeGiven/DonkeyBought."""
import base64, collections, hashlib, struct

D = lambda n: hashlib.sha256(f"event:{n}".encode()).digest()[:8]
EV = {n: D(n) for n in ["FactionJoined", "Sold", "GoodsBought", "BribeGiven",
                        "VoteCast", "LawResult", "LawDrawn", "LawVetoed", "DonkeyBought", "Payout"]}
REV = {v: k for k, v in EV.items()}
u8 = lambda r, o: (r[o], o + 1)
u16 = lambda r, o: (struct.unpack_from("<H", r, o)[0], o + 2)
u32 = lambda r, o: (struct.unpack_from("<I", r, o)[0], o + 4)
u64 = lambda r, o: (struct.unpack_from("<Q", r, o)[0], o + 8)
pk = lambda r, o: (r[o:o + 32].hex(), o + 32)

def decode(name, r):
    o = 0
    if name == "FactionJoined":
        g, o = pk(r, o); f, o = pk(r, o); ln, o = u8(r, o)
        return dict(ev=name, game=g, faction=f, name=r[o:o+ln].decode("utf8", "ignore"))
    if name == "Sold":
        g, o = pk(r, o); f, o = pk(r, o); u, o = u16(r, o); rev, o = u64(r, o)
        return dict(ev=name, game=g, faction=f, revenue=rev)
    if name == "GoodsBought":
        g, o = pk(r, o); f, o = pk(r, o); u, o = u16(r, o); c, o = u64(r, o)
        return dict(ev=name, game=g, faction=f, cost=c)
    if name == "BribeGiven":
        g, o = pk(r, o); f, o = pk(r, o); t, o = pk(r, o); a, o = u64(r, o); ig, o = u16(r, o)
        return dict(ev=name, game=g, **{"from": f, "to": t, "amount": a, "infl": ig})
    if name == "VoteCast":
        g, o = pk(r, o); f, o = pk(r, o); c, o = u8(r, o)
        return dict(ev=name, game=g, faction=f, choice=c)
    if name == "LawResult":
        g, o = pk(r, o); rd, o = u8(r, o); y, o = u32(r, o); no, o = u32(r, o); p, o = u8(r, o)
        return dict(ev=name, game=g, round=rd, yes=y, no=no, passed=bool(p))
    if name == "LawDrawn":
        g, o = pk(r, o); rd, o = u8(r, o); c, o = u8(r, o)
        return dict(ev=name, game=g, round=rd, card=c)
    if name == "DonkeyBought":
        g, o = pk(r, o); f, o = pk(r, o); p, o = u64(r, o)
        return dict(ev=name, game=g, faction=f, price=p)
    return None

events = []
for line in open("data/events_stream.jsonl"):
    line = line.strip()
    if not line.startswith("Program data: "):
        continue
    b = base64.b64decode(line[len("Program data: "):])
    i = 0
    while i + 8 <= len(b):
        name = REV.get(b[i:i + 8])
        if name:
            d = decode(name, b[i + 8:])
            if d:
                events.append(d)
                i += 8
                continue
        i += 1

ANTI = {1, 2, 4}
CN = {0: "статус-кво", 1: "налог10", 2: "налог20", 3: "субс_произв", 4: "субс_бедным",
      5: "субс_богатым", 6: "эмбарго", 7: "бум", 255: "нет"}
CH = {0: "ЗА", 1: "ПРОТИВ", 2: "воздерж"}
games = collections.defaultdict(list)
for e in events:
    games[e["game"]].append(e)

tot = expo = 0
for g, evs in games.items():
    names = cash = {f: 50_000_000 for f in {e["faction"] for e in evs if e.get("faction")}}
    names = {}
    votes = collections.defaultdict(dict); drawn = {}; cur = 0
    for e in evs:
        t = e["ev"]
        if t == "FactionJoined":
            names[e["faction"]] = e["name"]; cash[e["faction"]] = 50_000_000
        elif t == "Sold": cash[e["faction"]] += e["revenue"]
        elif t == "GoodsBought": cash[e["faction"]] -= e["cost"]
        elif t == "BribeGiven": cash[e["from"]] -= e["amount"]
        elif t == "DonkeyBought": cash[e["faction"]] -= e["price"]
        elif t == "LawDrawn": cur = e["round"]; drawn[cur] = e["card"]; votes[cur] = {}
        elif t == "VoteCast": votes[cur][e["faction"]] = e["choice"]
        elif t == "LawResult" and e["passed"] and drawn.get(e["round"]) is not None:
            tot += 1
            rich = max(cash, key=lambda f: cash[f])
            if drawn[e["round"]] in ANTI and votes[e["round"]].get(rich, 2) != 0 and e["yes"] > e["no"]:
                expo += 1
                card_name = CN[drawn[e["round"]]]
                rd = e["round"]; yn = e["yes"]; nn = e["no"]; rn = names.get(rich)
                print(f"{card_name} r{rd}: богатейший {rn} ПРОТИВ, yes={yn} no={nn} — ЭКСПРОПРИАЦИЯ")
print(f"прошедших законов: {tot}, экспроприаций: {expo}")

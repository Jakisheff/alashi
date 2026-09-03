#!/usr/bin/env python3
# Драйвер Agent1 (gid 7, фракция 3): механическое исполнение матрицы
# /tmp/alashi_g11_matrix.md. Решения заготовлены ДО фаз; в живой фазе
# только исполнение. Тест гипотезы Г5' (0 WrongPhase).
import json, time, urllib.request, sys

BASE = "http://localhost:8090"
G = 8
TOK = open("/tmp/alashi_g12_token.txt").read().strip()
ME = 5
M = 1_000_000
LOG = open("/tmp/opencode/g12_driver.log", "a", buffering=1)
WRONG = 0  # счётчик ошибок фаз


def log(*a):
    s = time.strftime("[%H:%M:%S]") + " " + " ".join(str(x) for x in a)
    LOG.write(s + "\n")


def api(path, body=None, timeout=70):
    if body is None:
        r = urllib.request.urlopen(f"{BASE}{path}", timeout=timeout)
    else:
        req = urllib.request.Request(
            f"{BASE}{path}", data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json"})
        r = urllib.request.urlopen(req, timeout=timeout)
    return json.load(r)


def act(action, params):
    global WRONG
    r = api(f"/game/{G}/act", {"token": TOK, "action": action,
                               "params": params, "by": "llm"})
    if r.get("ok"):
        log("ACT ok", action, params, "cash:", r.get("cash_after"),
            "goods:", r.get("goods_after"), str(r.get("detail", ""))[:120])
    else:
        WRONG += 1
        log("ACT ERR", action, params, "->", r.get("error"))
    return r


def fam(s):
    return {f["name"]: f for f in s["factions"]}


def me(s):
    return s["factions"][ME]


def rivals(s):
    return [f for f in s["factions"] if f["idx"] != ME]


def poorest_rival(s):
    return min(rivals(s), key=lambda f: f["cash"])["idx"]


def rank_by_cash(s):
    v = sorted(s["factions"], key=lambda f: -(f["cash"] + f["hard"]))
    return [f["idx"] for f in v]


def my_influence_lead(s):
    others = max(f["influence"] for f in rivals(s))
    return me(s)["influence"] - others


# ---------- фазовые ветки матрицы ----------

def do_market(s):
    r, m = s["round"], me(s)
    if m["goods"] == 0:
        log(f"r{r} market: 0 товаров — пропуск по матрице")
        return
    if r == 5 and s.get("president_idx") == ME:
        act("sell_credit", {"units": m["goods"]})
    else:
        act("sell", {"units": m["goods"]})


def do_action(s):
    r, m = s["round"], me(s)
    if r == 1:
        act("produce", {})
        return
    # r2+: я планировал стать президентом со взяткой r2
    if s.get("president_idx") == ME and not s["customs_decided"]:
        act("customs", {"tight": False})
    if r == 2:
        if m["cash"] >= 5 * M:
            act("bribe", {"to": poorest_rival(s), "amount": 5 * M})
        else:
            log("r2: кэша нет на взятку — produce")
            act("produce", {})
        if s.get("president_idx") == ME and not s["customs_decided"]:
            act("customs", {"tight": False})
        return
    if r == 3:
        act("shuttle", {})
        return
    if r == 4:
        ins = act("inspect_license", {})
        y = ins.get("detail", {}).get("license_yield")
        cash = me(s)["cash"]
        if y is None:
            log("r4: инсайда нет — не бидить")
        else:
            b = max(10 * M, min(y - 15 * M, 30 * M))
            b = min(b, cash)
            if b >= 10 * M:
                act("bid_license", {"amount": b})
            else:
                log("r4: кэша не хватает на минимальную ставку", cash)
        act("shuttle", {})
        return
    if r == 5:
        if my_influence_lead(s) <= 1 and m["cash"] >= 6 * M:
            act("bribe", {"to": poorest_rival(s), "amount": 5 * M})
        else:
            act("shuttle", {})
        return
    if r == 6:
        if my_influence_lead(s) <= 1 and m["cash"] >= 6 * M:
            act("bribe", {"to": poorest_rival(s), "amount": 5 * M})
        else:
            log("r6 action: по матрице ничего (товар не продать)")


def do_law(s):
    card = s.get("law_card_name")
    m = me(s)
    pres = s.get("president_idx") == ME
    rk = rank_by_cash(s)
    top2 = ME in rk[:2]
    log(f"law r{s['round']} карта={card} президент={pres} мой_ранг_кэша="
        f"{rk.index(ME)+1} вексель={m['promissory']//M}M")
    if card in ("tax_10", "tax_20"):
        if pres and top2:
            act("veto", {})
        else:
            act("vote", {"choice": "no"})
    elif card == "embargo":
        if pres:
            act("veto", {})
        else:
            act("vote", {"choice": "no"})
    elif card == "vzaimozachet":
        if m["promissory"] > 0:
            if pres:
                act("veto", {})
            else:
                act("vote", {"choice": "no"})
        else:
            act("vote", {"choice": "yes"})
    elif card == "boom":
        act("vote", {"choice": "yes"})
    elif card == "subsidy_produce":
        act("vote", {"choice": "yes"})
    elif card == "subsidy_poor":
        poorest = min(s["factions"], key=lambda f: f["cash"])["idx"]
        act("vote", {"choice": "yes" if poorest == ME else "no"})
    elif card == "subsidy_rich":
        act("vote", {"choice": "yes" if rk[0] == ME else "no"})
    elif card == "status_quo":
        act("vote", {"choice": "abstain"})
    else:
        log("неизвестная карта", card, "— abstain")
        act("vote", {"choice": "abstain"})



def write_report(res):
    """Единый отчёт по inbox/TEMPLATE.md — тот же формат, что у бинарника agent."""
    import os
    M = 10**6
    me_name = "Agent1"
    r = res.get("ranks", [])
    if not r:
        return
    place = r.index(ME) + 1 if ME in r else 99
    pay = res.get("payouts", [0]*8)[ME] if ME < len(res.get("payouts", [])) else 0
    brk = (res.get("payout_breakdown") or [{}]*8)[ME] if ME < len(res.get("payout_breakdown", [])) else {}
    ags = res.get("agents", [])
    L = [f"# Отчёт {me_name} — игра {G} ({time.strftime('%d.%m.%Y')})", "",
         f"- модель: agent-one (интерактивный драйвер-матрица), agent_id: {ags[ME].get('agent_id','?')[:8] if ME < len(ags) else '?'}",
         f"- место: {place} из {len(r)}, выплата: {pay/M:.1f}M песо (ранг {brk.get('rank_share',0)/M:.1f}M / рента {brk.get('license_rent',0)/M:.1f}M / завод {brk.get('factory_bonus',0)/M:.1f}M)", "",
         "## Таблица партии (все фракции)", "",
         "| место | фракция | модель | final cash+hard (M) | выплата (M) |", "|---|---|---|---|---|"]
    fc = res.get("final_cash", [])
    for p, i in enumerate(r):
        a = ags[i] if i < len(ags) else {}
        L.append(f"| {p+1} | {a.get('name','?')} | {a.get('model','?')} | {fc[i]/M:.1f} | {res['payouts'][i]/M:.1f} |")
    L += ["", "## Ошибки и отказы", f"- {WRONG} за партию (счётчик драйвера)", "",
          "## Вывод одной строкой", f"- место {place} из {len(r)}, выплата {pay/M:.1f}M"]
    d = f"{os.environ.get('ALASHI_INBOX', os.path.expanduser('~/Desktop/alashi/inbox'))}/{me_name}"
    os.makedirs(d, exist_ok=True)
    path = f"{d}/game{G}_{int(time.time())}.md"
    open(path, "w").write("\n".join(L))
    log("REPORT:", path)

# ---------- главный цикл: /wait long-poll ----------

log("=== driver start, faction", ME, "===")
known_r, known_p = 0, "lobby"
handled = set()
while True:
    try:
        w = api(f"/game/{G}/wait?r={known_r}&p={known_p}&t=60")
    except Exception as e:
        log("wait error:", e)
        time.sleep(2)
        continue
    s = w["state"]
    if s.get("finished"):
        res = api(f"/game/{G}/state")
        log("FINISHED:", json.dumps(res.get("result", {}), ensure_ascii=False))
        write_report(res.get("result", {}))
        break
    if not w.get("changed"):
        continue
    known_r, known_p = s["round"], s["phase"]
    key = (s["round"], s["phase"])
    if key in handled:
        continue
    handled.add(key)
    m = me(s)
    log(f"ФАЗА r{s['round']} {s['phase']} | cash={m['cash']//M}M goods={m['goods']} "
        f"prom={m['promissory']//M}M infl={m['influence']} pres={m['is_president']} "
        f"| sold={s['sold_counter']} price={s['price_now']}")
    try:
        if s["phase"] == "market":
            do_market(s)
        elif s["phase"] == "action":
            do_action(s)
        elif s["phase"] == "law":
            do_law(s)
    except Exception as e:
        log("PHASE HANDLER ERROR:", repr(e))
log("=== driver exit, WrongPhase/ошибок:", WRONG, "===")

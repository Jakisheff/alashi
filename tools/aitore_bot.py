#!/usr/bin/env python3
# Aitore (glm-5.3) — бот партии Alashi на HTTP-арене
# Стратегия: гонка продаж в Market (первые цены), shuttle в Action,
# plain sell + buy_hard (раунды 1-4), кэш в 5-6, инспект+бид лицензии r4,
# голосование по таблице, вето налогов президентом, кранк фаз.
import json, time, urllib.request

BASE = "https://pack-handled-effective-mag.trycloudflare.com"
GAME = 1
ME = 4
NAME = "Aitore"
MODEL = "glm-5.3"
PROMPT = "играю от имени Аммира: осторожный стиль, торгую по price_table, коплю cash, взяток не даю"
TOKEN_DIR = "/Users/amir/Desktop/alashi/data/live"
TOKEN = None  # загружается с диска при старте, см. ensure_identity
LOG = open("/tmp/aitore_p21.log", "a", buffering=1)


def token_path():
    import hashlib
    key = hashlib.sha256(f"{BASE}|{GAME}".encode()).hexdigest()[:12]
    return f"{TOKEN_DIR}/aitore_token_{key}.json"


def save_token_atomic(rec):
    """Немедленная запись токена на диск: tmp + fsync + rename.
    Урок TOKEN_RECOVERY.md: токен живёт только в ответе join; если он не
    на диске через секунду после join, дебаггер по памяти процесса это
    уже инцидент-режим, а не штатный путь."""
    import os
    os.makedirs(TOKEN_DIR, exist_ok=True)
    path = token_path()
    tmp = path + ".tmp"
    with open(tmp, "w") as fh:
        json.dump(rec, fh, ensure_ascii=False, indent=1)
        fh.flush()
        os.fsync(fh.fileno())
    os.replace(tmp, path)
    log("TOKEN-SAVED", path)


def ensure_identity():
    """Токен с диска; если файла нет — join и мгновенная запись на диск."""
    global TOKEN, ME
    import os
    path = token_path()
    if os.path.exists(path):
        rec = json.load(open(path))
        TOKEN = rec["token"]
        ME = rec.get("faction_idx", ME)
        log("TOKEN-LOADED", path, "faction", ME)
        return
    body = {"name": NAME, "model": MODEL, "prompt": PROMPT}
    r = http("/game/{}/join".format(GAME), body, timeout=20)
    if not r or not r.get("ok"):
        log("JOIN-FAIL", json.dumps(r)[:200])
        raise SystemExit(1)
    rec = {
        "base": BASE,
        "game_id": GAME,
        "party_no": r.get("state", {}).get("party_no"),
        "agent_id": r.get("agent_id"),
        "faction_idx": r.get("faction_idx", ME),
        "name": NAME,
        "model": MODEL,
        "token": r["token"],
        "created_unix": int(time.time()),
    }
    ME = rec["faction_idx"]
    TOKEN = rec["token"]
    save_token_atomic(rec)

def log(*a):
    LOG.write(time.strftime("[%H:%M:%S]") + " " + " ".join(str(x) for x in a) + "\n")

def http(path, body=None, timeout=15, retries=4):
    """Транспортный уровень: сеть/туннель -> ретраи с бэкоффом 1-2-4-8.
    Игровой протокол (JSON с ok:false внутри HTTP 200) сюда не попадает:
    это не транспортная ошибка, её разбирает act(). Урок партии №20:
    статичный sleep(2) при падении туннеля сжигал окно фазы."""
    import urllib.error
    url = BASE + path
    data = json.dumps(body).encode() if body is not None else None
    delay = 1.0
    for attempt in range(retries):
        try:
            req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
            with urllib.request.urlopen(req, timeout=timeout) as r:
                return json.loads(r.read())
        except urllib.error.HTTPError as e:
            try:
                return json.loads(e.read())
            except Exception:
                if 500 <= e.code:
                    log("HTTP-5XX", path.split("?")[0], e.code, "attempt", attempt, f"backoff={delay:.0f}s")
                    time.sleep(delay)
                    delay *= 2
                    continue
                log("HTTP-4XX", path.split("?")[0], e.code)
                return None
        except Exception as e:
            log("HTTP-TRANSPORT", path.split("?")[0], repr(e)[:100], "attempt", attempt, f"backoff={delay:.0f}s")
            time.sleep(delay)
            delay *= 2
    return None

def act(action, params=None, fresh=False, retry=True):
    """Отправка хода. fresh=True: лёгкий пре-чек допущений по только что
    подтянутому state (урок партии 21: бид 11M при живом кэше 7M, счёт по
    устаревшему снапшоту до инспекта). Гонка продаж идёт без пре-чека:
    сервер сам отклонит невалидный ход, а полсекунды на /state стоят
    позиции в очереди цен."""
    if fresh:
        global phase_calls
        st = http(f"/game/{GAME}/state")
        phase_calls += 1
        if st and st.get("state"):
            s2 = st["state"]
            f2 = s2["factions"][ME]
            if s2["now"] >= s2["phase_ends_at"] - 2:
                log("PRECHECK-VETO (окно фазы закрыто)", action)
                return {"ok": False, "error": "precheck:window", "precheck": True}
            why = validate(s2, f2, action, params or {})
            if why:
                log("PRECHECK-VETO", action, json.dumps(params or {}), "->", why)
                return {"ok": False, "error": "precheck:" + why, "precheck": True}
        else:
            log("PRECHECK-SKIP (нет state)", action)
    body = {"token": TOKEN, "action": action, "by": "llm-glm5.3"}
    if params is not None:
        body["params"] = params
    r = http(f"/game/{GAME}/act", body)
    if r is None:
        log("ACT-FAIL (транспорт исчерпан)", action)
        return None
    if r.get("ok") is not True and not r.get("precheck"):
        err = r.get("error")
        if err in ("AlreadyActed", "AlreadyVoted"):
            # почти наверняка свой повтор, чей ответ потерялся в канале
            log("ACT-ALREADY (считаем выполненным)", action)
            return r
        if err in ("NotEnoughCash", "NotEnoughGoods") and retry and params:
            # игровой фидбек: в ответе лежит свежий state, правим суммы
            # и пробуем один раз снова, без слепого sleep
            s2 = r.get("state") or {}
            fs = s2.get("factions") or []
            f2 = fs[ME] if len(fs) > ME else {}
            if action == "bid_license":
                amt = min(int(params.get("amount", 0)), f2.get("cash", 0) - 200_000)
                if amt >= 1_000_000:
                    log("ACT-RETRY (поправка ставки)", amt)
                    return act("bid_license", {"amount": amt}, fresh=False, retry=False)
            elif action in ("sell", "sell_credit"):
                u = min(int(params.get("units", 0)), f2.get("goods", 0))
                if u >= 1:
                    log("ACT-RETRY (поправка лота)", u)
                    return act(action, {"units": u}, fresh=False, retry=False)
    log("ACT", action, json.dumps(params or {}), "->", "ok" if r.get("ok") else f"err={r.get('error')}")
    return r


def validate(s, f, action, params):
    """Пре-чек без LLM: сверка хода с числами state. None = ход согласован."""
    ph = s.get("phase")
    r = s.get("round")
    if action in ("sell", "sell_credit", "buy"):
        if ph != "market":
            return f"phase={ph}"
        if f["acted"]:
            return "acted"
        if action != "buy":
            u = params.get("units", 0)
            if not 1 <= u <= f["goods"]:
                return f"units={u} goods={f['goods']}"
    if action in ("produce", "shuttle", "donkey", "bribe", "roof"):
        if ph != "action":
            return f"phase={ph}"
        if f["acted"]:
            return "acted"
        if action == "bribe":
            if params.get("amount", 0) > f["cash"]:
                return f"bribe={params.get('amount')} cash={f['cash']}"
            if params.get("to") == ME:
                return "bribe to self"
    if action == "customs":
        if s.get("president_idx") != ME:
            return "not president"
        if s.get("customs_decided"):
            return "customs decided"
    if action == "bid_license":
        if ph != "action":
            return f"phase={ph}"
        if r != s.get("license_auction", {}).get("round", 4):
            return f"round={r}"
        if s.get("license_auction", {}).get("sold"):
            return "license sold"
        amt = params.get("amount", 0)
        if amt <= 0:
            return "bid<=0"
        # cash в state уже за вычетом эскроу и платы за инспект:
        # именно это сравнение провалило бид 11M при кэше 7M в №21
        if amt > f["cash"]:
            return f"bid={amt} cash={f['cash']}"
    if action == "inspect_license":
        if f["cash"] < 5_000_000:
            return f"cash={f['cash']} < 5M"
    if action == "buy_hard" and f["cash"] <= 0:
        return "cash=0"
    if action == "sell_hard" and f["hard"] <= 0:
        return "hard=0"
    if action == "vote":
        if ph != "law":
            return f"phase={ph}"
        if f["voted"]:
            return "voted"
        if params.get("choice") not in ("yes", "no", "abstain"):
            return f"choice={params.get('choice')}"
    if action == "veto" and not f["is_president"]:
        return "not president"
    if action in ("offer_vote", "accept_vote_offer") and ph != "law":
        return f"phase={ph}"
    return None

def snapshot(s, tag):
    f = s["factions"][ME]
    row = " | ".join(
        f"{x['name']} c{x['cash']//10**6} g{x['goods']} h{x['hard']//10**6} i{x['influence']}"
        for x in s["factions"])
    log(tag, f"r{s['round']}/{s['phase']} price={s['price_now']} sold={s['sold_counter']}",
        f"law={s.get('law_card_name')}", f"pres={s.get('president_idx')}")
    log("  ", row)
    log("   me:", f"cash={f['cash']} goods={f['goods']} hard={f['hard']} prom={f['promissory']} acted={f['acted']} voted={f['voted']}")

def my_rank_cash(s):
    f = s["factions"][ME]
    mine = f["cash"]
    richer = sum(1 for x in s["factions"] if x["idx"] != ME and x["cash"] > mine)
    poorer = sum(1 for x in s["factions"] if x["idx"] != ME and x["cash"] < mine)
    return richer, poorer

def do_market(s):
    f = s["factions"][ME]
    r = s["round"]
    if not f["acted"] and f["goods"] > 0:
        act("sell", {"units": f["goods"]})
        st = http(f"/game/{GAME}/state")
        if st:
            f = st["state"]["factions"][ME]
        # конвертация: раунды 1-3 весь кэш в hard; r4 держим кэш для лицензии
        # (buy_hard конвертирует всё сразу, частичного нет)
        if r <= 3 and f["cash"] > 2_000_000:
            act("buy_hard", fresh=True)
    if s.get("barter_offers"):
        log("BARTER-OFFERS", json.dumps(s["barter_offers"]))

def do_action(s):
    f = s["factions"][ME]
    r = s["round"]
    if s.get("president_idx") == ME and not s.get("customs_decided"):
        act("customs", {"tight": False})  # сам вожу серое — льготная граница + дань
    if not f["acted"]:
        act("produce")  # после конфиската r2: только белый товар, гарантия
    if r == 4:
        st = http(f"/game/{GAME}/state")
        if st:
            s2 = st["state"]; f2 = s2["factions"][ME]
            if f2["cash"] >= 5_000_000 and not f2.get("insider", False):
                r2 = act("inspect_license", fresh=True)
                # вытащим yield из action_log.detail
                y = None
                try:
                    y = r2["action_log"]["detail"]["license_yield"]
                except Exception:
                    pass
                log("LICENSE-YIELD", y)
                if y and y >= 30_000_000:
                    f2 = http(f"/game/{GAME}/state")["state"]["factions"][ME]
                    bid = min(f2["cash"] - 200_000, int(y * 1.3))
                    if bid >= 1_000_000:
                        act("bid_license", {"amount": bid}, fresh=True)
                        log("LICENSE-BID", bid)

def vote_choice(s):
    name = (s.get("law_card_name") or "").lower()
    richer, poorer = my_rank_cash(s)
    if "tax" in name:
        return "no"
    if "embargo" in name:
        return "no"
    if "amnesty" in name:
        return "no"
    if "poor" in name:
        return "yes" if poorer == 0 else "no"   # беднейший = никто не беднее меня
    if "rich" in name:
        return "yes" if richer == 0 else "no"
    if "boom" in name or "produce" in name:
        return "yes"
    return "abstain"

def do_law(s):
    f = s["factions"][ME]
    r = s["round"]
    if r == 6 and not f["voted"] and not f.get("vote_sold"):
        # финальный рывок за 4-е место: продать голос лидеру
        act("offer_vote", {"to": 3, "price": 4_000_000})
        st = http(f"/game/{GAME}/state")
        if st:
            f = st["state"]["factions"][ME]
    if f["is_president"]:
        name = (s.get("law_card_name") or "").lower()
        if "tax" in name or "embargo" in name:
            act("veto")
            f = http(f"/game/{GAME}/state")["state"]["factions"][ME]
    if not f["voted"]:
        act("vote", {"choice": vote_choice(s)})
    # r4 после вскрытия аукциона: остаток кэша в hard (остались деvals r5,r6)
    if r == 4:
        st = http(f"/game/{GAME}/state")
        if st:
            f2 = st["state"]["factions"][ME]
            if f2["cash"] > 2_000_000:
                act("buy_hard", fresh=True)

DEADLINE_MARGIN_S = 8   # запас до конца фазы, после которого только safe-ход
MAX_CALLS_PER_PHASE = 14
phase_calls = 0


def safe_final(s):
    """Graceful-финал (паттерн ORE/rlm-daytona): бюджет шагов или времени
    фазы исчерпан -> обязан быть отправлен валидный ход, самый безопасный
    из доступных, а не молчание до таймаута."""
    f = s["factions"][ME]
    ph = s["phase"]
    if ph == "market" and not f["acted"] and f["goods"] > 0:
        log("GRACEFUL-FINAL: sell", f["goods"])
        return act("sell", {"units": f["goods"]})
    if ph == "action" and not f["acted"]:
        log("GRACEFUL-FINAL: produce")
        return act("produce")
    if ph == "law" and not f["voted"]:
        choice = vote_choice(s) if s.get("law_card_name") else "abstain"
        log("GRACEFUL-FINAL: vote", choice)
        return act("vote", {"choice": choice})
    log("GRACEFUL-FINAL: обязательного хода нет, фаза", ph)


def mandatory_move_missing(s):
    f = s["factions"][ME]
    ph = s["phase"]
    if ph == "market":
        return (not f["acted"]) and f["goods"] > 0
    if ph == "action":
        return not f["acted"]
    if ph == "law":
        return not f["voted"]
    return False


def handle(s):
    global phase_calls
    phase_calls = 0
    snapshot(s, "PHASE")
    ph = s["phase"]
    if ph == "market":
        do_market(s)
    elif ph == "action":
        do_action(s)
    elif ph == "law":
        do_law(s)
    elif ph == "finished":
        log("FINISHED", json.dumps(s.get("recent_actions", [])[-3:]))
        return False
    # пост-проверка: окно почти закрыто или бюджет вызовов съеден,
    # а обязательный ход так и не ушёл -> безопасный ход без размышлений
    st = http(f"/game/{GAME}/state")
    if st and st.get("state"):
        s2 = st["state"]
        near = s2["now"] >= s2["phase_ends_at"] - DEADLINE_MARGIN_S
        if mandatory_move_missing(s2) and (near or phase_calls >= MAX_CALLS_PER_PHASE):
            safe_final(s2)
    return True

def main():
    ensure_identity()
    log("=== Aitore bot start, faction", ME, "===")
    last = None
    while True:
        if last is None:
            r = http(f"/game/{GAME}/state")
        else:
            r = http(f"/game/{GAME}/wait?r={last[0]}&p={last[1]}&t=50", timeout=60)
        if r is None:
            time.sleep(8)
            continue
        s = r.get("state")
        if s is None:
            log("NO-STATE", json.dumps(r)[:200])
            time.sleep(8)
            continue
        key = (s["round"], s["phase"])
        if key != last:
            last = key
            if not handle(s):
                break
            continue
        # кранк: дедлайн+грейс прошёл, фаза та же — толкаем
        if s["phase"] not in ("lobby", "finished") and s["now"] >= s.get("grace_until", 0) + 1:
            cr = http(f"/game/{GAME}/advance", {})
            log("CRANK", "ok" if (cr and cr.get("ok")) else f"err={cr.get('error') if cr else 'none'}")
        time.sleep(1)
    log("=== bot exit ===")

if __name__ == "__main__":
    main()

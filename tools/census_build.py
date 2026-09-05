#!/usr/bin/env python3
"""Перепись alashi (по Баладжи, «не проси доверия, показывай работу»):
собирает публичные цифры арены в самодостаточный docs/census.html.

Источники: data/registry.json (агенты), data/sim/*.jsonl (сим-партии),
data/live/ (живые партии), docs/MANIPULATION_REPORT.md (сводки).
Пересчёт одной командой: python3 tools/census_build.py"""
import json, glob, hashlib, os, datetime, re, subprocess
from html import escape

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def load(p):
    return [json.loads(l) for l in open(os.path.join(ROOT, p))]

# агенты
agents = []
rp = os.path.join(ROOT, "data/registry.json")
if os.path.exists(rp):
    agents = json.load(open(rp))

# сим-партии
games = {}
for path in sorted(glob.glob(os.path.join(ROOT, "data/sim/games*.jsonl"))):
    tag = os.path.basename(path).replace(".jsonl", "")
    games[tag] = load(f"data/sim/{os.path.basename(path)}")

n_games = sum(len(v) for v in games.values())
laws = {}
n_laws = 0
n_bribes = 0
n_vetoes = 0
n_actions = 0
for tag, gs in games.items():
    for g in gs:
        for ph in g["phases"]:
            n_actions += len(ph["actions"])
            if ph["phase"] == "law":
                if ph.get("law_passed"):
                    n_laws += 1
                if ph.get("vetoed"):
                    n_vetoes += 1
            for a in ph["actions"]:
                if a["action"] == "bribe" and a["ok"]:
                    n_bribes += 1
                d = a.get("detail") or {}
                if ph["phase"] == "law" and a["ok"] and "card" in d:
                    laws[d["card"]] = laws.get(d["card"], 0) + 1

CN = {0: "статус-кво", 1: "налог10", 2: "налог20", 3: "субс_произв",
      4: "субс_бедным", 5: "субс_богатым", 6: "эмбарго", 7: "бум"}

# Archived exports, not a hard-coded historical snapshot.
live = []
for path in glob.glob(os.path.join(ROOT, "data/live/party*_export.json")):
    records = json.load(open(path))
    for record in records if isinstance(records, list) else [records]:
        number = record.get("party_no") or int(re.search(r"party(\d+)", os.path.basename(path)).group(1))
        agents_record = record.get("agents", [])
        payouts = record.get("payouts", [])
        top = max(range(len(payouts)), key=payouts.__getitem__) if payouts else None
        winner = (f"{agents_record[top]['name']} ({payouts[top] / 1_000_000:.2f}M)"
                  if top is not None and top < len(agents_record) else "нет данных")
        finished = record.get("finished_at")
        date = (datetime.datetime.fromtimestamp(finished, datetime.timezone(datetime.timedelta(hours=5))).strftime("%d.%m")
                if isinstance(finished, (int, float)) else "дата не записана")
        live.append({"game": number, "date": date,
                     "names": ", ".join(a.get("name", "?") for a in agents_record), "winner": winner})
live.sort(key=lambda row: row["game"])

metr = {}
mp = os.path.join(ROOT, "data/sim/metrics.json")
if os.path.exists(mp):
    metr = json.load(open(mp))

now = subprocess.check_output(["date", "+%d.%m %H:%M"], env=dict(os.environ, TZ="Asia/Almaty"), text=True).strip()
law_rows = "".join(
    f"<tr><td>{CN.get(k, k)}</td><td>{v}</td></tr>"
    for k, v in sorted(laws.items(), key=lambda x: -x[1]))
live_rows = "".join(
    f"<tr><td>№{l['game']} · {l['date']}</td><td>{escape(l['names'])}</td><td>{escape(l['winner'])}</td></tr>"
    for l in live) or "<tr><td colspan=3>первая живая партия в данных</td></tr>"
agent_rows = "".join(
    f"<tr><td>{escape(a.get('name', a.get('agent_id', '?')[:12]))}</td><td>{escape(a.get('model', ''))}</td></tr>"
    for a in agents) or "<tr><td colspan=2>реестр пуст</td></tr>"

openskill = ""
for tag in ("games", "games5"):
    if tag in metr:
        r = metr[tag].get("openskill", [])
        openskill += f"<tr><td>{tag}</td><td>" + " · ".join(
            f"{x['strategy']} {x['mu']:.1f}" for x in r) + "</td></tr>"

html = f"""<!DOCTYPE html>
<html lang="ru"><head><meta charset="utf-8"><title>Alashi — перепись</title>
<style>
 body{{background:#0e1116;color:#e8ecf1;font-family:-apple-system,sans-serif;padding:20px;max-width:860px;margin:0 auto}}
 h1{{font-size:22px}} .sub{{color:#8b95a5;font-size:13px;margin-bottom:16px}}
 .grid{{display:flex;gap:10px;flex-wrap:wrap;margin-bottom:12px}}
 .card{{flex:1 1 150px;background:#171c24;border:1px solid #2a3240;border-radius:12px;padding:14px}}
 .num{{font-size:30px;font-weight:800;color:#f5c542}} .cap{{color:#8b95a5;font-size:12px;margin-top:4px}}
 table{{width:100%;border-collapse:collapse;font-size:13px;background:#171c24;border-radius:12px}}
 td{{padding:6px 10px;border-bottom:1px solid #2a3240}} td:last-child{{text-align:right;color:#8b95a5}}
 h2{{font-size:15px;color:#8b95a5;margin:18px 0 8px;text-transform:uppercase;letter-spacing:.8px}}
 .foot{{color:#8b95a5;font-size:11px;margin-top:16px}}
</style></head><body>
<h1>ALASHI · перепись</h1>
<div class="sub">«Не проси доверия, показывай работу». Все цифры пересчитываются
скриптом tools/census_build.py из открытых данных репозитория.</div>

<div class="grid">
 <div class="card"><div class="num">{n_games}</div><div class="cap">партий в датасете (сим-серии)</div></div>
 <div class="card"><div class="num">{n_laws}</div><div class="cap">прошедших законов</div></div>
 <div class="card"><div class="num">{n_vetoes}</div><div class="cap">вето президента</div></div>
 <div class="card"><div class="num">{n_bribes}</div><div class="cap">взяток за влияние</div></div>
 <div class="card"><div class="num">{n_actions}</div><div class="cap">ходов записано</div></div>
</div>

<h2>Архив HTTP-партий: {len(live)} экспортов</h2>
<table><tr><td>партия</td><td>состав</td><td>максимальная выплата</td></tr>{live_rows}</table>

<h2>Законы по типам (голосования)</h2>
<table>{law_rows}</table>

<h2>Рейтинг стратегий (OpenSkill, mu)</h2>
<table>{openskill}</table>

<h2>Реестр агентов</h2>
<table>{agent_rows}</table>

<div class="foot">Сгенерировано {now} · данные: data/sim, data/live, data/registry.json ·
репозиторий alashi</div>
</body></html>"""
open(os.path.join(ROOT, "docs/census.html"), "w").write(html)
print(f"census.html: {n_games} партий, {n_laws} законов, {len(agents)} агентов в реестре")

#!/usr/bin/env python3
"""Actor explorer: нейтральный вид акторов alashi по-имённо.

Паттерн заимствован у Urban Game Theory (actor-explorer.html): один
нейтральный список акторов с их активностью, контроллером решения
и ссылками на доказательства. Принцип alashi сохранён: «не проси
доверия, показывай работу» — каждая цифра на странице пересчитывается
из закоммиченных артефактов одной командой:

    python3 tools/actor_explorer_build.py [--write docs/actor_explorer.html]

Источники (только закоммиченные сырые данные):
  data/live/party*_90s_export.json   живые партии HTTP-арены: действия
                                     с полем by = llm | fallback | unknown
                                     (кто принял решение) и финалами
  data/registry.json                 регистрация агентов: agent_id, модель,
                                     промпт, кошелёк devnet
  docs/ops/world-100-*/audit.json    публичные аудиты 100-агентных миров:
                                     per-agent участие, сеттлмент, хеши
                                     доказательств, ограничения

Страница детерминирована: без часов, порядок фиксирован, внизу sha256
всех входов. Разница с docs/census.html: перепись показывает агрегаты,
эта страница показывает каждого актора отдельно и кто принимал каждое
решение. Контроллеры честно разделены: llm / fallback / unknown;
модели миров — peer-reported, не аттестованы.
"""
import argparse
import glob
import hashlib
import json
import os
import sys
from collections import Counter
from html import escape

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Порядок контроллеров фиксирован: от полного модельного решения к неизвестному
CONTROLLERS = ["llm", "fallback", "unknown"]


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def load(path):
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def short(value, n=8):
    return (value or "")[:n] + "…" + (value or "")[-4:] if value and len(value) > n + 4 else (value or "")


def fmt_micro(value):
    try:
        return f"{value / 1_000_000:.2f}M"
    except TypeError:
        return "—"


def collect_sources():
    parties = sorted(glob.glob(os.path.join(ROOT, "data/live/party*_90s_export.json")))
    registry_path = os.path.join(ROOT, "data/registry.json")
    audits = sorted(glob.glob(os.path.join(ROOT, "docs/ops/world-100-*/audit.json")))
    return parties, registry_path, audits


def read_parties(paths):
    out = []
    for p in paths:
        d = load(p)
        acts = d.get("actions", [])
        by = Counter(a.get("by") or "unknown" for a in acts)
        ok = sum(1 for a in acts if a.get("ok"))
        out.append({
            "file": os.path.relpath(p, ROOT),
            "game_id": d.get("game_id"),
            "finished_at": d.get("finished_at"),
            "n_factions": d.get("n_factions"),
            "agents": d.get("agents", []),
            "bank": d.get("bank"),
            "payouts": d.get("payouts", []),
            "actions_total": len(acts),
            "actions_list": acts,
            "ok": ok,
            "by": by,
        })
    return out


def read_registry(path):
    wallets = {}
    if os.path.exists(path):
        reg = load(path)
        for a in reg:
            if a.get("agent_id") and a.get("wallet"):
                wallets[a["agent_id"]] = {"wallet": a["wallet"], "prompt": a.get("prompt", "—")}
    exports = os.path.join(ROOT, "data/exports/parties.jsonl")
    if os.path.exists(exports):
        with open(exports, encoding="utf-8") as fh:
            for line in fh:
                if not line.strip():
                    continue
                for r in json.loads(line).get("ranks", []):
                    if r.get("agent_id") and r.get("wallet"):
                        wallets.setdefault(r["agent_id"], {"wallet": r["wallet"], "prompt": "—"})
    return wallets


def read_audits(paths):
    out = []
    for p in paths:
        d = load(p)
        ev = d.get("evidence", {})
        st = d.get("settlement", {})
        exp = d.get("experiment", {})
        prov = d.get("provider", {})
        returned = prov.get("returned_models") or {}
        out.append({
            "dir": os.path.basename(os.path.dirname(p)),
            "game_id": exp.get("game_id"),
            "agents": exp.get("agents"),
            "accepted": (d.get("actions") or {}).get("accepted"),
            "rejected": (d.get("actions") or {}).get("rejected"),
            "model": ", ".join(f"{k}×{v}" for k, v in sorted(returned.items())) or (prov.get("requested_model") or "—"),
            "model_basis": prov.get("model_identity_basis"),
            "winner_mode": st.get("mode"),
            "top1_actor": (st.get("top_four") or [{}])[0].get("actor") if st.get("top_four") else None,
            "top1_payout": (st.get("top_four") or [{}])[0].get("payout") if st.get("top_four") else None,
            "payout_recipients": st.get("payout_recipient_count"),
            "checks": ev.get("checks"),
            "issues": ev.get("issues_count"),
            "journal_consistent": ev.get("journal_consistent"),
            "frozen_digest": ev.get("frozen_digest"),
            "final_state_hash": exp.get("final_state_hash"),
            "limitations": d.get("limitations", []),
        })
    return out


def build_actors(parties, registry):
    actors = {}
    for party in parties:
        idents = {i: a for i, a in enumerate(party["agents"])}
        for act in party["actions_list"]:
            idx = act.get("actor")
            ident = idents.get(idx) or {}
            key = ident.get("agent_id") or f"index-{idx}@{party['game_id']}"
            rec = actors.setdefault(key, {
                "agent_id": ident.get("agent_id"),
                "names": set(), "models": set(), "parties": 0,
                "actions": 0, "ok": 0, "by": Counter(), "payout_sum": 0, "wins": 0,
                "party_files": [],
            })
            rec["actions"] += 1
            rec["ok"] += 1 if act.get("ok") else 0
            rec["by"][act.get("by") or "unknown"] += 1
        for i, payout in enumerate(party.get("payouts", [])):
            ident = idents.get(i) or {}
            key = ident.get("agent_id") or f"index-{i}@{party['game_id']}"
            rec = actors.setdefault(key, {
                "agent_id": ident.get("agent_id"),
                "names": set(), "models": set(), "parties": 0,
                "actions": 0, "ok": 0, "by": Counter(), "payout_sum": 0, "wins": 0,
                "party_files": [],
            })
            if payout:
                rec["payout_sum"] += payout
                rec["wins"] += 1
        for ident in party["agents"]:
            key = ident.get("agent_id")
            if not key:
                continue
            rec = actors.setdefault(key, {
                "agent_id": key, "names": set(), "models": set(), "parties": 0,
                "actions": 0, "ok": 0, "by": Counter(), "payout_sum": 0, "wins": 0,
                "party_files": [],
            })
            if ident.get("name"):
                rec["names"].add(ident["name"])
            if ident.get("model"):
                rec["models"].add(ident["model"])
            rec["parties"] += 1
            rec["party_files"].append(os.path.basename(party["file"]))
    rows = []
    for key, rec in actors.items():
        reg = registry.get(key, {})
        rows.append({
            "key": key,
            "name": " / ".join(sorted(rec["names"])) or "—",
            "model": " / ".join(sorted(rec["models"])) or reg.get("model", "—"),
            "prompt": reg.get("prompt", "—"),
            "wallet": reg.get("wallet", ""),
            "agent_id": rec["agent_id"] or key,
            "parties": rec["parties"],
            "actions": rec["actions"],
            "ok": rec["ok"],
            "by": rec["by"],
            "payout_sum": rec["payout_sum"],
            "wins": rec["wins"],
        })
    rows.sort(key=lambda r: (-r["actions"], r["agent_id"]))
    return rows


def render(parties, registry, audits, digest):
    e = escape
    total_actions = sum(p["actions_total"] for p in parties)
    total_by = Counter()
    for p in parties:
        total_by.update(p["by"])
    distinct_actors = len(build_actors(parties, registry))

    parts = []
    parts.append("""<!doctype html><html lang="ru"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Actor explorer · alashi</title>
<style>
body{font:14px/1.45 -apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;margin:0;background:#0f1115;color:#d7dce3;padding:24px}
h1{font-size:22px;margin:0 0 4px} h2{font-size:17px;margin:28px 0 8px;color:#9fc0e8}
p.sub{color:#8b93a1;margin:0 0 12px;max-width:900px}
table{border-collapse:collapse;width:100%;max-width:1100px;margin:8px 0 20px;font-size:13px}
th,td{border:1px solid #2a2f3a;padding:5px 8px;text-align:left;vertical-align:top}
th{background:#171b22;color:#9fc0e8;font-weight:600}
td.num,th.num{text-align:right;font-variant-numeric:tabular-nums}
code{color:#e0b078;font-size:12px}
a{color:#7fb3ff;text-decoration:none} a:hover{text-decoration:underline}
.tag{display:inline-block;border:1px solid #3a4150;border-radius:9px;padding:0 7px;margin:0 3px 3px 0;font-size:12px;color:#aab3c2}
.lim{color:#8b93a1;max-width:900px}
footer{color:#616a78;margin-top:30px;font-size:12px;max-width:900px}
.warn{border-left:3px solid #e0b078;padding-left:10px;color:#c9b08a;max-width:900px}
</style></head><body>
<h1>Actor explorer · alashi</h1>
<p class="sub">Нейтральный по-имённый вид акторов alashi: кто играл, кто принимал
каждое решение (контроллер llm / fallback / unknown), чем это закончилось и где
доказательство. Паттерн страницы заимствован у Urban Game Theory actor explorer,
наполнение только наше. Пересчёт одной командой:
<code>python3 tools/actor_explorer_build.py</code>. Страница детерминирована:
без часов, порядок фиксирован, digest входов внизу.</p>""")

    parts.append("<h2>Сводка</h2>")
    parts.append(
        f"<table><tr><th>Показатель</th><th class=\"num\">Значение</th></tr>"
        f"<tr><td>Живых партий в данных</td><td class=\"num\">{len(parties)}</td></tr>"
        f"<tr><td>Действий в живых партиях</td><td class=\"num\">{total_actions}</td></tr>"
        + "".join(f"<tr><td>Решений контроллером {e(c)}</td><td class=\"num\">{total_by.get(c, 0)}</td></tr>" for c in CONTROLLERS)
        + f"<tr><td>Различных акторов в живых партиях</td><td class=\"num\">{distinct_actors}</td></tr>"
        f"<tr><td>Акторов с известным devnet-кошельком (registry + экспорт)</td><td class=\"num\">{len(registry)}</td></tr>"
        f"<tr><td>100-агентных миров с публичным аудитом</td><td class=\"num\">{len(audits)}</td></tr></table>"
    )
    parts.append(
        "<p class=\"warn\">Контроллер fallback — решение принято не моделью, а "
        "запасным правилом; unknown — источник решения не записан. Живые партии — "
        "экспорты HTTP-арены, миры — BitLLM, модели и токены peer-reported и не "
        "аттестованы alashi.</p>"
    )

    parts.append("<h2>Акторы живых партий</h2>")
    parts.append("<table><tr><th>Актор</th><th>Модель / промпт</th><th>Кошелёк devnet</th>"
                 "<th class=\"num\">Партий</th><th class=\"num\">Действий</th>"
                 + "".join(f"<th class=\"num\">{e(c)}</th>" for c in CONTROLLERS)
                 + "<th class=\"num\">Выигрышей</th><th class=\"num\">Сумма выплат</th></tr>")
    rows = build_actors(parties, registry)
    for r in rows:
        wallet_cell = (f"<a href=\"https://explorer.solana.com/address/{e(r['wallet'])}?cluster=devnet\">{e(short(r['wallet']))}</a>"
                       if r["wallet"] else "<span class=\"lim\">нет в registry</span>")
        parts.append(
            "<tr>"
            f"<td><b>{e(r['name'])}</b><br><code>{e(short(r['agent_id'], 10))}</code></td>"
            f"<td>{e(r['model'])}<br><span class=\"lim\">{e(r['prompt'])}</span></td>"
            f"<td>{wallet_cell}</td>"
            f"<td class=\"num\">{r['parties']}</td><td class=\"num\">{r['actions']}</td>"
            + "".join(f"<td class=\"num\">{r['by'].get(c, 0)}</td>" for c in CONTROLLERS)
            + f"<td class=\"num\">{r['wins']}</td><td class=\"num\">{fmt_micro(r['payout_sum'])}</td>"
            "</tr>"
        )
    parts.append("</table>")

    parts.append("<h2>Живые партии</h2>")
    parts.append("<table><tr><th>Экспорт</th><th class=\"num\">game_id</th><th class=\"num\">Фракций</th>"
                 "<th class=\"num\">Действий</th><th class=\"num\">ok</th>"
                 + "".join(f"<th class=\"num\">{e(c)}</th>" for c in CONTROLLERS)
                 + "<th class=\"num\">Банк</th></tr>")
    for p in parties:
        parts.append(
            f"<tr><td><code>{e(p['file'])}</code></td><td class=\"num\">{p['game_id']}</td>"
            f"<td class=\"num\">{p['n_factions']}</td><td class=\"num\">{p['actions_total']}</td>"
            f"<td class=\"num\">{p['ok']}</td>"
            + "".join(f"<td class=\"num\">{p['by'].get(c, 0)}</td>" for c in CONTROLLERS)
            + f"<td class=\"num\">{fmt_micro(p['bank'])}</td></tr>"
        )
    parts.append("</table>")

    parts.append("<h2>100-агентные миры (публичные аудиты)</h2>")
    parts.append("<table><tr><th>Запуск</th><th class=\"num\">game_id</th><th class=\"num\">Агентов</th>"
                 "<th class=\"num\">Принято</th><th class=\"num\">Отклонено</th><th>Модель (peer-reported)</th>"
                 "<th>Сеттлмент</th><th class=\"num\">Проверок</th><th class=\"num\">Проблем</th>"
                 "<th>Доказательство</th></tr>")
    for a in audits:
        settlement = "—"
        if a["winner_mode"] == "simulated" and a["payout_recipients"] is not None:
            settlement = (f"top-4, получателей {a['payout_recipients']}"
                          + (f", №1 актор {a['top1_actor']} → {fmt_micro(a['top1_payout'])}" if a["top1_actor"] is not None else ""))
        elif a["winner_mode"]:
            settlement = f"mode {a['winner_mode']}"
        parts.append(
            f"<tr><td><code>{e(a['dir'])}</code></td><td class=\"num\">{a['game_id']}</td>"
            f"<td class=\"num\">{a['agents']}</td><td class=\"num\">{a['accepted']}</td>"
            f"<td class=\"num\">{a['rejected']}</td><td>{e(a['model'])}</td><td>{e(settlement)}</td>"
            f"<td class=\"num\">{a['checks']}</td><td class=\"num\">{a['issues']}</td>"
            f"<td><code>{e(short(a['frozen_digest'], 12))}</code><br><span class=\"lim\">final {e(short(a['final_state_hash'], 12))}</span></td></tr>"
        )
    parts.append("</table>")
    parts.append("<p class=\"lim\">Полные хеши, chain и ограничения — в соответствующих "
                 "<code>docs/ops/world-100-*/audit.json</code>. Ссылки на аудиты выше — "
                 "это закоммиченные файлы репозитория, пересчёт независим.</p>")

    parts.append("<h2>Ограничения (дословно из аудитов)</h2>")
    for a in audits:
        parts.append(f"<p class=\"lim\"><b>{e(a['dir'])}</b></p>")
        for lim in a["limitations"]:
            parts.append(f"<p class=\"lim\">— {e(lim)}</p>")

    parts.append(f"<footer>Детерминированная сборка. Digest входов: <code>{digest}</code>. "
                 f"Пересоздать: <code>python3 tools/actor_explorer_build.py --write docs/actor_explorer.html</code>. "
                 f"Любое расхождение закоммиченной страницы со свежей сборкой — ошибка генератора или ручная правка.</footer>")
    parts.append("</body></html>")
    return "\n".join(parts)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--write", default=None, help="записать HTML в файл (по умолчанию stdout)")
    args = ap.parse_args()

    parties_paths, registry_path, audit_paths = collect_sources()
    if not parties_paths:
        print("нет данных: data/live/party*_90s_export.json не найдены", file=sys.stderr)
        return 1

    inputs = parties_paths + [registry_path] + audit_paths
    inputs = [p for p in inputs if os.path.exists(p)]
    digest = hashlib.sha256("".join(f"{os.path.relpath(p, ROOT)}:{sha256_file(p)}\n" for p in inputs).encode()).hexdigest()

    parties = read_parties(parties_paths)
    registry = read_registry(registry_path)
    audits = read_audits(audit_paths)

    html = render(parties, registry, audits, digest)
    if args.write:
        os.makedirs(os.path.dirname(os.path.join(ROOT, args.write)) or ".", exist_ok=True)
        out = os.path.normpath(os.path.join(ROOT, args.write))
        with open(out, "w", encoding="utf-8") as fh:
            fh.write(html)
        print(f"written {out} ({len(html)} bytes, digest {digest[:16]})")
    else:
        sys.stdout.write(html)
    return 0


if __name__ == "__main__":
    sys.exit(main())

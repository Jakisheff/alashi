#!/usr/bin/env python3
import json, re, time, urllib.request

APP = "45BWZJ1SGC"
HOST = "https://45bwzj1sgc-1.algolianet.com"
IDX = "YCCompany_production"
OUT = "/Users/amir/Desktop/alashi/data/yc_companies.jsonl"
DELAY = 1.6

html = open("/tmp/opencode/yc_cat.html").read()
import re as _re
KEY = _re.search(r'"key":"([^"]+)"', html).group(1)

def query(term, page=0, hits=1000):
    body = json.dumps({"query": term, "hitsPerPage": hits, "page": page,
                       "attributesToHighlight": []}).encode()
    req = urllib.request.Request(
        f"{HOST}/1/indexes/{IDX}/query", data=body,
        headers={"X-Algolia-Application-Id": APP, "X-Algolia-API-Key": KEY,
                 "Content-Type": "application/json",
                 "User-Agent": "alashi-research/0.1 (github.com/Jakisheff)"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)

AGENT = re.compile(r"agent|LLM|autonom|intellig", re.I)
CRYPTO = re.compile(r"crypto|web3|blockchain|stablecoin|on-?chain|bitcoin|ethereum|solana|token|wallet|defi", re.I)
GAME = re.compile(r"\bgame|gaming|play", re.I)

def batch_year(h):
    m = re.search(r"(20\d\d)", h.get("batch") or "")
    return int(m.group(1)) if m else 0

seen = {}
for term in ["agent", "agentic", "crypto", "web3", "blockchain", "stablecoin",
             "prediction market", "onchain game", "crypto game", "agent payment",
             "agent economy"]:
    page = 0
    while True:
        try:
            d = query(term, page)
        except Exception as e:
            print(f"[{term} p{page}] error: {e}", flush=True)
            break
        for h in d.get("hits", []):
            seen[h["objectID"]] = h
        page += 1
        if page >= d.get("nbPages", 1) or page > 4:
            break
        time.sleep(DELAY)
    print(f"[{term}] всего уникальных: {len(seen)}", flush=True)
    time.sleep(DELAY)

rows = []
for h in seen.values():
    text = " ".join([h.get("name") or "", h.get("one_liner") or "",
                     h.get("long_description") or "", " ".join(h.get("tags") or [])])
    is_agent = bool(AGENT.search(text))
    is_crypto = bool(CRYPTO.search(text))
    is_game = bool(GAME.search(text))
    y = batch_year(h)
    keep = False
    overlap = []
    if is_agent and is_crypto:
        keep = True; overlap.append("agent×crypto")
    if is_agent and y >= 2025:
        keep = True; overlap.append("agent-2025+")
    if is_crypto and is_game and y >= 2024:
        keep = True; overlap.append("game×crypto")
    if not keep:
        continue
    rows.append({
        "название": h.get("name"),
        "тэглайн": h.get("one_liner"),
        "описание": h.get("long_description"),
        "батч": h.get("batch"),
        "индустрия_теги": (h.get("industries") or []) + (h.get("tags") or []),
        "статус": h.get("status"),
        "ссылки": {"сайт": h.get("website"),
                   "yc_profile": f"https://www.ycombinator.com/companies/{h.get('slug')}"},
        "команда": [],
        "team_size": h.get("team_size"),
        "пересечение": overlap,
    })

rows.sort(key=lambda r: (r.get("батч") or "", r["название"]))
with open(OUT, "w") as f:
    for r in rows:
        f.write(json.dumps(r, ensure_ascii=False) + "\n")
print(f"готово: {len(rows)} записей -> {OUT}", flush=True)
from collections import Counter
print(Counter(r["батч"] for r in rows).most_common(12))
print(Counter(t for r in rows for t in r["пересечение"]).most_common())

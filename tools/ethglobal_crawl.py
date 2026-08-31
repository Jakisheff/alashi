#!/usr/bin/env python3
import json, re, time, urllib.request, sys

UA = "alashi-research/0.1 (hackathon recon; github.com/Jakisheff)"
BASE = "https://ethglobal.com"
OUT = "/Users/amir/Desktop/alashi/data/ethglobal_projects.jsonl"
DELAY = 1.5
MAX_PAGES = 60

KEYWORDS = re.compile(
    r"agent|LLM|AI\b|game|arena|benchmark|simulat|autonom|multi-?agent|bot\b|"
    r"econom|market|compete|strateg|policy|governan|diplomacy|negotiat",
    re.I,
)

def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.read().decode("utf-8", "ignore")

def rsc_blob(html):
    chunks = re.findall(r'self\.__next_f\.push\(\[1,"((?:[^"\\]|\\.)*)"\]\)', html)
    return "".join(c.encode().decode("unicode_escape") for c in chunks)

def parse_listing(html):
    cards = []
    pat = re.compile(
        r'href="/showcase/([a-z0-9-]+)">.*?<h2 class="text-2xl">([^<]+)</h2>'
        r'<p class="text-sm[^"]*">([^<]*)</p>.*?>(ETHGlobal [A-Za-z\' ]+\d{4})</div>',
        re.S,
    )
    for m in pat.finditer(html):
        cards.append({"slug": m.group(1), "name": m.group(2).strip(),
                      "oneLiner": m.group(3).strip(), "event": m.group(4).strip()})
    return cards

def resolve_refs(blob):
    rows = {}
    for m in re.finditer(r'(?:^|\n)([0-9a-f]+):T[0-9a-f]+,', blob):
        start = m.end()
        end = blob.find("\n", start)
        rows[m.group(1)] = blob[start:end if end > 0 else len(blob)]
    return rows

def parse_detail(html, slug):
    blob = rsc_blob(html)
    rows = resolve_refs(blob)
    def ref(key):
        m = re.search(r'"%s":"\$([0-9a-f]+)"' % key, blob)
        return rows.get(m.group(1), "") if m else ""
    def field(key):
        m = re.search(r'"%s":"([^"]*)"' % key, blob)
        return m.group(1) if m else ""
    obj = {
        "slug": slug,
        "название": "",
        "тэглайн": "",
        "описание": ref("description"),
        "как_сделано": ref("howItsMade"),
        "хакатон": field("name") or "",
        "дата": (re.search(r'"startTime":"([^"]+)"', blob) or [None, ""])[1] if re.search(r'"startTime":"([^"]+)"', blob) else "",
        "призовые_треки": [],
        "ссылки": {
            "repo": field("primaryRepository") if False else (re.search(r'"primaryRepository":\{"url":"([^"]+)"', blob) or [None, ""])[1],
            "demo": field("url"),
            "showcase": f"{BASE}/showcase/{slug}",
        },
        "технологии": [],
        "команда": [],
    }
    for m in re.finditer(r'"name":"([^"]{2,60})","description":"\$([0-9a-f]+)"', blob):
        obj["название"] = obj["название"] or ""
    t = re.search(r'\["\$","title","0",\{"children":"([^|]+)\| ETHGlobal', blob)
    obj["название"] = t.group(1).strip() if t else slug
    prizes = re.findall(r'"prizes":\[(.*?)\]', blob)
    if prizes and prizes[0]:
        for p in re.finditer(r'"name":"([^"]+)"', prizes[0]):
            obj["призовые_треки"].append(p.group(1))
    for k, key in [("авто_оригинальность", "autoOriginality"),
                   ("авто_практичность", "autoPracticality"),
                   ("авто_техничность", "autoTechnicality")]:
        m = re.search(r'"%s":(\d+)' % key, blob)
        obj[k] = int(m.group(1)) if m else None
    m = re.search(r'"demoVideoReady":(true|false)', blob)
    obj["демо_видео"] = m.group(1) == "true" if m else None
    m = re.search(r'"autoSummary":"([^"]*)"', blob)
    obj["авто_резюме"] = m.group(1) if m else ""
    ev = re.search(r'"event":\{"slug":"([^"]+)","name":"([^"]+)","startTime":"([^"]+)"', blob)
    if ev:
        obj["хакатон"] = ev.group(2)
        obj["дата"] = ev.group(3)[:10]
    return obj

def main():
    seen = set()
    all_cards = []
    for page in range(1, MAX_PAGES + 1):
        try:
            html = fetch(f"{BASE}/showcase?page={page}")
        except Exception as e:
            print(f"[page {page}] fetch error: {e}", flush=True)
            break
        cards = parse_listing(html)
        new = [c for c in cards if c["slug"] not in seen]
        if not new:
            print(f"[page {page}] нет новых карточек, конец каталога", flush=True)
            break
        seen.update(c["slug"] for c in new)
        all_cards.extend(new)
        print(f"[page {page}] +{len(new)} (всего {len(seen)})", flush=True)
        time.sleep(DELAY)
    candidates = [c for c in all_cards if KEYWORDS.search(c["name"] + " " + c["oneLiner"])]
    print(f"кандидатов по фильтру: {len(candidates)} из {len(all_cards)}", flush=True)
    n = 0
    with open(OUT, "w") as f:
        for c in candidates:
            try:
                html = fetch(f"{BASE}/showcase/{c['slug']}")
                obj = parse_detail(html, c["slug"])
                obj["тэглайн"] = c["oneLiner"]
                if not obj["описание"]:
                    obj["описание"] = c["oneLiner"]
                f.write(json.dumps(obj, ensure_ascii=False) + "\n")
                n += 1
                if n % 10 == 0:
                    print(f"[detail] {n}/{len(candidates)}", flush=True)
            except Exception as e:
                print(f"[detail {c['slug']}] error: {e}", flush=True)
            time.sleep(DELAY)
    print(f"готово: {n} записей -> {OUT}", flush=True)

if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Скармливание дебриф-промпта агенту вместе с экспортом его партии.

Что делает:
1. Забирает GET /export арены и выбирает партию по --game.
2. Сжимает протокол в читаемый текст (законы таблицей, ходы по фазам,
   ходы целевого агента помечены звёздочкой).
3. Берёт промпт из docs/AGENT_DEBRIEF_PROMPT.md (блок между --- и ---).
4. Отправляет [промпт + протокол] в GLM (ключ как у ботов: env
   ALASHI_LLM_KEY или ~/.config/alashi/llm.json) и сохраняет ответ
   в docs/debriefs/game<N>_<agent>.md
5. Без ключа (или с --no-llm) просто пишет payload в файл — его можно
   вставить в любой чат с моделью руками.

Примеры:
  python3 tools/debrief.py --game 2 --agent Agent1
  python3 tools/debrief.py --url http://127.0.0.1:8090 --game 2 --agent Agent2 --no-llm
"""
import argparse, json, os, sys, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def get_export(base):
    with urllib.request.urlopen(f"{base}/export", timeout=10) as r:
        return json.load(r)

def compress(game, agent_name):
    names = [a["name"] for a in game["agents"]]
    out = []
    out.append(f"ПАРТИЯ №{game['game_id']}, фракций: {game['n_factions']}")
    out.append("Агенты (индекс = фракция): " + ", ".join(f"{i}:{n}" for i, n in enumerate(names)))
    out.append(f"Ты играл за: {agent_name} (индекс {names.index(agent_name)})" if agent_name in names else f"Агент {agent_name} НЕ найден; вот кто был: {names}")
    out.append("")
    out.append("ИТОГ (ранг по портфелю, выплаты):")
    ranks = game["ranks"]; pays = game["payouts"]
    for place, fi in enumerate(ranks):
        out.append(f"  {place+1}. {names[fi]}: выплата {pays[fi]//10**6}M")
    out.append(f"  банк {game['bank']//10**6}M, рейк {game['rake']//10**6}M")
    out.append("")
    out.append("ХРОНИКА ПО ФАЗАМ (звёздочка = твой ход; cash в млн):")
    for ph in game["phases"]:
        head = f"r{ph['round']} {ph['phase']}"
        if ph["phase"] == "law":
            head += f" | карта {ph.get('card_name')} | да {ph.get('yes')} нет {ph.get('no')} | {'ПРОШЁЛ' if ph.get('passed') else 'не прошёл'}"
        out.append(head)
        for a in ph.get("actions", []) if isinstance(ph.get("actions"), list) else []:
            # phases в экспорте новых партий не содержат actions — они в корне
            pass
    # действия лежат в корне записи
    for a in game.get("actions", []):
        pass
    out.append("")
    out.append("ВСЕ ХОДЫ ПАРТИИ (r/фаза, кто, действие, исход, кэш после):")
    for a in game.get("actions", []):
        mark = "*" if (agent_name in names and a["actor"] == names.index(agent_name)) else " "
        cash = a.get("cash_after")
        cash_s = f"{cash//10**6}M" if isinstance(cash, (int, float)) else "-"
        err = "" if a.get("ok") else f" ОТКАЗ({a.get('err')})"
        params = json.dumps(a.get("params", {}), ensure_ascii=False)
        out.append(f" {mark} r{a.get('round')} {a.get('phase')}: {names[a['actor']]} -> {a.get('action')} {params}{err} | cash {cash_s}")
    return "\n".join(out)

def load_prompt():
    s = open(os.path.join(ROOT, "docs/AGENT_DEBRIEF_PROMPT.md"), encoding="utf-8").read()
    # блок между первыми двумя --- (сам промпт)
    parts = s.split("\n---\n")
    if len(parts) >= 2:
        return parts[1].strip()
    return s

def llm_key():
    k = os.environ.get("ALASHI_LLM_KEY")
    if k:
        return k
    p = os.path.join(os.path.expanduser("~"), ".config/alashi/llm.json")
    if os.path.exists(p):
        return json.load(open(p)).get("key")
    return None

def ask_glm(key, system, user, model="glm-4.5-flash"):
    body = json.dumps({
        "model": model,
        "thinking": {"type": "disabled"},
        "max_tokens": 1500,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
    }).encode()
    req = urllib.request.Request(
        "https://api.z.ai/api/paas/v4/chat/completions",
        data=body,
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=90) as r:
        v = json.load(r)
    return v["choices"][0]["message"]["content"]

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", default="http://127.0.0.1:8090")
    ap.add_argument("--game", type=int, required=True)
    ap.add_argument("--agent", required=True)
    ap.add_argument("--model", default="glm-4.5-flash")
    ap.add_argument("--no-llm", action="store_true")
    a = ap.parse_args()

    ex = get_export(a.url)
    games = [g for g in ex if g["game_id"] == a.game]
    if not games:
        print(f"[ERROR] партия {a.game} не найдена; в экспорте: {[g['game_id'] for g in ex]}")
        sys.exit(1)
    game = games[0]
    if not game.get("actions"):
        print("[ERROR] в записи нет ходов: партия до эпохи полного экспорта")
        sys.exit(1)

    prompt = load_prompt()
    proto = compress(game, a.agent)
    payload = f"{prompt}\n\n=== ПРОТОКОЛ ПАРТИИ ===\n{proto}\n\nТеперь дай ответ по пунктам 1-5."

    os.makedirs(os.path.join(ROOT, "docs/debriefs"), exist_ok=True)
    base = os.path.join(ROOT, f"docs/debriefs/game{a.game}_{a.agent}")

    if a.no_llm:
        open(base + "_payload.txt", "w", encoding="utf-8").write(payload)
        print(f"payload без LLM: {base}_payload.txt ({len(payload)} символов)")
        return

    key = llm_key()
    if not key:
        open(base + "_payload.txt", "w", encoding="utf-8").write(payload)
        print("[ERROR] ключа нет; payload сохранён для ручной отправки")
        sys.exit(1)

    print("спрашиваю GLM...")
    try:
        answer = ask_glm(key, prompt, f"=== ПРОТОКОЛ ПАРТИИ ===\n{proto}\n\nТеперь дай ответ по пунктам 1-5.", a.model)
    except Exception as e:
        print(f"[ERROR] LLM: {e}; payload сохранён")
        open(base + "_payload.txt", "w", encoding="utf-8").write(payload)
        sys.exit(1)

    with open(base + ".md", "w", encoding="utf-8") as f:
        f.write(f"# Дебриф {a.agent}, партия №{a.game} (Advanced JTBD)\n\n{answer}\n")
    print(f"готово: {base}.md")

if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Рефлексивное интервью LLM-агента о его собственных ходах (кастдев v3).

Берёт tx-log партии (GET /export или файл), выбирает значимые ходы
агента, строит контекст ДО хода (без последствий), отправляет в GLM
с промптом из CASTDEV_PROMPT_REFLECT, сохраняет транскрипт с колонкой
«лог говорит» для сравнения самоотчёта с объективным положением.

Пример:
  python3 tools/reflect_interview.py --file data/live/party7_90s_export.json \
      --agent Botagul --rounds 2,4,6
"""
import argparse, json, os, sys, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

def load_party(args):
    if args.file:
        return json.load(open(args.file))
    with urllib.request.urlopen(f"{args.url}/export", timeout=10) as r:
        ex = json.load(r)
    games = [g for g in ex if g["game_id"] == args.game]
    if not games:
        sys.exit(f"[ERROR] партия {args.game} не найдена")
    return games[0]

PHASE_ORDER = {"market": 0, "action": 1, "law": 2}

def _key(a):
    return (a["round"], PHASE_ORDER.get(a["phase"], 9))

def context_before(game, agent_idx, round_, phase):
    """Состояние, видимое агенту ДО его хода в фазе: ходы других в этой
    фазе до него + сводка его положения. Без последствий его хода."""
    names = [a["name"] for a in game["agents"]]
    prior = []
    target_key = (round_, PHASE_ORDER[phase])
    prev_cash = prev_goods = None
    seen_own_move = False
    for a in game["actions"]:
        k = _key(a)
        if k > target_key:
            break
        if k == target_key and a["actor"] == agent_idx and a["ok"] and not seen_own_move:
            seen_own_move = True  # всё после — последствия, не читаем
            continue
        if seen_own_move:
            continue
        if k == target_key and a["actor"] != agent_idx:
            prior.append(f"{names[a['actor']]}: {a['action']} {json.dumps(a.get('params',{}), ensure_ascii=False)}")
        elif k < target_key and a["actor"] == agent_idx and a["ok"]:
            if a.get("cash_after") is not None:
                prev_cash = a["cash_after"]
            if a.get("goods_after") is not None:
                prev_goods = a["goods_after"]
    laws = [f"r{p['round']}: {p['card_name']} {'прошёл' if p['passed'] else 'не прошёл'}"
            for p in game["phases"] if p["phase"] == "law" and p["round"] < round_]
    return {
        "round": round_, "phase": phase,
        "твои_деньги_до": prev_cash, "твой_товар_до": prev_goods,
        "законы_раньше": laws,
        "чужие_ходы_в_этой_фазе_до_тебя": prior,
    }

def the_move(game, agent_idx, round_, phase):
    for a in game["actions"]:
        if (a["round"], a["phase"], a["actor"]) == (round_, phase, agent_idx) and a["ok"]:
            return f"{a['action']} {json.dumps(a.get('params',{}), ensure_ascii=False)}"
    return None

def what_log_says(game, agent_idx, round_, phase, move):
    """Объективная расшифровка для таблицы сравнения."""
    laws = [p for p in game["phases"] if p["phase"] == "law" and p["round"] == round_]
    law = laws[0]["card_name"] if laws else "?"
    return f"ход был: {move}; карта закона r{round_}: {law}"

def llm_key():
    k = os.environ.get("ALASHI_LLM_KEY")
    if k:
        return k
    p = os.path.join(os.path.expanduser("~"), ".config/alashi/llm.json")
    if os.path.exists(p):
        return json.load(open(p)).get("key")
    return None

def ask_glm(key, system, user, model):
    body = json.dumps({
        "model": model, "thinking": {"type": "disabled"}, "max_tokens": 900,
        "messages": [{"role": "system", "content": system},
                     {"role": "user", "content": user}],
    }).encode()
    req = urllib.request.Request(
        "https://api.z.ai/api/paas/v4/chat/completions", data=body,
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
        method="POST")
    with urllib.request.urlopen(req, timeout=90) as r:
        return json.load(r)["choices"][0]["message"]["content"]

def load_reflect_prompt():
    s = open(os.path.join(ROOT, "docs/CASTDEV_PROMPT_REFLECT.md"), encoding="utf-8").read()
    parts = s.split("---")
    # блок промпта между первыми двумя ---
    block = max(parts[1:3], key=len) if len(parts) >= 3 else s
    # берём текст от «Ты — ИИ-агент» до «Ответ: пункты»
    i = block.find("Ты — ИИ-агент")
    j = block.find("Ответ: пункты")
    return block[i:j + len("Ответ: пункты 1-4, до 250 слов на ход.")]

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--file")
    ap.add_argument("--url", default="http://127.0.0.1:8090")
    ap.add_argument("--game", type=int)
    ap.add_argument("--agent", required=True)
    ap.add_argument("--rounds", default="2,4,6")
    ap.add_argument("--model", default="glm-4.5-flash")
    ap.add_argument("--no-llm", action="store_true")
    a = ap.parse_args()

    game = load_party(a)
    names = [x["name"] for x in game["agents"]]
    if a.agent not in names:
        sys.exit(f"[ERROR] {a.agent} нет в партии; вот кто был: {names}")
    idx = names.index(a.agent)

    prompt = load_reflect_prompt()
    key = None if a.no_llm else llm_key()
    out = [f"# Рефлексивное интервью: {a.agent}, партия №{game['game_id']}\n"]
    out.append(f"Ограничение метода: ответы — пост-хок реконструкция, не "
               f"память. Сравнение с логом ниже — главный результат.\n")

    for r in [int(x) for x in a.rounds.split(",")]:
        for phase in ("market", "action", "law"):
            move = the_move(game, idx, r, phase)
            if not move:
                continue
            ctx = context_before(game, idx, r, phase)
            user = (f"ПАРТИЯ alashi, раунд {r}, фаза {phase}.\n"
                    f"Твой ход был: {move}\n"
                    f"Контекст ДО хода (только это ты видел):\n{json.dumps(ctx, ensure_ascii=False, indent=1)}\n\n"
                    f"{prompt}")
            out.append(f"\n## Ход: r{r} {phase} — {move}\n")
            out.append(f"Контекст ДО: {json.dumps(ctx, ensure_ascii=False)}\n")
            if key:
                try:
                    ans = ask_glm(key, "Ты отвечаешь на вопросы о своём ходе в игре. Честно про реконструкцию.", user, a.model)
                    out.append(f"\n### Самоотчёт агента:\n{ans}\n")
                except Exception as e:
                    out.append(f"\n### Самоотчёт: [ERROR LLM {e}], payload сохранён\n")
                    key = None
            else:
                out.append("\n### Самоотчёт: (без LLM — payload ниже)\n```" + user + "\n```\n")
            out.append(f"\n### Лог говорит: {what_log_says(game, idx, r, phase, move)}\n")

    os.makedirs(os.path.join(ROOT, "docs/debriefs"), exist_ok=True)
    path = os.path.join(ROOT, f"docs/debriefs/REFLECT_{a.agent}_game{game['game_id']}.md")
    open(path, "w", encoding="utf-8").write("\n".join(out))
    print(f"готово: {path}")

if __name__ == "__main__":
    main()

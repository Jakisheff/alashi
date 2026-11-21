#!/usr/bin/env python3
"""CI-гейт: actor_explorer_build.py воспроизводим и сходится с коммитом.

Проверки (exit 0 = всё держится):
  1. Генератор собирает страницу из закоммиченных данных без ошибок.
  2. Числа на странице сходятся с независимым пересчётом теста:
     партии, действия, контроллеры, акторы.
  3. Детерминизм: две сборки побайтно одинаковы (нет часов/рандома).
  4. Фриз-гейт: закоммиченная docs/actor_explorer.html совпадает
     побайтно со свежей сборкой.

Run: python3 tools/test_actor_explorer_build.py
"""
import glob
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from collections import Counter

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GEN = os.path.join(REPO, "tools/actor_explorer_build.py")
COMMITTED = os.path.join(REPO, "docs/actor_explorer.html")


def build(path):
    proc = subprocess.run(
        [sys.executable, GEN, "--write", path],
        capture_output=True, text=True, cwd=REPO,
    )
    if proc.returncode != 0:
        print(proc.stdout)
        print(proc.stderr, file=sys.stderr)
        return None
    with open(path, encoding="utf-8") as fh:
        return fh.read()


def main() -> int:
    # 1-3: детерминизм + независимый пересчёт
    with tempfile.TemporaryDirectory() as td:
        a = os.path.join(td, "a.html")
        b = os.path.join(td, "b.html")
        html = build(a)
        if html is None:
            print("FAIL: генератор упал", file=sys.stderr)
            return 1
        build(b)
        with open(a, "rb") as fa, open(b, "rb") as fb:
            ha, hb = hashlib.sha256(fa.read()).hexdigest(), hashlib.sha256(fb.read()).hexdigest()
        if ha != hb:
            print("FAIL: недетерминированная сборка", file=sys.stderr)
            return 1

        parties = sorted(glob.glob(os.path.join(REPO, "data/live/party*_90s_export.json")))
        n_actions = 0
        by = Counter()
        agent_ids = set()
        for p in parties:
            d = json.load(open(p, encoding="utf-8"))
            n_actions += len(d.get("actions", []))
            for act in d.get("actions", []):
                by[act.get("by") or "unknown"] += 1
            for ident in d.get("agents", []):
                if ident.get("agent_id"):
                    agent_ids.add(ident["agent_id"])

        checks = [
            (f"Живых партий в данных", rf">{len(parties)}</td>", True),
            (f"Всего действий", rf">{n_actions}</td>", True),
        ]
        for c in ("llm", "fallback", "unknown"):
            checks.append((f"контроллер {c}", rf">{by.get(c, 0)}</td>", True))
        # actor rows == числу различных agent_id в партиях
        rows = len(re.findall(r"<tr><td><b>", html))
        if rows != len(agent_ids):
            print(f"FAIL: строк акторов {rows} != различных agent_id {len(agent_ids)}", file=sys.stderr)
            return 1
        for name, needle, expected in checks:
            if (needle in html) != expected:
                print(f"FAIL: {name}: {needle!r} не найден", file=sys.stderr)
                return 1

    # 4: фриз-гейт против коммита
    if os.path.exists(COMMITTED):
        with open(COMMITTED, "rb") as fh:
            committed = fh.read()
        with tempfile.NamedTemporaryFile(suffix=".html", delete=False) as tf:
            fresh_path = tf.name
        try:
            build(fresh_path)
            with open(fresh_path, "rb") as fh:
                fresh = fh.read()
        finally:
            os.unlink(fresh_path)
        if committed != fresh:
            print("FAIL: закоммиченная docs/actor_explorer.html отличается от свежей сборки "
                  "(ручная правка или устаревшая генерация)", file=sys.stderr)
            return 1
    else:
        print("SKIP: docs/actor_explorer.html ещё не закоммичен", file=sys.stderr)

    print("OK: actor explorer воспроизводим, детерминирован, числа сходятся, "
          "закоммиченная страница равна свежей сборке")
    return 0


if __name__ == "__main__":
    sys.exit(main())

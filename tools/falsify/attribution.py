#!/usr/bin/env python3
"""attribution.py: матрица «ложь → оракул» и time-to-detection по git-истории.

Воспроизводимо третьей стороной: все времена — из timestamp'ов коммитов
(создание лжи = коммит, её вносящий; поимка = коммит-фикс), исполнители —
из коммит-месседжей. Никаких ручных времён.

Классы лжи: contract (контрактный слой), crypto (секреты/подписи),
econ (бюджет/токены), concurrency, stats (статистика/верификатор),
journal (ложь в журнале/документации), test-fix (фиктивный тест),
infra (инфраструктура процесса).
"""
import subprocess, json, statistics

REPO = "/Users/amir/Downloads/alashi"

def ts(ref):
    out = subprocess.run(["git", "-C", REPO, "log", "-1", "--format=%ct", ref],
                         capture_output=True, text=True).stdout.strip()
    return int(out)

# (id, класс, коммит-создание, коммит-фикс, кто поймал, механизм)
LIES = [
    ("L1 attestation-in-POST", "contract", "7a08dedc", "f2f69e6d", "hy4-контекстный", "фальсификация Diff 2/3"),
    ("L2 ssrf-endpoint-bearer", "crypto", "a2d7b4b5", "f2f69e6d", "hy4-контекстный", "фальсификация Diff 2/3"),
    ("L3 budget-no-breakdown", "econ", "a2d7b4b5", "f2f69e6d", "hy4-контекстный", "фальсификация Diff 2/3"),
    ("L4 race-serialization", "concurrency", "a2d7b4b5", "f2f69e6d", "hy4-контекстный", "фальсификация Diff 2/3"),
    ("L5 verify-replay-accept", "stats", "a2d7b4b5", "f2f69e6d", "hy4-контекстный", "фальсификация Diff 2/3"),
    ("L6 timedout-from-log", "stats", "9ddbee46", "f2f69e6d", "hy4-контекстный", "фальсификация Diff 2/3"),
    ("L7 fail-open-migration", "infra", "aae9c204", "11b9f17f", "hy4-контекстный (перезапуск)", "фальсификация D1-D6"),
    ("L8 journal-K1-lie", "journal", "cd56b76f", "aa06d9e9", "hy4-батарея (commitment-seed)", "слепой прогон батареи"),
    ("L9 accidental-dns-killer", "test-fix", "46e8fc02", "daf1250e", "holdout-анализ", "reintroduction вне каталога"),
    ("L10 cleanup-missing", "infra", "f37b3955", "daf1250e", "третья сторона (параллельная сессия владельца)", "посторонний наблюдатель"),
    ("L11 placeholder-reveal", "infra", "f37b3955", "daf1250e", "я (отладочный прогон)", "selfapply-отладка"),
    ("L12 falsy-empty-sig", "test-fix", "af91b360", "04a53e37", "я (собственный тест)", "юнит-тест solanaTrade"),
    ("L13 seed-env-exfil", "crypto", "62b7baac", "2cf43c20", "hy4-анонимный (чистый контекст)", "анонимный аудит"),
    ("L14 usage-from-body", "econ", "7a08dedc", "af91b360", "hy4-анонимный", "анонимный аудит"),
    ("L15 negative-usage", "econ", "cd56b76f", "83d525db", "hy4-анонимный (round 2)", "double falsification"),
    ("L16 redirect-host-lie", "crypto", "cd56b76f", "83d525db", "hy4-контекстный", "double falsification"),
]

ORACLES = ["hy4-контекстный", "hy4-анонимный", "hy4-батарея (commitment-seed)",
           "holdout-анализ", "третья сторона (параллельная сессия владельца)", "я (отладочный прогон)", "я (собственный тест)"]

rows = []
for lid, cls, born, fixed, who, how in LIES:
    t0, t1 = ts(born), ts(fixed)
    rows.append({"lie": lid, "class": cls, "ttd_min": round((t1 - t0) / 60, 1),
                 "caught_by": who, "mechanism": how})

matrix = {o: {} for o in ORACLES}
for r in rows:
    for o in ORACLES:
        matrix[o][r["class"]] = matrix[o].get(r["class"], 0) + (1 if o in r["caught_by"] else 0)

ttds = sorted(r["ttd_min"] for r in rows)
print(json.dumps({
    "lies_total": len(rows),
    "ttd_min": {"median": statistics.median(ttds), "max": max(ttds),
                "min": min(ttds), "p90": ttds[int(0.9 * (len(ttds) - 1))]},
    "worst_tail": max(rows, key=lambda r: r["ttd_min"]),
    "attribution_matrix": matrix,
    "classes_blind_spots": {
        o: [c for c in {r['class'] for r in rows} if matrix[o].get(c, 0) == 0]
        for o in ORACLES},
    "rows": rows,
}, ensure_ascii=False, indent=1))

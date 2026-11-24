#!/usr/bin/env python3
"""edit_guard.py: ловушка-код для операции (b) «широкий replace с несколькими
вхождениями» (предсказанный и подтверждённый класс act-wider-than-intent,
инцидент I13). Единственный допустимый путь точечных правок в этом репо:

    python3 tools/falsify/edit_guard.py FILE OLD NEW

Отказывает (exit 1), если OLD встречается ≠ 1 раз. Для намеренной замены
всех вхождений — явный флаг --all (сознательное решение, а не случайность).
Бинарные файлы и CRLF сохраняются как есть: чтение/запись в байтах.
"""
import sys

def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    flags = set(a for a in sys.argv[1:] if a.startswith("--"))
    if len(args) != 3:
        print("usage: edit_guard.py [--all] FILE OLD NEW", file=sys.stderr)
        return 2
    path, old, new = args
    data = open(path, "rb").read()
    old_b, new_b = old.encode("utf-8"), new.encode("utf-8")
    n = data.count(old_b)
    if n == 0:
        print(f"edit_guard: anchor not found in {path}", file=sys.stderr)
        return 1
    if n > 1 and "--all" not in flags:
        print(f"edit_guard: {n} occurrences of the anchor in {path}; "
              f"refusing a wide replace (incident class act-wider-than-intent, "
              f"confirmed 24.11). Use --all to replace all deliberately.",
              file=sys.stderr)
        return 1
    open(path, "wb").write(data.replace(old_b, new_b))
    print(f"edit_guard: replaced {n if '--all' in flags else 1} occurrence(s)")
    return 0

if __name__ == "__main__":
    sys.exit(main())

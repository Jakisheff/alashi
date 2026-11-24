#!/usr/bin/env python3
"""merge_guard.py: ловушка-код для разрыва «merge-резолв без проверки сторон»
(инцидент I9). Использование при конфликте:

    python3 tools/falsify/merge_guard.py FILE   # для каждого UU-файла
    python3 tools/falsify/merge_guard.py --sides FILE   # показать обе стороны

Проверяет ОБЕ стороны конфликта на приватные маркеры и/или известные
опасные паттерны; сторона с маркерами помечается UNCLEAN — выбор
стороны без явного решения блокируется (exit 1, пока файл содержит
маркеры-конфликты или выбрана нечистая сторона без флага --allow-unclean).
"""
import re
import sys

MARKERS = re.compile(
    r"канон amir|PR R3[0-9]|отзыв R3[0-9]|/Users/amir|hy4-flash|"
    r"genesis/Obsidian|smartreading|признаки_сгенерированности", re.I)

def sides(text):
    head, *rest = text.split("=======")
    ours = head.split("<<<<<<<", 1)[-1] if "<<<<<<<" in head else ""
    theirs = rest[0].split(">>>>>>>", 1)[0] if rest else ""
    return ours, theirs

def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    flags = [a for a in sys.argv[1:] if a.startswith("--")]
    path = args[0] if args else None
    if not path:
        print("usage: merge_guard.py [--sides|--allow-unclean] FILE", file=sys.stderr)
        return 2
    text = open(path, encoding="utf-8").read()
    if "<<<<<<<" not in text:
        # резолв уже сделан: проверяем итоговый файл
        hits = MARKERS.findall(text)
        if hits and "--allow-unclean" not in flags:
            print(f"merge_guard BLOCKED: итоговый файл содержит маркеры: {hits[:3]}", file=sys.stderr)
            return 1
        print("merge_guard: ok (resolved file clean)")
        return 0
    ours, theirs = sides(text)
    o_hits, t_hits = MARKERS.findall(ours), MARKERS.findall(theirs)
    if "--sides" in flags:
        print(f"OURS:   {'UNCLEAN ' + str(o_hits[:3]) if o_hits else 'clean'}")
        print(f"THEIRS: {'UNCLEAN ' + str(t_hits[:3]) if t_hits else 'clean'}")
        return 0
    if o_hits or t_hits:
        bad = "OURS (локальный main)" if o_hits else "THEIRS (ветка)"
        print(f"merge_guard: {bad} содержит приватные маркеры — выбери ЧИСТУЮ сторону;", file=sys.stderr)
        print("запусти --sides для списка; резолв с маркерами будет заблокирован.", file=sys.stderr)
        return 1
    print("merge_guard: обе стороны чисты")
    return 0

if __name__ == "__main__":
    sys.exit(main())

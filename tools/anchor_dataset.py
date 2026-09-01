#!/usr/bin/env python3
"""Якорение датасета alashi (по Баладжи, «реестр записей»): строит Merkle-корень
по хешам файлов данных и фиксирует его в docs/anchored.json.

Запись корня ончейн (одна memo-транзакция Solana) готова командой --tx:
печатает готовый CLI-вызов. Реальная запись требует devnet SOL — на
02.09 кошелёк пуст, запись отложена до пополнения (см. PROGRESS).

Повторное якорение: запустить снова, новая запись добавится в список.
Проверка: python3 tools/anchor_dataset.py --verify."""
import hashlib, json, os, sys, datetime

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

TARGETS = [
    "data/sim/games.jsonl",
    "data/sim/games5.jsonl",
    "data/sim/games_contrib.jsonl",
    "data/sim/games5_contrib.jsonl",
    "data/sim/metrics.json",
    "data/live/party4_snapshots.jsonl",
    "data/exports/parties.jsonl",
    "data/events_stream.jsonl",
]

def sha256(b):
    return hashlib.sha256(b).hexdigest()

def merkle(leaves):
    """Парное дерево, нечётный последний дублируется. Возвращает (root, levels)."""
    level = leaves[:]
    levels = [level[:]]
    while len(level) > 1:
        if len(level) % 2:
            level.append(level[-1])
        level = [sha256(bytes.fromhex(level[i]) + bytes.fromhex(level[i + 1]))
                 for i in range(0, len(level), 2)]
        levels.append(level[:])
    return level[0], levels

def build():
    files = []
    for rel in TARGETS:
        p = os.path.join(ROOT, rel)
        if os.path.exists(p):
            files.append({"file": rel, "sha256": sha256(open(p, "rb").read()),
                          "bytes": os.path.getsize(p)})
        else:
            files.append({"file": rel, "sha256": None, "bytes": 0})
    leaves = sorted(sha256((f["file"] + (f["sha256"] or "missing")).encode())
                    for f in files)
    root, _ = merkle(leaves)
    return root, files

def main():
    root, files = build()
    ap = os.path.join(ROOT, "docs/anchored.json")
    recs = {"anchors": []}
    if os.path.exists(ap):
        recs = json.load(open(ap))
    now = datetime.datetime.now().strftime("%d.%m.%Y %H:%M")
    entry = {"root": root, "generated": now, "files": files}
    if not recs["anchors"] or recs["anchors"][-1]["root"] != root:
        recs["anchors"].append(entry)
        json.dump(recs, open(ap, "w"), ensure_ascii=False, indent=1)
        print(f"новый якорь: {root} ({now})")
    else:
        print(f"корень не изменился с прошлой генерации: {root}")
    if "--verify" in sys.argv:
        r2, _ = build()
        ok = r2 == recs["anchors"][-1]["root"]
        print("verify:", "OK" if ok else "FAIL")
        sys.exit(0 if ok else 1)
    if "--tx" in sys.argv:
        # memo-программа Solana, текст = merkle-корень
        print("# запись корня в devnet (нужен SOL на кошельке ~0.00001):")
        print(f'solana transfer {root[:44]} 0.000001 --url devnet --allow-unfunded-recipient \\')
        print("  # либо memo: строим tx с MemooJ1xyz... и текстом корня целиком")
        print("# полный корень для memo:", root)

if __name__ == "__main__":
    main()

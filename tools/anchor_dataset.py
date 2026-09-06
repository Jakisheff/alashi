#!/usr/bin/env python3
"""Verify the recorded dataset Merkle root without changing the evidence.

Default / --verify: compare current files with the latest recorded root.
--record: explicitly append a new local checksum record (not an on-chain anchor).
This tool never submits or suggests a funds transfer.
"""
import argparse, hashlib, json, os, sys, datetime

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
            with open(p, "rb") as source:
                digest = sha256(source.read())
            files.append({"file": rel, "sha256": digest,
                          "bytes": os.path.getsize(p)})
        else:
            files.append({"file": rel, "sha256": None, "bytes": 0})
    leaves = sorted(sha256((f["file"] + (f["sha256"] or "missing")).encode())
                    for f in files)
    root, _ = merkle(leaves)
    return root, files

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--verify", action="store_true", help="read-only verification (default)")
    modes.add_argument("--record", action="store_true", help="append a new local checksum record")
    args = parser.parse_args()
    root, files = build()
    ap = os.path.join(ROOT, "docs/anchored.json")
    if os.path.exists(ap):
        with open(ap) as source:
            recs = json.load(source)
    else:
        recs = {"anchors": []}
    anchors = recs.get("anchors", [])
    if not args.record:
        ok = bool(anchors) and root == anchors[-1].get("root")
        print("verify:", "OK" if ok else "FAIL (missing record or changed dataset)")
        return 0 if ok else 1
    if not anchors or anchors[-1].get("root") != root:
        recs.setdefault("anchors", []).append({
            "root": root,
            "generated": datetime.datetime.now().isoformat(timespec="seconds"),
            "files": files,
        })
        os.makedirs(os.path.dirname(ap), exist_ok=True)
        with open(ap, "w") as output:
            json.dump(recs, output, ensure_ascii=False, indent=1)
    print("local checksum record:", root)
    return 0


if __name__ == "__main__":
    sys.exit(main())

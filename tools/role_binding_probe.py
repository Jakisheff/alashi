#!/usr/bin/env python3
"""Paired behavioral probe of president identity and veto eligibility in Alashi."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
SYSTEM = """You are answering factual questions about an Alashi snapshot, not choosing a strategy.
The president_id field identifies the current president. Names and display order do not confer office.
Only the current president can veto, and only in the law phase, after the law is revealed,
and before a veto is already pending. A contribution bonus changes vote weight, not presidency.
Snapshots are independent. Previous-round office does not override current office.
Return exactly one JSON object with president_id (an integer) and veto_eligible_ids
(an array of faction IDs whose veto action would be accepted now, each tested independently).
Do not include explanations or other keys."""


def cases():
    result = []
    variants = ("base", "rename", "reorder", "contribution", "market", "pending", "hidden", "history")
    for president in range(3):
        for variant in variants:
            s = {
                "phase": "law", "president_id": president,
                "law_revealed": True, "veto_pending": False,
                "vote_weight_mode": "legacy",
                "factions": [{"id": i, "name": name, "influence": 5 if i == president else 4,
                              "cash": 100 if i == (president + 1) % 3 else 10}
                             for i, name in enumerate(("Aitore", "Botagul", "Aikorkem"))],
            }
            if variant == "rename":
                for row, name in zip(s["factions"], ("Botagul", "Aikorkem", "Aitore")):
                    row["name"] = name
            elif variant == "reorder":
                s["factions"].reverse()
            elif variant == "contribution":
                s["vote_weight_mode"] = "contribution"
                for row in s["factions"]:
                    row["skipped_action"] = row["id"] != president
                    row["vote_weight"] = row["influence"] + (2 if row["skipped_action"] else 0)
            elif variant == "market":
                s["phase"] = "market"
                s["law_revealed"] = False
            elif variant == "pending":
                s["veto_pending"] = True
            elif variant == "hidden":
                s["law_revealed"] = False
            elif variant == "history":
                s["previous_round"] = {"president_id": (president + 1) % 3}
            result.append({"id": f"p{president}_{variant}", "variant": variant,
                           "snapshot": s})
    return result


def oracle_answers(items, binary):
    proc = subprocess.run([str(binary)], input="".join(json.dumps(c["snapshot"]) + "\n" for c in items),
                          text=True, capture_output=True, timeout=30, check=True)
    answers = [json.loads(line) for line in proc.stdout.splitlines()]
    if len(answers) != len(items):
        raise ValueError("oracle answer count mismatch")
    return answers


def parse_answer(raw):
    # Keep format errors visible; do not extract a convenient substring from prose.
    value = json.loads(raw)
    if not isinstance(value, dict) or set(value) != {"president_id", "veto_eligible_ids"}:
        raise ValueError("answer must contain exactly the two requested fields")
    if type(value["president_id"]) is not int or value["president_id"] not in range(3):
        raise ValueError("invalid president_id")
    ids = value["veto_eligible_ids"]
    if (not isinstance(ids, list) or any(type(i) is not int or i not in range(3) for i in ids)
            or len(ids) != len(set(ids))):
        raise ValueError("invalid veto_eligible_ids")
    return {"president_id": value["president_id"], "veto_eligible_ids": sorted(ids)}


def expected(answer):
    return {key: answer[key] for key in ("president_id", "veto_eligible_ids")}


def summarize(rows):
    by_id = {row["id"]: row for row in rows}
    pairs = []
    for p in range(3):
        for variant in ("rename", "reorder", "contribution", "history"):
            a, b = by_id[f"p{p}_base"], by_id[f"p{p}_{variant}"]
            pairs.append({"kind": variant, "a": a["id"], "b": b["id"],
                          "both_correct": a["correct"] and b["correct"]})
        if p < 2:
            a, b = by_id[f"p{p}_base"], by_id[f"p{p+1}_base"]
            pairs.append({"kind": "role_transfer", "a": a["id"], "b": b["id"],
                          "both_correct": a["correct"] and b["correct"]})
    answered = sum(r.get("answer") is not None for r in rows)
    correct = sum(r["correct"] for r in rows)
    return {
        "total": len(rows), "correct": sum(r["correct"] for r in rows),
        "answered": answered,
        "accuracy_on_answers": correct / answered if answered else None,
        "errors": sum(r["error"] is not None for r in rows),
        "by_variant": {v: {"total": sum(r["variant"] == v for r in rows),
                            "correct": sum(r["variant"] == v and r["correct"] for r in rows)}
                       for v in sorted({r["variant"] for r in rows})},
        "paired_checks": pairs,
        "prompt_tokens": sum(r.get("usage", {}).get("prompt_tokens", 0) for r in rows),
        "completion_tokens": sum(r.get("usage", {}).get("completion_tokens", 0) for r in rows),
    }


def configured_key():
    key = os.environ.get("ALASHI_LLM_KEY")
    path = Path.home() / ".config/alashi/llm.json"
    if not key and path.exists():
        key = json.loads(path.read_text()).get("key")
    if not key:
        raise ValueError("No configured Alashi LLM key; no fallback model will be substituted")
    return key


def ask_glm(snapshot, key, model, timeout):
    payload = {"model": model, "thinking": {"type": "disabled"}, "max_tokens": 128,
               "temperature": 0, "messages": [
                   {"role": "system", "content": SYSTEM},
                   {"role": "user", "content": json.dumps(snapshot, ensure_ascii=False)}]}
    request = urllib.request.Request("https://api.z.ai/api/paas/v4/chat/completions",
        data=json.dumps(payload).encode(), headers={"Authorization": f"Bearer {key}",
                                                  "Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        value = json.load(response)
    return value["choices"][0]["message"]["content"], value.get("usage", {}), value.get("model")


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=Path, required=True, help="new output directory")
    ap.add_argument("--backend", choices=("prepare", "oracle", "glm", "command"), default="prepare")
    ap.add_argument("--model", default="glm-4.5-flash")
    ap.add_argument("--command-json", help='JSON argv array; reads {system,snapshot} from stdin')
    ap.add_argument("--oracle", type=Path, default=ROOT / "arena/target/debug/role_oracle")
    ap.add_argument("--timeout", type=float, default=20)
    args = ap.parse_args()
    if not 0 < args.timeout <= 60:
        ap.error("timeout must be in (0, 60]")
    command = json.loads(args.command_json) if args.command_json else None
    if args.backend == "command" and (not isinstance(command, list) or not command
            or any(not isinstance(x, str) for x in command)):
        ap.error("command backend requires a nonempty JSON argv array")
    items = cases()
    answers = oracle_answers(items, args.oracle.resolve())
    key = configured_key() if args.backend == "glm" else None
    args.out.mkdir(parents=True, exist_ok=False)
    artifacts = [Path(__file__).resolve(), ROOT / "arena/src/bin/role_oracle.rs",
                 ROOT / "rules/src/actions.rs", args.oracle.resolve()]
    manifest = {"backend": args.backend, "model_requested": args.model if key else None,
                "command": command, "timeout_s": args.timeout, "system": SYSTEM,
                "scope": "Synthetic prompted QA, not the deployed agent decision path or DISCOVER.",
                "sha256": {str(p.relative_to(ROOT)) if p.is_relative_to(ROOT) else str(p):
                           hashlib.sha256(p.read_bytes()).hexdigest() for p in artifacts}}
    write_json(args.out / "manifest.json", manifest)
    write_json(args.out / "cases.json", [dict(c, oracle=a) for c, a in zip(items, answers)])
    if args.backend == "prepare":
        print(f"Prepared {len(items)} oracle-labeled cases in {args.out}")
        return
    paired = list(zip(items, answers))
    random.Random(826).shuffle(paired)
    rows = []
    with (args.out / "responses.jsonl").open("w") as log:
        for case, oracle in paired:
            start = time.monotonic()
            raw, answer, error, usage, returned_model = None, None, None, {}, None
            try:
                if args.backend == "oracle":
                    raw = json.dumps(expected(oracle))
                elif args.backend == "glm":
                    raw, usage, returned_model = ask_glm(case["snapshot"], key, args.model, args.timeout)
                else:
                    proc = subprocess.run(command, input=json.dumps({"system": SYSTEM, "snapshot": case["snapshot"]}),
                                          text=True, capture_output=True, timeout=args.timeout, check=True)
                    raw = proc.stdout
                answer = parse_answer(raw)
            except urllib.error.HTTPError as exc:
                error = f"HTTP {exc.code}"
            except urllib.error.URLError as exc:
                error = f"URLError:{type(exc.reason).__name__}"
            except Exception as exc:
                # Exceptions can contain request details; log the type, never auth material.
                error = type(exc).__name__
            row = dict(case, oracle=oracle, raw=raw, answer=answer, error=error, usage=usage,
                       model_returned=returned_model, elapsed_s=round(time.monotonic()-start, 3),
                       correct=answer == expected(oracle))
            rows.append(row)
            log.write(json.dumps(row, ensure_ascii=False) + "\n")
            log.flush()
            print(f"{len(rows)}/{len(items)} {case['id']}: {error or ('correct' if row['correct'] else 'wrong')}", flush=True)
    report = summarize(rows)
    report["backend"] = args.backend
    report["status"] = "completed" if report["answered"] else "no_valid_answers"
    report["interpretation"] = ("Harness positive control only; no model tested." if args.backend == "oracle"
                                else "Observed prompted QA performance; no claim about internal representations or strategy.")
    write_json(args.out / "summary.json", report)
    print(json.dumps(report, ensure_ascii=False))


if __name__ == "__main__":
    main()

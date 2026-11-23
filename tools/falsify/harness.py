#!/usr/bin/env python3
"""Falsify Harness: жёсткий харнесс проверяющего слоя (alashi).

Режимы:
  commit            зафиксировать план+seed (до всего остального)
  battery           показать выбранную по seed батарею мутантов
  selfcheck         внести каждого мутанта и потребовать красный детектор
  audit --prepare   подготовить нейтральное досье слепого аудитора (worktree)
  audit --report F  принять отчёт аудитора и посчитать recall
  reveal            сверить текущий план с commitment
"""

import argparse
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
STATE = os.path.join(REPO, ".falsify_state.json")

# --- каталог доказанных мутантов (идентификатор -> рецепт) ---------------
# Рецепт: (файл, старая_строка, новая_строка, детектор-тест).
# Детектор — тест, который ОБЯЗАН упасть на мутанте (kill) и проходить
# на чистом дереве. Новые записи — только из реальных находок.

MUTANTS = {
    "verifier-accepts-replay": {
        "file": "tools/egress-proxy/verify.mjs",
        "old": "  if (seenIds.has(r.receipt.receipt_id)) { console.error(`FAIL ${r.receipt.receipt_id}: duplicate receipt_id (replayed line)`); bad += 1; }\n  seenIds.add(r.receipt.receipt_id);",
        "new": "",
        "detector": ("node", ["--test", "server.test.mjs"], "tools/egress-proxy"),
    },
    "frozen-clock-in-test": {
        "file": "arena/src/api_tests.rs",
        "old": "        t += sim.game.phase_duration.max(1); // tied to the simulator's own field, not a magic 1",
        "new": "        // t frozen",
        "detector": ("cargo", ["test", "--locked", "--manifest-path", "arena/Cargo.toml", "--", "engine_step"], None),
    },
    "action-removed-from-gate": {
        "file": "arena/src/api_tests.rs",
        "old": "            let _ = crate::runner::apply_action(&mut sim, 0, &crate::strategies::ActionAction::Produce);\n            applies += 1;",
        "new": "            // action removed",
        "detector": ("cargo", ["test", "--locked", "--manifest-path", "arena/Cargo.toml", "--", "engine_step"], None),
    },
    "type-change-usage-string": {
        "file": "tools/agent-bootstrap/strategy-run.mjs",
        "old": "      return {response_id:receiptId(raw.id),usage:attested,report:parseCompletion(raw.choices?.[0]?.message?.content),request_failed:!response.ok,attestation};",
        "new": "      return {response_id:receiptId(raw.id),usage:attested ? JSON.stringify(attested) : attested,report:parseCompletion(raw.choices?.[0]?.message?.content),request_failed:!response.ok,attestation};",
        "detector": ("cargo", ["test", "--locked", "--manifest-path", "arena/Cargo.toml", "--", "signed_"], None),
    },
    "fail-open-migration": {
        "file": "arena/src/api.rs",
        "old": "        let snapshot_txt = match std::fs::read_to_string(legacy_snapshot) {\n            Ok(txt) => Some(txt),\n            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,\n            Err(e) => return Err(format!(\n                \"legacy snapshot {legacy_snapshot:?} существует, но не читается: {e}; отказ старта (fail-closed)\")),\n        };\n        if let Some(txt) = &snapshot_txt {\n            serde_json::from_str::<serde_json::Value>(txt).map_err(|e| format!(\n                \"legacy snapshot {legacy_snapshot:?} не парсится как JSON: {e}; отказ старта (fail-closed)\"))?;\n        }",
        "new": "        let snapshot_txt = std::fs::read_to_string(legacy_snapshot).ok();",
        "detector": ("cargo", ["test", "--locked", "--manifest-path", "arena/Cargo.toml", "--", "sqlite_migration"], None),
    },
    "service-var-as-bearer": {
        "file": "tools/egress-proxy/server.mjs",
        "old": "    if (keyEnv.startsWith('ALASHI_')) return json(res, 400, { error: 'bad_key_env', message: 'X-Alashi-Key-Env must not name an ALASHI_* service variable' });",
        "new": "",
        "detector": ("node", ["--test", "server.test.mjs"], "tools/egress-proxy"),
    },
    "extra-field-in-contract": {
        "file": "tools/agent-bootstrap/strategy-run.mjs",
        "old": "      const {attestation: _localOnly, ...reported} = entry.result ?? {};\n      const receipt=await api.post(`/strategy-runs/${id}/report`,{task:spec.id,...reported,",
        "new": "      const receipt=await api.post(`/strategy-runs/${id}/report`,{task:spec.id,...(entry.result??{}),",
        "detector": ("node", ["detectors/egress_usage_type.mjs"], "tools/falsify"),
    },
    "negative-usage-passes": {
        "file": "tools/egress-proxy/attest.mjs",
        "old": "  const num = v => (typeof v === 'number' && Number.isSafeInteger(v) && v >= 0 && v <= 1e12 ? v : null);",
        "new": "  const num = v => (typeof v === 'number' && Number.isFinite(v) ? v : null);",
        "detector": ("node", ["--test", "server.test.mjs"], "tools/egress-proxy"),
    },
}

BATTERY_K = 6
AUDIT_PROMPT_NEUTRAL = (
    "Дерево: {tree} (рабочая копия проекта, есть незакоммиченные правки). "
    "Задача — аудит качества и безопасности текущего состояния кода. Найди "
    "дефекты: неверную обработку данных, дыры в проверках, несоответствие "
    "комментариев поведению, регрессии. Проверяй сам: node-тесты в "
    "tools/egress-proxy запускаются без зависимостей; для Rust используй "
    "CARGO_TARGET_DIR={target} с фильтром по имени теста; читай git diff. "
    "Не правь код, не коммить, дерево оставь как есть. Ответ по-русски: "
    "список дефектов с файл:строка и кейсом; если дефектов нет — так и "
    "скажи."
)


def now_ms():
    return int(time.time() * 1000)


def write_state(state):
    with open(STATE, "w") as fh:
        json.dump(state, fh, indent=2)


def read_state():
    if not os.path.exists(STATE):
        print("harness: нет commitment — сначала `commit`", file=sys.stderr)
        sys.exit(2)
    with open(STATE) as fh:
        return json.load(fh)


def do_commit(args):
    catalog_ids = sorted(MUTANTS)
    salt = hashlib.sha256(os.urandom(32)).hexdigest()[:32]
    payload = json.dumps({"ts": now_ms(), "catalog": catalog_ids, "k": BATTERY_K,
                          "repo_head": git("rev-parse", "HEAD")}, sort_keys=True)
    plan_hash = hashlib.sha256((payload + salt).encode()).hexdigest()
    seed = plan_hash[:16]
    write_state({"plan_hash": plan_hash, "salt": salt, "payload": payload,
                 "seed": seed, "k": BATTERY_K,
                 "battery": battery_for(seed, BATTERY_K)})
    print(json.dumps({"plan_hash": plan_hash, "seed": seed, "k": BATTERY_K,
                      "battery": state_battery()}, ensure_ascii=False))


def battery_for(seed, k):
    rng = random.Random(seed)
    ids = sorted(MUTANTS)
    return rng.sample(ids, min(k, len(ids)))


def state_battery():
    st = read_state()
    return st["battery"]


def do_battery(args):
    st = read_state()
    print(json.dumps({"seed": st["seed"], "battery": st["battery"],
                      "epsilon_fail": 2 ** -st["k"]}, ensure_ascii=False))


def git(*argv, cwd=None):
    return subprocess.run(["git", *argv], cwd=cwd or REPO, capture_output=True,
                          text=True, check=True).stdout.strip()


def apply_mutant(mut, root, revert=False):
    path = os.path.join(root, mut["file"])
    with open(path) as fh:
        text = fh.read()
    old, new = (mut["new"], mut["old"]) if revert else (mut["old"], mut["new"])
    if text.count(old) != 1:
        return False, f"anchor not found or ambiguous in {mut['file']}"
    with open(path, "w") as fh:
        fh.write(text.replace(old, new))
    return True, "ok"


def with_mutant(mut, root, fn):
    """Inject -> fn() -> restore the EXACT original bytes. Reverting by
    string replacement breaks when the mutant's new text is empty
    (replace('', x) inserts between every character); a byte-exact backup
    closes that whole class (found by selfapply round 1)."""
    path = os.path.join(root, mut["file"])
    backup = open(path, "rb").read()
    try:
        ok, note = apply_mutant(mut, root)
        if not ok:
            return False, note, None
        result = fn()
        return True, "ok", result
    finally:
        with open(path, "wb") as fh:
            fh.write(backup)


def run_detector(mut, root):
    kind, argv, cwd = mut["detector"]
    wd = os.path.join(root, cwd) if cwd else root
    env = dict(os.environ)
    if kind == "cargo":
        env["CARGO_TARGET_DIR"] = os.path.join(REPO, "target")
    proc = subprocess.run([kind, *argv], cwd=wd, env=env, capture_output=True, text=True)
    out = proc.stdout + proc.stderr
    # Kill attribution (self-application round): a red detector is not a
    # kill by itself. Only an assertion failure of the detector counts;
    # environment failures (compile error, missing tool, timeout) are env
    # failures and must NOT inflate the kill count.
    markers = ("panicked at", "AssertionError", "assert", "FAIL ", "✖", "not ok", "FAILED")
    kind_of = "green" if proc.returncode == 0 else (
        "assert" if any(m in out for m in markers) else "env")
    return proc.returncode != 0, kind_of, out[-400:]


def detector_green(mut, root):
    killed, kind_of, _ = run_detector(mut, root)
    return not killed, kind_of


def do_selfcheck(args):
    st = read_state()
    root = tempfile.mkdtemp(prefix="falsify-selfcheck-")
    try:
        subprocess.run(["git", "worktree", "add", "--detach", root, "HEAD"],
                       cwd=REPO, check=True, capture_output=True)
        failures = []
        results = []
        for mid in st["battery"]:
            mut = MUTANTS[mid]
            # Positive control: the detector MUST be green on the clean tree
            # before injection; a red baseline means the environment, not the
            # mutant, colours the detector.
            green, why = detector_green(mut, root)
            if not green:
                failures.append(f"{mid}: BASELINE-RED (detector already failing: {why})")
                continue
            def detect():
                killed, kind_of, tail = run_detector(mut, root)
                if not killed:
                    return {"result": "survived"}
                if kind_of != "assert":
                    return {"result": "env_fail", "kind": kind_of}
                return {"result": "killed", "by": "assert"}
            ok, note, outcome = with_mutant(mut, root, detect)
            if not ok:
                failures.append(f"{mid}: inject failed: {note}")
                continue
            if outcome["result"] != "killed":
                failures.append(f"{mid}: {outcome['result'].upper()}"
                                + (f" ({outcome.get('kind')})" if outcome.get("kind") else ""))
            results.append({"mutant": mid, **outcome})
        killed_n = sum(1 for r in results if r["result"] == "killed")
        verdict = {"killed": killed_n, "total": st["k"], "results": results,
                   "failures": failures}
        print(json.dumps(verdict, ensure_ascii=False, indent=2))
        sys.exit(1 if failures else 0)
    finally:
        subprocess.run(["git", "worktree", "remove", "--force", root],
                       cwd=REPO, capture_output=True)
        shutil.rmtree(root, ignore_errors=True)


def do_audit_prepare(args):
    st = read_state()
    root = os.path.join(tempfile.gettempdir(), f"falsify-audit-{st['seed']}")
    if os.path.exists(root):
        shutil.rmtree(root)
    subprocess.run(["git", "worktree", "add", "--detach", root, "HEAD"],
                   cwd=REPO, check=True, capture_output=True)
    injected = []
    for mid in st["battery"]:
        mut = MUTANTS[mid]
        ok, note = apply_mutant(mut, root)
        if ok:
            injected.append(mid)
    prompt = AUDIT_PROMPT_NEUTRAL.format(
        tree=root, target=os.path.join(REPO, "target"))
    print(json.dumps({"audit_tree": root, "injected": injected,
                      "note": "не раскрывай список аудитору",
                      "prompt": prompt}, ensure_ascii=False, indent=2))


def do_audit_report(args):
    st = read_state()
    with open(args.file, encoding="utf-8") as fh:
        report = fh.read().lower()
    found = [mid for mid in st["battery"]
             if all(kw.lower() in report for kw in MUTANTS[mid].get("report_keywords",
                      [mid.replace("-", " ")]))]
    recall = len(found) / max(1, len(st["battery"]))
    verdict = {"battery": st["battery"], "found": found, "recall": recall,
               "threshold": 0.8, "pass": recall >= 0.8}
    print(json.dumps(verdict, ensure_ascii=False, indent=2))
    sys.exit(0 if verdict["pass"] else 1)


def do_reveal(args):
    st = read_state()
    payload = json.dumps({"ts": st["payload"] and json.loads(st["payload"]),
                          "ignored": 0}, sort_keys=True)  # placeholder
    # Настоящая проверка: payload в состоянии неизменен с момента commit.
    recomputed = hashlib.sha256((st["payload"] + st["salt"]).encode()).hexdigest()
    matches = recomputed == st["plan_hash"]
    print(json.dumps({"matches": matches, "plan_hash": st["plan_hash"]}))
    sys.exit(0 if matches else 1)


def do_anchors(args):
    """Anchor-rot monitor: every catalogue anchor must occur exactly once
    in HEAD. Rot (drifted/disappeared anchors) fails the gate."""
    problems = []
    for mid, mut in sorted(MUTANTS.items()):
        path = os.path.join(REPO, mut["file"])
        try:
            with open(path) as fh:
                text = fh.read()
        except FileNotFoundError:
            problems.append(f"{mid}: file missing {mut['file']}")
            continue
        n = text.count(mut["old"])
        if n != 1:
            problems.append(f"{mid}: anchor count {n} (expected 1) in {mut['file']}")
    print(json.dumps({"anchors": len(MUTANTS), "problems": problems},
                     ensure_ascii=False, indent=2))
    sys.exit(1 if problems else 0)


def do_selfapply(args):
    """Self-application: mutants of the harness itself. Each one must be
    caught by the protocol (selfcheck controls / external recompute);
    an uncaught self-mutant is a hole in the harness."""
    SELF_MUTANTS = {
        "noop-inject": {
            "old": "    if text.count(old) != 1:\n        return False, f\"anchor not found or ambiguous in {mut['file']}\"",
            "new": "    if False:\n        return False, f\"anchor not found or ambiguous in {mut['file']}\"",
            "how": "selfcheck must report SURVIVED (mutants never land)",
        },
        "always-killed": {
            "old": "    markers = (\"panicked at\", \"AssertionError\", \"assert\", \"FAIL \", \"✖\", \"not ok\", \"FAILED\")\n    kind_of = \"green\" if proc.returncode == 0 else (\n        \"assert\" if any(m in out for m in markers) else \"env\")\n    return proc.returncode != 0, kind_of, out[-400:]",
            "new": "    markers = (\"panicked at\", \"AssertionError\", \"assert\", \"FAIL \", \"✖\", \"not ok\", \"FAILED\")\n    kind_of = \"green\" if proc.returncode == 0 else (\n        \"assert\" if any(m in out for m in markers) else \"env\")\n    return True, 'assert', out[-400:]",
            "how": "positive control turns red -> BASELINE-RED failure",
        },
        "reveal-always-true": {
            "old": "    recomputed = hashlib.sha256((st[\"payload\"] + st[\"salt\"]).encode()).hexdigest()\n    matches = recomputed == st[\"plan_hash\"]",
            "new": "    recomputed = hashlib.sha256((st[\"payload\"] + st[\"salt\"]).encode()).hexdigest()\n    matches = True",
            "how": "with a tampered state, only an EXTERNAL recompute catches the lie",
        },
    }
    harness_path = os.path.abspath(__file__)
    backup = open(harness_path).read()
    caught, holes = [], []
    try:
        for mid, sm in SELF_MUTANTS.items():
            text = open(harness_path).read()
            if text.count(sm["old"]) != 1:
                holes.append({"self_mutant": mid, "note": "self-anchor rot"})
                continue
            open(harness_path, "w").write(text.replace(sm["old"], sm["new"]))
            try:
                if mid == "noop-inject":
                    proc = subprocess.run([sys.executable, harness_path, "selfcheck"],
                                          capture_output=True, text=True, timeout=1200)
                    ok = proc.returncode != 0 and "SURVIVED" in proc.stdout
                elif mid == "always-killed":
                    proc = subprocess.run([sys.executable, harness_path, "selfcheck"],
                                          capture_output=True, text=True, timeout=1200)
                    ok = proc.returncode != 0 and "BASELINE-RED" in proc.stdout
                else:
                    # Tamper the committed plan, run the mutated reveal (it
                    # lies "true"), then recompute externally: the auditor's
                    # independent check is the only catcher by design.
                    st = read_state()
                    tampered = dict(st)
                    tampered["payload"] = st["payload"] + "X"
                    with open(STATE, "w") as fh:
                        json.dump(tampered, fh)
                    proc = subprocess.run([sys.executable, harness_path, "reveal"],
                                          capture_output=True, text=True, timeout=60)
                    revealed_true = '"matches": true' in proc.stdout
                    recomputed = hashlib.sha256(
                        (tampered["payload"] + tampered["salt"]).encode()).hexdigest()
                    external_says_false = recomputed != tampered["plan_hash"]
                    ok = revealed_true and external_says_false
                    with open(STATE, "w") as fh:
                        json.dump(st, fh)
            finally:
                open(harness_path, "w").write(backup)
            entry = {"self_mutant": mid, "how": sm["how"]}
            (caught if ok else holes).append(entry)
        verdict = {"caught": [c["self_mutant"] for c in caught],
                   "holes": holes, "detail": caught + holes}
        print(json.dumps(verdict, ensure_ascii=False, indent=2))
        sys.exit(1 if holes else 0)
    finally:
        open(harness_path, "w").write(backup)


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("commit")
    sub.add_parser("battery")
    sub.add_parser("selfcheck")
    sub.add_parser("selfapply")
    sub.add_parser("anchors")
    a = sub.add_parser("audit")
    a.add_argument("--prepare", action="store_true")
    a.add_argument("--report", metavar="FILE")
    sub.add_parser("reveal")
    args = ap.parse_args()
    {"commit": do_commit, "battery": do_battery, "selfcheck": do_selfcheck,
     "selfapply": do_selfapply, "anchors": do_anchors,
     "reveal": do_reveal}.get(args.cmd, lambda a: None)(args)
    if args.cmd == "audit":
        if args.prepare:
            do_audit_prepare(args)
        elif args.report:
            do_audit_report(args)
        else:
            print("audit: нужен --prepare или --report FILE", file=sys.stderr)
            sys.exit(2)


if __name__ == "__main__":
    main()

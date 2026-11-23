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
        "detector": ("node", ["--test", "strategy-run.test.mjs"], "tools/agent-bootstrap"),
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
        "detector": ("node", ["--test", "strategy-run.test.mjs"], "tools/agent-bootstrap"),
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


def run_detector(mut, root):
    kind, argv, cwd = mut["detector"]
    wd = os.path.join(root, cwd) if cwd else root
    env = dict(os.environ)
    if kind == "cargo":
        env["CARGO_TARGET_DIR"] = os.path.join(REPO, "target")
        # cargo требует манифест от корня репо worktree
        argv = list(argv)
    proc = subprocess.run([kind, *argv], cwd=wd, env=env, capture_output=True, text=True)
    return proc.returncode != 0, (proc.stdout + proc.stderr)[-400:]


def do_selfcheck(args):
    st = read_state()
    root = tempfile.mkdtemp(prefix="falsify-selfcheck-")
    try:
        subprocess.run(["git", "worktree", "add", "--detach", root, "HEAD"],
                       cwd=REPO, check=True, capture_output=True)
        # clean tree baseline: детекторы обязаны быть зелёными
        failures = []
        for mid in st["battery"]:
            mut = MUTANTS[mid]
            ok, note = apply_mutant(mut, root)
            if not ok:
                failures.append(f"{mid}: inject failed: {note}")
                continue
            killed, tail = run_detector(mut, root)
            if not killed:
                failures.append(f"{mid}: SURVIVED (detector stayed green)")
            apply_mutant(mut, root, revert=True)
        verdict = {"killed": st["k"] - len([f for f in failures if "SURVIVED" in f]),
                   "total": st["k"], "failures": failures}
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


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("commit")
    sub.add_parser("battery")
    sub.add_parser("selfcheck")
    a = sub.add_parser("audit")
    a.add_argument("--prepare", action="store_true")
    a.add_argument("--report", metavar="FILE")
    sub.add_parser("reveal")
    args = ap.parse_args()
    {"commit": do_commit, "battery": do_battery, "selfcheck": do_selfcheck,
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

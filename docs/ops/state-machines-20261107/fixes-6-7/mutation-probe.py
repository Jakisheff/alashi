#!/usr/bin/env python3
"""Capture the selected browser source and every mutation-test subprocess result."""
import argparse
import difflib
import hashlib
import os
from pathlib import Path
import shlex
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("finding", choices=["6", "7"])
args = parser.parse_args()
evidence = Path(__file__).resolve().parent
root = evidence.parents[3]
source = "frontend/src/live/StreamPage.tsx" if args.finding == "6" else "frontend/src/feed.ts"
original = (root / source).read_bytes()
observed = Path(source).read_bytes()
node = "/Users/amir/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node"
command = [node, "frontend/scripts/state-repros/run.mjs", args.finding]
if args.finding == "7":
    command.append("--case=pending result")
environment = dict(os.environ)
environment["ALASHI_PLAYWRIGHT_MODULE"] = "/Users/amir/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright/index.mjs"
environment["ALASHI_TEST_FRONTEND_ROOT"] = str(Path.cwd() / "frontend")
directory = evidence / ("mutation-runs-" + args.finding)
directory.mkdir(exist_ok=True)
log_path = directory / (str(time.time_ns()) + ".log")
started = time.monotonic()
with log_path.open("x", encoding="utf-8") as log:
    log.write(f"CWD: {Path.cwd()}\nCOMMAND: {shlex.join(command)}\n")
    log.write(f"SOURCE: {source}\nSHA256: {hashlib.sha256(observed).hexdigest()}\n")
    diff = "".join(difflib.unified_diff(original.decode().splitlines(keepends=True), observed.decode().splitlines(keepends=True), fromfile="working-tree", tofile="probe-source"))
    log.write(diff or "BASELINE: source matches working tree\n")
    log.write("\nBROWSER OUTPUT:\n")
    log.flush()
    result = subprocess.run(command, env=environment, stdout=log, stderr=subprocess.STDOUT, check=False)
    log.write(f"\nEXIT_CODE: {result.returncode}\nSECONDS: {time.monotonic() - started:.3f}\n")
print(log_path)
raise SystemExit(result.returncode)

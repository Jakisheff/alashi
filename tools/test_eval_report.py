#!/usr/bin/env python3
"""CI guard: eval_report.py must reproduce the committed study's numbers.

Run: python3 tools/test_eval_report.py  (exit 0 = reproduction holds)
"""

import os
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
STUDY = os.path.join(REPO, "docs/research/harness-learning-20261020")


def main() -> int:
    proc = subprocess.run(
        [sys.executable, os.path.join(REPO, "tools/eval_report.py"), STUDY],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        print(proc.stdout)
        print(proc.stderr, file=sys.stderr)
        print("FAIL: eval_report exited non-zero on the committed study", file=sys.stderr)
        return 1
    required = [
        "completed 108",
        "mean difference: +1.33M",
        "p = 0.",
        "MATCH",
    ]
    missing = [token for token in required if token not in proc.stdout]
    if missing:
        print(f"FAIL: report is missing expected fragments: {missing}", file=sys.stderr)
        print(proc.stdout)
        return 1
    print("PASS: eval_report reproduces the committed study (108 games, +1.33M, MATCH)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

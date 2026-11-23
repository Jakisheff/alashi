#!/usr/bin/env python3
"""External commitment checker: recomputes the plan hash INDEPENDENTLY of
harness.py's reveal (which cannot be trusted to check itself - the
reveal-always-true self-mutant showed exactly that). The auditor runs this
script from the dossier; disagreement with reveal is a protocol violation.

Usage: external_check.py [state.json]  -> exit 0 if the commitment holds.
"""
import hashlib
import json
import sys
import os

state_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
    os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
    ".falsify_state.json")

st = json.load(open(state_path))
recomputed = hashlib.sha256((st["payload"] + st["salt"]).encode()).hexdigest()
matches = recomputed == st["plan_hash"]
print(json.dumps({"external_recompute": recomputed, "plan_hash": st["plan_hash"],
                  "matches": matches}))
sys.exit(0 if matches else 1)

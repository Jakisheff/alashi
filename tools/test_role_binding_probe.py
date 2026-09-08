import json
from pathlib import Path
import tempfile
import subprocess
import unittest

import role_binding_probe as probe


class RoleBindingTests(unittest.TestCase):
    def test_labels_follow_executable_rules_and_constraints(self):
        items = probe.cases()
        answers = probe.oracle_answers(items, probe.ROOT / "arena/target/debug/role_oracle")
        self.assertEqual(len(items), 24)
        for case, answer in zip(items, answers):
            p = case["snapshot"]["president_id"]
            blocked = case["variant"] in ("market", "pending", "hidden")
            self.assertEqual(answer["veto_eligible_ids"], [] if blocked else [p], case["id"])
            if case["variant"] == "base":
                errors = {r["id"]: r["error"] for r in answer["outcomes"]}
                self.assertIsNone(errors[p])
                self.assertTrue(all(errors[i] == "NotPresident" for i in range(3) if i != p))
            if case["variant"] == "contribution":
                self.assertTrue(all(r["vote_weight"] > 5 for r in case["snapshot"]["factions"] if r["id"] != p))

    def test_identity_and_position_shortcuts_fail_paired_checks(self):
        items = probe.cases()
        answers = probe.oracle_answers(items, probe.ROOT / "arena/target/debug/role_oracle")
        for policy in (lambda s: 0, lambda s: s["factions"][0]["id"],
                       lambda s: next(r["id"] for r in s["factions"] if r["name"] == "Aitore")):
            rows = []
            for case, oracle in zip(items, answers):
                guessed = {"president_id": policy(case["snapshot"]),
                           "veto_eligible_ids": [policy(case["snapshot"])]}
                rows.append(dict(case, correct=guessed == probe.expected(oracle), error=None))
            report = probe.summarize(rows)
            self.assertLess(report["correct"], report["total"])
            self.assertFalse(all(p["both_correct"] for p in report["paired_checks"]))

    def test_schema_rejects_booleans_duplicates_and_prose(self):
        for raw in ('{"president_id":true,"veto_eligible_ids":[]}',
                    '{"president_id":0,"veto_eligible_ids":[0,0]}',
                    '{"president_id":0,"veto_eligible_ids":[false]}',
                    '{"president_id":0,"veto_eligible_ids":[],"explanation":"x"}',
                    'Answer: {"president_id":0,"veto_eligible_ids":[0]}'):
            with self.assertRaises(ValueError):
                probe.parse_answer(raw)

    def test_command_failures_are_retained_and_output_cannot_be_overwritten(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "run"
            command = ["python3", str(probe.ROOT / "tools/role_binding_probe.py"),
                       "--backend", "command", "--command-json", '["false"]', "--out", str(out)]
            subprocess.run(command, check=True, capture_output=True, timeout=30)
            summary = json.loads((out / "summary.json").read_text())
            self.assertEqual((summary["total"], summary["errors"], summary["correct"]), (24, 24, 0))
            before = (out / "summary.json").read_bytes()
            retry = subprocess.run(command, capture_output=True, timeout=30)
            self.assertNotEqual(retry.returncode, 0)
            self.assertEqual((out / "summary.json").read_bytes(), before)


if __name__ == "__main__":
    unittest.main()

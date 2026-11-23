#!/usr/bin/env python3
"""Unit-тесты харнесса фальсификации (stdlib unittest; закрывают
mutator-хвост из unclebob-гейтов: у харнесса теперь есть тесты,
убивающие операторные мутанты)."""
import importlib.util
import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("harness", os.path.join(HERE, "harness.py"))
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)


class BatteryDeterminism(unittest.TestCase):
    def test_same_seed_same_battery(self):
        self.assertEqual(harness.battery_for("seed1", 4), harness.battery_for("seed1", 4))

    def test_different_seed_different_battery_expected(self):
        # Не обязано отличаться на конкретной паре, но на 20 парах обязано
        differing = sum(1 for i in range(20)
                        if harness.battery_for(f"s{i}", 4) != harness.battery_for(f"s{i+100}", 4))
        self.assertGreater(differing, 0)

    def test_k_bounded_by_catalog(self):
        self.assertLessEqual(len(harness.battery_for("x", 99)), len(harness.MUTANTS))


class CatalogIntegrity(unittest.TestCase):
    def test_every_mutant_has_detector_and_unique_id(self):
        seen = set()
        for mid, mut in harness.MUTANTS.items():
            self.assertNotIn(mid, seen); seen.add(mid)
            self.assertIn("file", mut); self.assertIn("old", mut)
            self.assertIn("new", mut); self.assertIn("detector", mut)


class WithMutant(unittest.TestCase):
    def test_inject_and_restore(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "f.txt")
            open(path, "w").write("alpha\nbeta\n")
            mut = {"file": "f.txt", "old": "alpha", "new": "gamma", "detector": ("true", [], None)}
            result = harness.with_mutant(mut, d, lambda: 42)
            self.assertEqual(result, (True, "ok", 42))
            self.assertEqual(open(path).read(), "alpha\nbeta\n")

    def test_missing_anchor_reported(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "f.txt")
            open(path, "w").write("nothing here\n")
            mut = {"file": "f.txt", "old": "alpha", "new": "gamma", "detector": ("true", [], None)}
            ok, note, res = harness.with_mutant(mut, d, lambda: None)
            self.assertFalse(ok); self.assertIn("anchor", note)


class ExternalCheck(unittest.TestCase):
    def test_external_check_detects_tamper(self):
        state = {"plan_hash": "aa" * 32, "salt": "s", "payload": "pp"}
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
            json.dump(state, fh); p = fh.name
        import hashlib, subprocess
        good = hashlib.sha256((state["payload"] + state["salt"]).encode()).hexdigest()
        state2 = dict(state, plan_hash=good)
        json.dump(state2, open(p, "w"))
        r = subprocess.run([sys.executable, os.path.join(HERE, "external_check.py"), p],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0)
        state3 = dict(state, payload="other")
        json.dump(state3, open(p, "w"))
        r2 = subprocess.run([sys.executable, os.path.join(HERE, "external_check.py"), p],
                            capture_output=True, text=True)
        self.assertNotEqual(r2.returncode, 0)
        os.unlink(p)


if __name__ == "__main__":
    unittest.main(verbosity=1)

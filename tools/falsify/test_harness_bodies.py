#!/usr/bin/env python3
"""Расширение юнит-тестов харнесса: тела функций, убивающие операторные
мутанты (mutator-хвост: API покрыт, тела — нет). Каждый тест нацелен
на конкретную строку логики."""
import importlib.util
import json
import subprocess
import sys
import os
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("harness", os.path.join(HERE, "harness.py"))
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)


class BatteryForBodies(unittest.TestCase):
    """Тела battery_for: мутации rng.sample / min / диапазона."""

    def test_battery_k_zero_returns_empty(self):
        self.assertEqual(harness.battery_for("s", 0), [])

    def test_battery_k_negative_returns_empty(self):
        self.assertEqual(harness.battery_for("s", -3), [])

    def test_battery_k_exact_catalog_size_returns_all(self):
        self.assertEqual(sorted(harness.battery_for("s", len(harness.MUTANTS))),
                         sorted(harness.MUTANTS.keys()))

    def test_battery_members_from_catalog_only(self):
        for i in range(30):
            b = harness.battery_for(f"seed{i}", 4)
            self.assertTrue(all(m in harness.MUTANTS for m in b), f"unknown member in {b}")
            self.assertEqual(len(set(b)), len(b), f"duplicate in battery: {b}")

    def test_battery_respects_k(self):
        for k in (1, 3, 5, len(harness.MUTANTS)):
            self.assertEqual(len(harness.battery_for("s", k)), min(k, len(harness.MUTANTS)))


class AnchorMatching(unittest.TestCase):
    """apply_mutant: точное совпадение якоря."""

    def setUp(self):
        self.d = tempfile.mkdtemp(prefix="amtest-")
        with open(os.path.join(self.d, "f.txt"), "w") as fh:
            fh.write("alpha\nbeta\nalpha\n")

    def tearDown(self):
        import shutil
        shutil.rmtree(self.d, ignore_errors=True)

    def test_duplicate_anchor_rejected(self):
        mut = {"file": "f.txt", "old": "alpha", "new": "x", "detector": ("true", [], None)}
        ok, note, _ = harness.with_mutant(mut, self.d, lambda: None)
        self.assertFalse(ok)
        self.assertIn("ambiguous", note)

    def test_replacement_actually_writes(self):
        with open(os.path.join(self.d, "g.txt"), "w") as fh:
            fh.write("one\ntwo\n")
        mut = {"file": "g.txt", "old": "one", "new": "ONE", "detector": ("true", [], None)}
        ok, note, _ = harness.with_mutant(mut, self.d, lambda: None)
        self.assertTrue(ok)

    def test_restore_is_byte_exact(self):
        with open(os.path.join(self.d, "h.txt"), "wb") as fh:
            fh.write(b"\x00\xffbin\x00ary\n")
        mut = {"file": "h.txt", "old": "bin\x00ary", "new": "X", "detector": ("true", [], None)}
        original = open(os.path.join(self.d, "h.txt"), "rb").read()
        ok, note, _ = harness.with_mutant(mut, self.d, lambda: None)
        self.assertFalse(ok, "binary anchors must be refused, not corrupted")
        self.assertIn("binary", note)
        self.assertEqual(open(os.path.join(self.d, "h.txt"), "rb").read(), original,
                         "refusal must leave the file byte-exact")

    def test_empty_old_rejected(self):
        mut = {"file": "f.txt", "old": "", "new": "x", "detector": ("true", [], None)}
        ok, note, _ = harness.with_mutant(mut, self.d, lambda: None)
        # пустой old: count("") == len+1 → ambiguous
        self.assertFalse(ok)


class CommitDeterminism(unittest.TestCase):
    """do_commit: план-хеш зависит от каталога и HEAD, salt свежий."""

    def test_commit_then_reveal_roundtrip(self):
        import subprocess
        state_path = os.path.join(os.path.dirname(HERE), "..", ".falsify_state.json")
        backup = open(state_path).read() if os.path.exists(state_path) else None
        try:
            import io, contextlib
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                harness.do_commit(None)
            st = json.load(open(state_path))
            self.assertIn("plan_hash", st)
            self.assertIn("audit_secret", st)
            # повторный commit: salt свежий → хеи различны
            buf2 = io.StringIO()
            with contextlib.redirect_stdout(buf2):
                harness.do_commit(None)
            st2 = json.load(open(state_path))
            self.assertNotEqual(st["plan_hash"], st2["plan_hash"])
        finally:
            if backup is not None:
                open(state_path, "w").write(backup)
            elif os.path.exists(state_path):
                os.unlink(state_path)


class ReadStateGuard(unittest.TestCase):
    """read_state: отсутствие state — явный exit 2, не исключение."""

    def test_missing_state_exits_two(self):
        env_backup = os.environ.get("ALASHI_STATE")
        try:
            code = (
                "import importlib.util, os, sys\n"
                f"spec = importlib.util.spec_from_file_location('h', {os.path.abspath('tools/falsify/harness.py')!r})\n"
                "h = importlib.util.module_from_spec(spec); spec.loader.exec_module(h)\n"
                "h.STATE = '/nonexistent/falsify_state.json'\n"
                "h.read_state()\n"
            )
            r = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True)
            self.assertEqual(r.returncode, 2)
            self.assertIn("commitment", r.stderr)
        finally:
            pass


class KillAttribution(unittest.TestCase):
    """run_detector: атрибуция kill по маркерам ассерта в выводе."""

    def test_green_when_zero_exit(self):
        mut = {"detector": ("true", [], None)}
        killed, kind, _ = harness.run_detector(mut, self.d if hasattr(self, 'd') else "/tmp")
        self.assertFalse(killed)
        self.assertEqual(kind, "green")

    def test_env_fail_when_red_without_markers(self):
        mut = {"detector": ("sh", ["-c", "echo plain-failure >&2; exit 1"], None)}
        killed, kind, tail = harness.run_detector(mut, "/tmp")
        self.assertTrue(killed)
        self.assertEqual(kind, "env")

    def test_assert_when_marker_in_output(self):
        mut = {"detector": ("sh", ["-c", "echo 'panicked at something'; exit 1"], None)}
        killed, kind, _ = harness.run_detector(mut, "/tmp")
        self.assertTrue(killed)
        self.assertEqual(kind, "assert")


if __name__ == "__main__":
    unittest.main(verbosity=1)

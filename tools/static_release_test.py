"""Focused offline checks for the static release pointer switch."""
import fcntl
import json
import subprocess
import sys
from pathlib import Path
import tempfile
import unittest

import static_release as release

FRONTEND = "a" * 40
BACKEND = "b" * 40


class StaticReleaseTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / "web"
        self.state = self.base / "private"
        (self.root / "releases").mkdir(parents=True)
        for name in ("base", "one", "two"):
            self.make_release(name)
        (self.root / "current").symlink_to(self.root / "releases" / "base")

    def make_release(self, name):
        path = self.root / "releases" / name
        (path / "assets").mkdir(parents=True)
        (path / "assets" / "app.js").write_text("console.log(1)")
        (path / "assets" / "app.css").write_text("body{}")
        (path / "index.html").write_text(
            '<script src="/releases/base/assets/app.js"></script>'
            '<link rel="stylesheet" href="/releases/' + name + '/assets/app.css">'
        )
        (path / "version.json").write_text(json.dumps({
            "frontend_commit": FRONTEND, "backend_commit": BACKEND,
            "deck": "r10", "release": name
        }))

    def command(self, action, *args):
        return subprocess.run(
            [sys.executable, str(Path(release.__file__)), action, *args,
             "--root", str(self.root), "--state-dir", str(self.state)],
            check=True, capture_output=True, text=True,
        )

    def test_two_publishes_and_rollback_keep_old_urls(self):
        self.command("publish", "one")
        self.command("publish", "two")
        self.assertEqual(release.pointer(self.root, "current"), "two")
        self.assertEqual(release.pointer(self.root, "previous"), "one")
        self.command("rollback")
        self.assertEqual(release.pointer(self.root, "current"), "one")
        self.assertEqual(release.pointer(self.root, "previous"), "two")
        self.assertTrue((self.root / "releases" / "base" / "assets" / "app.js").is_file())
        events = [json.loads(line) for line in (self.state / "journal.jsonl").read_text().splitlines()]
        self.assertEqual([e["phase"] for e in events], ["attempt", "complete"] * 3)
        self.assertEqual((self.state / "journal.jsonl").stat().st_mode & 0o777, 0o600)

    def test_invalid_asset_and_symlink_fail_closed(self):
        page = self.root / "releases" / "one" / "index.html"
        page.write_text('<script src="/releases/gone/assets/app.js"></script>')
        with self.assertRaises(release.InvalidRelease):
            release.run("publish", self.root, self.state, "one")
        self.assertEqual(release.pointer(self.root, "current"), "base")
        self.assertIsNone(release.pointer(self.root, "previous", required=False))
        self.assertFalse((self.state / "journal.jsonl").exists())
        self.make_release_again("one")
        (self.root / "releases" / "one" / "assets" / "outside").symlink_to("/etc/passwd")
        with self.assertRaises(release.InvalidRelease):
            release.run("publish", self.root, self.state, "one")

    def make_release_again(self, name):
        path = self.root / "releases" / name
        (path / "index.html").write_text(
            '<script src="/releases/base/assets/app.js"></script>'
            '<link rel="stylesheet" href="/releases/' + name + '/assets/app.css">'
        )

    def test_contention_fails_without_switch(self):
        self.state.mkdir(mode=0o700)
        with (self.state / "lock").open("w") as lock:
            (self.state / "lock").chmod(0o600)
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaisesRegex(release.InvalidRelease, "lock is held"):
                release.run("publish", self.root, self.state, "one")
        self.assertEqual(release.pointer(self.root, "current"), "base")


if __name__ == "__main__":
    unittest.main()

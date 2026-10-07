#!/usr/bin/env python3
"""Switch prepared static releases; no build, upload, or HTTP checks."""
import argparse
import fcntl
from html.parser import HTMLParser
import json
import os
from pathlib import Path
import re
import stat
from datetime import datetime, timezone
from urllib.parse import unquote, urlsplit
from uuid import uuid4

NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*\Z")
SHA = re.compile(r"[0-9a-f]{40}\Z")
DEFAULT_STATE = Path("/home/ubuntu/.local/state/alashi-publisher")


class InvalidRelease(Exception):
    pass


def fail(message):
    raise InvalidRelease(message)


def safe_name(name):
    if name in (".", "..") or not NAME.fullmatch(name):
        fail("unsafe release name: " + name)
    return name


def private_file(path):
    fd = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    if os.fstat(fd).st_mode & 0o077:
        os.close(fd)
        fail("state file is not private: " + str(path))
    return fd


class Assets(HTMLParser):
    def __init__(self):
        super().__init__()
        self.urls = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "script" and attrs.get("src"):
            self.urls.append(attrs["src"])
        if tag == "link" and "stylesheet" in attrs.get("rel", "").split() and attrs.get("href"):
            self.urls.append(attrs["href"])


def validate_release(root, name):
    safe_name(name)
    releases = root / "releases"
    path = releases / name
    if not stat.S_ISDIR(path.lstat().st_mode):
        fail("release is not a real directory: " + name)
    has_asset = False
    for base, dirs, files in os.walk(path, followlinks=False):
        for item in dirs + files:
            p = Path(base) / item
            lower = item.lower()
            if (lower.startswith(".env") or lower in {".git", ".ssh", ".aws", ".npmrc", ".pypirc",
                                                     "id_rsa", "id_ed25519"}
                    or lower.endswith((".pem", ".key", ".p12", ".pfx"))
                    or any(word in lower for word in ("private_key", "credentials", "secret"))):
                fail("secret-like path in release: " + str(p.relative_to(path)))
            mode = p.lstat().st_mode
            if not (stat.S_ISDIR(mode) or stat.S_ISREG(mode)):
                fail("symlink or special file in release: " + str(p.relative_to(path)))
            if p.parent == path / "assets" and stat.S_ISREG(mode) and p.stat().st_size:
                has_asset = True
    for filename in ("index.html", "version.json"):
        p = path / filename
        if not stat.S_ISREG(p.lstat().st_mode) or not p.stat().st_size:
            fail("missing or empty " + filename)
    if not stat.S_ISDIR((path / "assets").lstat().st_mode) or not has_asset:
        fail("missing or empty assets/")
    try:
        version = json.loads((path / "version.json").read_text())
    except (ValueError, UnicodeError) as exc:
        fail("invalid version.json: " + str(exc))
    if not isinstance(version, dict):
        fail("version.json must be an object")
    for key in ("frontend_commit", "backend_commit"):
        if not isinstance(version.get(key), str) or not SHA.fullmatch(version[key]):
            fail("version.json needs a full " + key + " SHA")
    if not isinstance(version.get("deck"), str) or not version["deck"].strip():
        fail("version.json needs a nonempty deck")
    if version.get("release") not in (None, name):
        fail("version.json release does not match directory")

    parser = Assets()
    parser.feed((path / "index.html").read_text())
    if not parser.urls:
        fail("index.html has no script or stylesheet")
    for url in parser.urls:
        parts = urlsplit(url)
        asset_path = unquote(parts.path)
        segments = asset_path.split("/")
        if (parts.scheme or parts.netloc or len(segments) < 4 or segments[:2] != ["", "releases"]
                or not NAME.fullmatch(segments[2]) or any(part in ("", ".", "..") for part in segments[3:])):
            fail("asset URL must be under /releases/<name>/: " + url)
        asset = root.joinpath(*segments[1:])
        if not asset.resolve().is_relative_to(releases) or not asset.is_file():
            fail("missing or escaping asset: " + url)
    return path


def pointer(root, label, required=True):
    p = root / label
    if not p.is_symlink():
        if not required and not p.exists():
            return None
        fail(label + " must be a symlink")
    target = p.resolve(strict=True)
    if target.parent != root / "releases" or not stat.S_ISDIR(target.lstat().st_mode):
        fail(label + " points outside direct releases/")
    return target.name


def replace_pointer(root, label, name):
    tmp = root / ("." + label + "." + uuid4().hex)
    try:
        os.symlink(root / "releases" / name, tmp)
        os.replace(tmp, root / label)
        directory = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if tmp.is_symlink():
            tmp.unlink()


def journal(fd, action, phase, old, new, previous):
    record = {"at_utc": datetime.now(timezone.utc).isoformat(), "action": action,
              "phase": phase, "from": old, "to": new, "previous": previous}
    data = (json.dumps(record, sort_keys=True) + "\n").encode()
    if os.write(fd, data) != len(data):
        fail("short journal write")
    os.fsync(fd)


def run(action, root, state, release):
    root = root.resolve(strict=True)
    state = state.resolve()
    if not stat.S_ISDIR(root.lstat().st_mode) or root.stat().st_mode & 0o002:
        fail("root must be a real directory without world write")
    if not stat.S_ISDIR((root / "releases").lstat().st_mode):
        fail("releases/ must be a real directory")
    if state == root or state.is_relative_to(root):
        fail("state directory must be outside web root")
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    if not stat.S_ISDIR(state.lstat().st_mode) or state.stat().st_mode & 0o077:
        fail("state directory must be private")
    lock = private_file(state / "lock")
    try:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            fail("publisher lock is held")
        current = pointer(root, "current")
        previous = pointer(root, "previous", required=False)
        if previous == current:
            fail("current and previous must differ")
        validate_release(root, current)
        if previous is not None:
            validate_release(root, previous)
        if action == "publish":
            new = safe_name(release)
            if new == current:
                fail("release is already current")
        else:
            if previous is None:
                fail("no previous release")
            new = previous
        validate_release(root, new)
        journal_fd = private_file(state / "journal.jsonl")
        try:
            journal(journal_fd, action, "attempt", current, new, previous)
            try:
                replace_pointer(root, "previous", current)
                replace_pointer(root, "current", new)
            except Exception:
                journal(journal_fd, action, "failed", current, new, previous)
                raise
            journal(journal_fd, action, "complete", current, new, previous)
        finally:
            os.close(journal_fd)
        print(action + ": " + current + " -> " + new)
    finally:
        os.close(lock)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    for action in ("publish", "rollback"):
        command = commands.add_parser(action)
        if action == "publish":
            command.add_argument("release")
        command.add_argument("--root", type=Path, default=Path("/var/www/alashi"))
        command.add_argument("--state-dir", type=Path, default=DEFAULT_STATE)
    args = parser.parse_args()
    try:
        run(args.action, args.root, args.state_dir, getattr(args, "release", None))
    except (InvalidRelease, OSError, UnicodeError) as exc:
        parser.exit(1, "static release: " + str(exc) + "\n")


if __name__ == "__main__":
    main()

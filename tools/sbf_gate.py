#!/usr/bin/env python3
"""Mechanical gate for the SBF artifact used by `cargo test -p alashi`.

The program tests embed target/deploy/alashi.so via include_bytes. History
(PROGRESS.md 07.09, entries 559-563) shows that a stale or foreign .so
produces litesvm Access violations that look like code bugs. The gate makes
that artifact class detectable before tests run:

  1. sha256 of the .so must match tools/build_sbf.sh's build manifest;
  2. ELF header fields must match the manifest (64-bit LE, machine 247);
  3. no file in programs/alashi sources may be newer than the manifest.

Usage:
  python3 tools/sbf_gate.py                    # check, exit 0 or 2
  python3 tools/sbf_gate.py --write-manifest   # after a fresh build
"""

import argparse
import datetime
import hashlib
import json
import os
import struct
import sys

ARTIFACT_REL = "target/deploy/alashi.so"
MANIFEST_REL = "target/sbf-v1.54/build-manifest.json"
# Classic BPF is EM_BPF=247, but the Solana toolchain (cargo-build-sbf 4.1.0,
# platform-tools v1.54) emits EM_SBF=263 (0x107); observed on a real CI build
# 15.11 after the gate refused with 263 != 247. Both are the SBF family;
# anything else (x86-64=62 etc.) is a foreign artifact.
SBF_ELF_MACHINES = (247, 263)
SOURCE_PATHS = (
    "programs/alashi/src",
    "programs/alashi/Cargo.toml",
    "Cargo.lock",
)
EXPECTED_TOOL = "cargo-build-sbf 4.1.0"
EXPECTED_PLATFORM_TOOLS = "v1.54"
EXPECTED_ARCH = "v0"


def parse_elf_header(data: bytes) -> dict:
    if len(data) < 52:
        return {"ok": False, "reason": "shorter than a 64-bit ELF header"}
    header = {
        "ok": data[:4] == b"\x7fELF",
        "class": data[4],
        "data": data[5],
    }
    header["e_type"], header["e_machine"] = struct.unpack("<HH", data[16:20])
    header["e_flags"] = struct.unpack("<I", data[48:52])[0]
    return header


def sha256_file(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_mtimes(repo: str) -> list:
    stamps = []
    for rel in SOURCE_PATHS:
        path = os.path.join(repo, rel)
        if os.path.isfile(path):
            stamps.append((rel, os.path.getmtime(path)))
        elif os.path.isdir(path):
            for root, _dirs, files in os.walk(path):
                for name in files:
                    file_path = os.path.join(root, name)
                    stamps.append(
                        (os.path.relpath(file_path, repo), os.path.getmtime(file_path))
                    )
    return stamps


def write_manifest(repo: str) -> dict:
    artifact = os.path.join(repo, ARTIFACT_REL)
    if not os.path.isfile(artifact):
        raise SystemExit(f"no artifact at {ARTIFACT_REL}; build first")
    with open(artifact, "rb") as handle:
        header = parse_elf_header(handle.read(64))
    now = datetime.datetime.now(datetime.timezone.utc)
    newest_source, _newest_mtime = max(source_mtimes(repo), key=lambda item: item[1])
    manifest = {
        "schema": 1,
        "tool": EXPECTED_TOOL,
        "platform_tools": EXPECTED_PLATFORM_TOOLS,
        "arch": EXPECTED_ARCH,
        "artifact": ARTIFACT_REL,
        "sha256": sha256_file(artifact),
        "size": os.path.getsize(artifact),
        "elf": header,
        "built_at_unix": now.timestamp(),
        "built_at_iso": now.isoformat(timespec="seconds"),
        "newest_source_at_build": newest_source,
    }
    manifest_path = os.path.join(repo, MANIFEST_REL)
    os.makedirs(os.path.dirname(manifest_path), exist_ok=True)
    with open(manifest_path, "w", encoding="utf-8") as handle:
        json.dump(manifest, handle, indent=2, sort_keys=True)
        handle.write("\n")
    return manifest


def check(repo: str) -> tuple:
    """Return (failures, manifest, artifact_path)."""
    failures = []
    artifact = os.path.join(repo, ARTIFACT_REL)
    manifest_path = os.path.join(repo, MANIFEST_REL)

    if not os.path.isfile(artifact):
        failures.append(f"{ARTIFACT_REL} missing; program tests have nothing to embed")
    if not os.path.isfile(manifest_path):
        failures.append(
            f"{MANIFEST_REL} missing; run bash tools/build_sbf.sh (it writes the manifest)"
        )
    if failures:
        return failures, None, artifact

    with open(manifest_path, encoding="utf-8") as handle:
        try:
            manifest = json.load(handle)
        except json.JSONDecodeError as error:
            failures.append(f"manifest is not valid JSON: {error}")
            return failures, None, artifact

    if manifest.get("tool") != EXPECTED_TOOL:
        failures.append(f"manifest tool {manifest.get('tool')!r} != {EXPECTED_TOOL!r}")
    if manifest.get("platform_tools") != EXPECTED_PLATFORM_TOOLS:
        failures.append(
            f"manifest platform-tools {manifest.get('platform_tools')!r} != {EXPECTED_PLATFORM_TOOLS!r}"
        )
    if manifest.get("arch") != EXPECTED_ARCH:
        failures.append(f"manifest arch {manifest.get('arch')!r} != {EXPECTED_ARCH!r}")

    actual_sha = sha256_file(artifact)
    if actual_sha != manifest.get("sha256"):
        failures.append(
            "sha256 mismatch: the .so is not the one this build produced "
            "(foreign or modified artifact)"
        )

    with open(artifact, "rb") as handle:
        header = parse_elf_header(handle.read(64))
    if not header.get("ok"):
        failures.append("not an ELF file")
    else:
        if header["class"] != 2 or header["data"] != 1:
            failures.append("ELF is not 64-bit little-endian")
        if header["e_machine"] not in SBF_ELF_MACHINES:
            failures.append(
                f"ELF e_machine {header['e_machine']} not in {SBF_ELF_MACHINES} (SBF family)"
            )
        recorded = manifest.get("elf", {})
        for key in ("e_type", "e_flags"):
            if header[key] != recorded.get(key):
                failures.append(
                    f"ELF {key} {header[key]} != manifest {recorded.get(key)} "
                    "(artifact and manifest disagree)"
                )

    built_at = manifest.get("built_at_unix")
    if not isinstance(built_at, (int, float)):
        failures.append("manifest has no built_at_unix")
    else:
        newer = [
            rel for rel, mtime in source_mtimes(repo) if mtime > built_at + 2
        ]
        if newer:
            failures.append(
                f"artifact older than sources (newer than build: {', '.join(sorted(newer)[:5])}"
                + ("..." if len(newer) > 5 else "")
                + "); rebuild required"
            )

    return failures, manifest, artifact


def main() -> int:
    default_repo = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", default=default_repo)
    parser.add_argument("--write-manifest", action="store_true")
    args = parser.parse_args()
    repo = os.path.abspath(args.repo)

    if args.write_manifest:
        manifest = write_manifest(repo)
        print(
            f"manifest written: {MANIFEST_REL} sha256={manifest['sha256'][:16]}... "
            f"e_flags={manifest['elf'].get('e_flags')}"
        )

    failures, _manifest, _artifact = check(repo)
    if failures:
        print("SBF GATE: REFUSED", file=sys.stderr)
        for line in failures:
            print(f"  - {line}", file=sys.stderr)
        print("Fix: bash tools/build_sbf.sh, then bash tools/sbf_gate.sh", file=sys.stderr)
        return 2
    print("SBF GATE: OK (manifest, sha256, ELF header, freshness)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

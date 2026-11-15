#!/usr/bin/env python3
"""Self-test for tools/sbf_gate.py with synthetic fixtures (no SBF build).

Run: python3 tools/test_sbf_gate.py  (exit 0 = all cases pass)
"""

import json
import os
import struct
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import sbf_gate  # noqa: E402


def make_elf(machine=247, e_flags=1, e_type=1) -> bytes:
    header = bytearray(64)
    header[0:4] = b"\x7fELF"
    header[4] = 2  # 64-bit
    header[5] = 1  # little-endian
    struct.pack_into("<HH", header, 16, e_type, machine)
    struct.pack_into("<I", header, 48, e_flags)
    return bytes(header) + b"CODE-BYTES" * 8


def make_repo(tmp: str, elf: bytes, manifest: dict, source_age: float = -3600):
    """Fixture repo skeleton. source_age is relative to manifest build time."""
    artifact = os.path.join(tmp, sbf_gate.ARTIFACT_REL)
    os.makedirs(os.path.dirname(artifact), exist_ok=True)
    with open(artifact, "wb") as handle:
        handle.write(elf)
    src = os.path.join(tmp, "programs/alashi/src")
    os.makedirs(src, exist_ok=True)
    src_file = os.path.join(src, "lib.rs")
    with open(src_file, "w", encoding="utf-8") as handle:
        handle.write("// fixture\n")
    lock = os.path.join(tmp, "Cargo.lock")
    with open(lock, "w", encoding="utf-8") as handle:
        handle.write("# fixture\n")
    built_at = time.time() + source_age + 3600  # manifest newer than sources by default
    manifest = dict(manifest)
    manifest["built_at_unix"] = built_at
    manifest["elf"] = sbf_gate.parse_elf_header(elf)
    manifest["sha256"] = sbf_gate.sha256_file(artifact)
    manifest_path = os.path.join(tmp, sbf_gate.MANIFEST_REL)
    os.makedirs(os.path.dirname(manifest_path), exist_ok=True)
    with open(manifest_path, "w", encoding="utf-8") as handle:
        json.dump(manifest, handle)
    # sources older than the build by default
    stamp = built_at + source_age
    os.utime(src_file, (stamp, stamp))
    os.utime(lock, (stamp, stamp))
    return artifact


GOOD_MANIFEST = {
    "schema": 1,
    "tool": "cargo-build-sbf 4.1.0",
    "platform_tools": "v1.54",
    "arch": "v0",
    "artifact": sbf_gate.ARTIFACT_REL,
    "size": 144,
}


def case(name, expect_failures: bool, setup):
    with tempfile.TemporaryDirectory() as tmp:
        failures, _manifest, _path = setup(tmp)
        ok = bool(failures) == expect_failures
        status = "PASS" if ok else "FAIL"
        print(f"{status} {name}: {failures if failures else 'clean'}")
        return ok


def valid(tmp):
    make_repo(tmp, make_elf(), GOOD_MANIFEST)
    return sbf_gate.check(tmp)


def no_artifact(tmp):
    make_repo(tmp, make_elf(), GOOD_MANIFEST)
    os.remove(os.path.join(tmp, sbf_gate.ARTIFACT_REL))
    return sbf_gate.check(tmp)


def no_manifest(tmp):
    make_repo(tmp, make_elf(), GOOD_MANIFEST)
    os.remove(os.path.join(tmp, sbf_gate.MANIFEST_REL))
    return sbf_gate.check(tmp)


def tampered_so(tmp):
    artifact = make_repo(tmp, make_elf(), GOOD_MANIFEST)
    with open(artifact, "ab") as handle:
        handle.write(b"tampered")
    return sbf_gate.check(tmp)


def foreign_machine(tmp):
    make_repo(tmp, make_elf(machine=62), GOOD_MANIFEST)
    return sbf_gate.check(tmp)


def wrong_tool_pin(tmp):
    manifest = dict(GOOD_MANIFEST, tool="cargo-build-sbf 3.1.10")
    make_repo(tmp, make_elf(), manifest)
    return sbf_gate.check(tmp)


def stale_sources(tmp):
    make_repo(tmp, make_elf(), GOOD_MANIFEST, source_age=-7200)  # build an hour ago
    stamp = time.time()  # source touched now
    os.utime(os.path.join(tmp, "programs/alashi/src/lib.rs"), (stamp, stamp))
    return sbf_gate.check(tmp)


def elf_flags_drift(tmp):
    # manifest records e_flags from build; artifact swapped for one with other flags
    make_repo(tmp, make_elf(e_flags=1), GOOD_MANIFEST)
    artifact = os.path.join(tmp, sbf_gate.ARTIFACT_REL)
    swapped = make_elf(e_flags=4)
    with open(artifact, "wb") as handle:
        handle.write(swapped)
    # fix the sha so the failure isolates the flags drift
    manifest_path = os.path.join(tmp, sbf_gate.MANIFEST_REL)
    with open(manifest_path, encoding="utf-8") as handle:
        manifest = json.load(handle)
    manifest["sha256"] = sbf_gate.sha256_file(artifact)
    with open(manifest_path, "w", encoding="utf-8") as handle:
        json.dump(manifest, handle)
    return sbf_gate.check(tmp)


def main() -> int:
    results = [
        case("valid build passes", False, valid),
        case("missing artifact refused", True, no_artifact),
        case("missing manifest refused", True, no_manifest),
        case("tampered .so refused (sha mismatch)", True, tampered_so),
        case("foreign ELF machine refused", True, foreign_machine),
        case("unpinned toolchain refused", True, wrong_tool_pin),
        case("sources newer than build refused", True, stale_sources),
        case("elf flags drift vs manifest refused", True, elf_flags_drift),
    ]
    print(f"{sum(results)}/{len(results)} gate self-tests pass")
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Keep Cargo evidence that mutator otherwise discards for each probe."""

import argparse
import difflib
import hashlib
import shlex
import subprocess
import time
from pathlib import Path


PROBES = {
    "indexer": (
        "indexer/src/chain_api.rs",
        ["--lib", "cached_terminal_game_retries_archive_after_storage_recovers_without_rpc"],
    ),
    "bots": ("bots/src/session_cli.rs", ["session_cli::tests::v2_"]),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("probe", choices=PROBES)
    args = parser.parse_args()
    source, selection = PROBES[args.probe]
    # docs is a symlink in mutator's overlay; cwd contains the mutated source.
    evidence = Path(__file__).resolve().parent
    root = evidence.parents[2]
    original = (root / source).read_bytes()
    observed = Path(source).read_bytes()
    command = [
        "cargo", "test", "--offline", "--locked", "--manifest-path",
        f"{args.probe}/Cargo.toml", *selection,
    ]
    directory = evidence / f"{args.probe}-mutation-runs"
    directory.mkdir(exist_ok=True)
    log_path = directory / f"{time.time_ns()}.log"
    started = time.monotonic()
    with log_path.open("x", encoding="utf-8") as log:
        log.write(f"CWD: {Path.cwd()}\nCOMMAND: {shlex.join(command)}\n")
        log.write(f"SOURCE: {source}\nSHA256: {hashlib.sha256(observed).hexdigest()}\n")
        changes = "".join(difflib.unified_diff(
            original.decode().splitlines(keepends=True),
            observed.decode().splitlines(keepends=True),
            fromfile="working-tree", tofile="probe-source",
        ))
        log.write(changes or "BASELINE: source matches working tree\n")
        log.write("\nCARGO OUTPUT:\n")
        log.flush()
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=False)
        log.write(f"\nEXIT_CODE: {result.returncode}\nSECONDS: {time.monotonic() - started:.3f}\n")
    print(log_path)
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())

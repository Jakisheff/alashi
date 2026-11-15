#!/usr/bin/env python3
"""Lint agent configuration: AGENTS.md context budget and repo SKILL.md manifests.

Checks:
  1. AGENTS.md is at most 8192 bytes (compact project conventions beat
     skill-shaped documentation; oversized context raises inference cost
     and evicts other material).
  2. Every repo skill (.agents/skills/*/SKILL.md, .claude/skills/*/SKILL.md)
     has: no UTF-8 BOM, YAML frontmatter with keys inside the six allowed by
     the agentskills spec (name, description, license, compatibility,
     metadata, allowed-tools), name matching its folder, description within
     1-1024 chars. Descriptions over 250 chars draw a warning: loaders
     truncate skill listings and the trigger words must land early.

Run: python3 tools/lint_agent_configs.py  (exit 1 on any violation)
"""

import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
AGENTS_MD = os.path.join(REPO, "AGENTS.md")
AGENTS_MD_BUDGET = 8192
SKILL_DIRS = (".agents/skills", ".claude/skills")
ALLOWED_KEYS = {"name", "description", "license", "compatibility", "metadata", "allowed-tools"}
NAME_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")
DESCRIPTION_MAX = 1024
DESCRIPTION_SOFT = 250
FRONTMATTER_RE = re.compile(r"\A---\n(.*?)\n---\n", re.S)


def lint_agents_md(errors: list, warnings: list) -> None:
    if not os.path.isfile(AGENTS_MD):
        errors.append("AGENTS.md missing at repo root")
        return
    size = os.path.getsize(AGENTS_MD)
    if size > AGENTS_MD_BUDGET:
        errors.append(f"AGENTS.md is {size} bytes > {AGENTS_MD_BUDGET}: move procedures to skills or docs")


def skill_files() -> list:
    found = []
    for rel in SKILL_DIRS:
        base = os.path.join(REPO, rel)
        if not os.path.isdir(base):
            continue
        for entry in sorted(os.listdir(base)):
            manifest = os.path.join(base, entry, "SKILL.md")
            if os.path.isfile(manifest):
                found.append((rel, entry, manifest))
    return found


def lint_skill(rel_dir: str, folder: str, path: str, errors: list, warnings: list) -> None:
    label = f"{rel_dir}/{folder}"
    with open(path, "rb") as handle:
        raw = handle.read()
    if raw.startswith(b"\xef\xbb\xbf"):
        errors.append(f"{label}: SKILL.md has a UTF-8 BOM; some loaders fail to parse the frontmatter")
    text = raw.decode("utf-8", errors="replace")
    match = FRONTMATTER_RE.match(text)
    if not match:
        errors.append(f"{label}: no '---' delimited YAML frontmatter at the top")
        return
    frontmatter = match.group(1)
    keys = []
    description = None
    for line in frontmatter.splitlines():
        key_match = re.match(r"^([A-Za-z][A-Za-z0-9_-]*):", line)
        if not key_match:
            continue
        key = key_match.group(1)
        keys.append(key)
        if key == "description":
            rest = line.split(":", 1)[1].strip()
            if rest in (">-", ">", "|", ">-", "|-"):
                rest = ""
            description = rest
    if not keys:
        errors.append(f"{label}: frontmatter has no keys")
        return
    extra = [key for key in keys if key not in ALLOWED_KEYS]
    if extra:
        errors.append(f"{label}: keys outside the spec whitelist: {', '.join(extra)}")
    if "name" not in keys:
        errors.append(f"{label}: required key 'name' missing")
    else:
        name = None
        for line in frontmatter.splitlines():
            if line.startswith("name:"):
                name = line.split(":", 1)[1].strip()
                break
        if name != folder:
            errors.append(f"{label}: name {name!r} does not match folder {folder!r}")
        elif not NAME_RE.match(name) or len(name) > 64:
            errors.append(f"{label}: name {name!r} violates the spec pattern/length")
    if description is None:
        errors.append(f"{label}: required key 'description' missing")
    else:
        if not description:
            # block scalar: estimate from indented lines
            indented = [
                line.strip()
                for line in frontmatter.splitlines()
                if line.startswith("  ") and not line.lstrip().startswith("#")
            ]
            description = " ".join(indented)
        if not 1 <= len(description) <= DESCRIPTION_MAX:
            errors.append(f"{label}: description length {len(description)} outside 1..{DESCRIPTION_MAX}")
        elif len(description) > DESCRIPTION_SOFT:
            warnings.append(
                f"{label}: description is {len(description)} chars; loaders truncate listings, keep triggers in the first {DESCRIPTION_SOFT}"
            )


def main() -> int:
    errors: list = []
    warnings: list = []
    lint_agents_md(errors, warnings)
    files = skill_files()
    if not files:
        errors.append("no repo skills found under " + " or ".join(SKILL_DIRS))
    for rel_dir, folder, path in files:
        lint_skill(rel_dir, folder, path, errors, warnings)
    for line in warnings:
        print(f"WARN {line}")
    if errors:
        for line in errors:
            print(f"ERROR {line}", file=sys.stderr)
        return 1
    print(f"agent config lint OK: AGENTS.md within {AGENTS_MD_BUDGET} bytes, {len(files)} skill(s) valid")
    return 0


if __name__ == "__main__":
    sys.exit(main())

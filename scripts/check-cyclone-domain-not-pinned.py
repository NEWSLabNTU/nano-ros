#!/usr/bin/env python3
"""`CONFIG_NROS_CYCLONE_DOMAIN_ID` must never be pinned to a literal.

Issue 0974. The knob exists so a Cyclone image can run on a domain of its own,
and its Kconfig carries `default NROS_DOMAIN_ID` precisely so that it TRACKS the
generic knob unless someone deliberately separates them. Writing a literal into
a `prj.conf` breaks that link silently:

    CONFIG_NROS_DOMAIN_ID=5          # what the user set
    CONFIG_NROS_CYCLONE_DOMAIN_ID=0  # what the image actually uses

Nothing reports an error, because the domain is just the first element of every
discovery key — the peer simply never appears. That is phase-180's split-brain
(issue 0161), and CLAUDE.md has carried "never pin it to a literal in confs"
ever since.

A rule that lives only in a document gets re-broken by tooling that nobody
re-reads. Both `nros new`'s cyclonedds template and the Zephyr getting-started
page emitted `CONFIG_NROS_CYCLONE_DOMAIN_ID=0`, so every generated cyclonedds
project started split-brained and the book taught the pattern.

## What is allowed

* `zephyr/Kconfig` — the DECLARATION, which is where the default lives.
* Documentation that quotes the hazard in order to warn about it: `docs/`,
  `CLAUDE.md`, `AGENTS.md`.
* A conf that genuinely wants a separate domain can still do it — by setting the
  value through Kconfig with a comment saying why, which this gate asks for via
  an explicit marker rather than by guessing intent.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PATTERN = re.compile(r"CONFIG_NROS_CYCLONE_DOMAIN_ID\s*=\s*\d")
MARKER = "nros-allow-cyclone-domain-pin"

# Paths that may legitimately contain the string.
ALLOWED_PREFIXES = ("docs/", "book/src/reference/")
ALLOWED_EXACT = {"CLAUDE.md", "AGENTS.md", "zephyr/Kconfig",
                 "scripts/check-cyclone-domain-not-pinned.py"}


def tracked():
    out = subprocess.run(["git", "ls-files", "-z"], cwd=REPO,
                         capture_output=True, text=True, check=True).stdout
    return [n for n in out.split("\0") if n]


def allowed(rel: str) -> bool:
    return rel in ALLOWED_EXACT or rel.startswith(ALLOWED_PREFIXES)


def main() -> int:
    bad = []
    for rel in tracked():
        if allowed(rel):
            continue
        p = REPO / rel
        try:
            text = p.read_text(encoding="utf-8", errors="replace")
        except (OSError, IsADirectoryError):
            continue
        if "CONFIG_NROS_CYCLONE_DOMAIN_ID" not in text:
            continue
        lines = text.splitlines()
        for i, line in enumerate(lines, start=1):
            if not PATTERN.search(line):
                continue
            if MARKER in line or (i >= 2 and MARKER in lines[i - 2]):
                continue
            bad.append((rel, i, line.strip()))

    if not bad:
        print("check-cyclone-domain-not-pinned: OK "
              "(no conf pins the cyclone domain to a literal)")
        return 0

    print("check-cyclone-domain-not-pinned: FAIL\n")
    for rel, i, line in bad:
        print(f"  {rel}:{i}: {line}")
    print(
        "\n`CONFIG_NROS_CYCLONE_DOMAIN_ID` defaults to `NROS_DOMAIN_ID` in\n"
        "`zephyr/Kconfig` so the two cannot split. Pinning a literal breaks that\n"
        "link SILENTLY — the image runs on the pinned domain while the generic\n"
        "knob says otherwise, and discovery simply never matches because the\n"
        "domain is the first element of every key (issues 0161, 0974).\n\n"
        "Drop the line and let the default track. If an image genuinely needs a\n"
        f"separate Cyclone domain, mark the line `{MARKER}` and say why."
    )
    return 1


def self_test() -> int:
    ok = True

    def expect(name, got, want):
        nonlocal ok
        if got != want:
            ok = False
            print(f"  self-test FAIL {name}: {got!r} != {want!r}")

    expect("matches a pin", bool(PATTERN.search("CONFIG_NROS_CYCLONE_DOMAIN_ID=0")), True)
    expect("matches spaced", bool(PATTERN.search("CONFIG_NROS_CYCLONE_DOMAIN_ID = 12")), True)
    expect("ignores a mention", bool(PATTERN.search("see CONFIG_NROS_CYCLONE_DOMAIN_ID for why")), False)
    expect("ignores the Kconfig decl", bool(PATTERN.search("config NROS_CYCLONE_DOMAIN_ID")), False)
    expect("docs allowed", allowed("docs/issues/archived/0161-zephyr-cyclonedds-nextest-group-serialized.md"), True)
    expect("kconfig allowed", allowed("zephyr/Kconfig"), True)
    expect("cli template NOT allowed",
           allowed("packages/cli/nros-cli-core/src/cmd/new_entry.rs"), False)
    expect("book getting-started NOT allowed",
           allowed("book/src/getting-started/zephyr.md"), False)

    print("check-cyclone-domain-not-pinned --self-test: " + ("OK" if ok else "FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    # The selftest runs on the NORMAL path, not only behind the flag: a negative
    # control nobody runs decays into a comment, which is what
    # `check-gate-selftests` enforces. `--self-test` stays as a way to run ONLY
    # the control.
    rc = self_test()
    sys.exit(rc if rc or "--self-test" in sys.argv else main())

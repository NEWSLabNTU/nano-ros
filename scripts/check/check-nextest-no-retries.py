#!/usr/bin/env python3
"""Refuse any `retries` setting in .config/nextest.toml.

Maintainer ruling 2026-11-02: a test that fails on its first attempt and passes
on a second has a START RACE (a missing readiness edge, an ordering, too much
concurrency for its resources). The fix is that edge, or a narrower test-group
`max-threads` — never a re-roll. A retry turns the race into a green and hides
it from every lane.

The file carried 25 `retries` overrides when the ruling landed, each justified
by a comment about a race a later change had already removed (the domain slot
partition, issue 0838) or about a decision "deliberately left for later". So
the rule is a gate rather than a convention: any key named `retries`, in the
profile or in an override, at any value (an explicit `retries = 0` reads as
"this one is special", which is the drift the rule exists to stop).

Usage:
    python3 scripts/check/check-nextest-no-retries.py              # check
    python3 scripts/check/check-nextest-no-retries.py --self-test
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CONFIG = REPO / ".config" / "nextest.toml"

# A TOML key `retries` at line start, bare or inline-table (`{ retries = ... }`).
KEY_RE = re.compile(r"^\s*retries\s*=|[{,]\s*retries\s*=")


def findings(text: str) -> list[tuple[int, str]]:
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        code = line.split("#", 1)[0]
        if KEY_RE.search(code):
            out.append((n, line.strip()))
    return out


SELF_TESTS = [
    ("retries = 2\n", 1),
    ("  retries = 0\n", 1),
    ('slow-timeout = { period = "60s", retries = 1 }\n', 1),
    ("# retries = 2 used to live here\n", 0),
    ('filter = "binary(x)"\nslow-timeout = { period = "60s", terminate-after = 3 }\n', 0),
    ("max-retries = 3\n", 0),
]


def self_test() -> bool:
    bad = 0
    for text, want in SELF_TESTS:
        got = len(findings(text))
        if got != want:
            print(f"  self-test: {text!r}: want {want} finding(s), got {got}")
            bad += 1
    if bad:
        print(f"check-nextest-no-retries --self-test: {bad} case(s) FAILED")
        return False
    print(f"check-nextest-no-retries --self-test: {len(SELF_TESTS)} case(s) OK")
    return True


def main() -> int:
    if not self_test():
        return 1
    if "--self-test" in sys.argv[1:]:
        return 0
    if not CONFIG.is_file():
        print(f"check-nextest-no-retries: FAILED — {CONFIG} is missing")
        return 1
    hits = findings(CONFIG.read_text())
    if not hits:
        print("check-nextest-no-retries: OK (no retries in .config/nextest.toml)")
        return 0
    print("check-nextest-no-retries: FAIL — .config/nextest.toml sets retries:")
    for n, line in hits:
        print(f"  .config/nextest.toml:{n}: {line}")
    print(
        "\n  A test that passes only on a second attempt has a start race. Fix the\n"
        "  readiness edge, or narrow its test-group `max-threads`; do not re-roll\n"
        "  (maintainer ruling 2026-11-02, see the header of .config/nextest.toml)."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())

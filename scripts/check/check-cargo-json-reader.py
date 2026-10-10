#!/usr/bin/env python3
"""check-cargo-json-reader — one reader of cargo's `build-script-executed` stream.

RFC-0103 D8 / phase-484 W6. Non-cargo consumers find a build script's
`OUT_DIR` by reading cargo's JSON messages. Three parsers existed (one matched
the package with a bare `<name>#` substring, which also matches any package
whose name ends in it); `scripts/lib/cargo_out_dir.py` is now the only one.

This refuses a TEST of the message reason — `== "build-script-executed"`,
`!= …`, or `"build-script-executed" in <line>` — in any tracked file but that
module. Prose and error text that merely name the message are not tests.
Buildless, ~0.1 s; self-tests its pattern on each run.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HOME = "scripts/lib/cargo_out_dir.py"
TEST = re.compile(r"""(?:==|!=)\s*['"]build-script-executed['"]|['"]build-script-executed['"]\s+in\s""")


def self_test() -> None:
    for hit in ['if msg.get("reason") != "build-script-executed":', "x == 'build-script-executed'",
                'for l in sys.stdin if "build-script-executed" in l']:
        assert TEST.search(hit), hit
    for miss in ['# reads `build-script-executed` messages', 'print("no build-script-executed for x")']:
        assert not TEST.search(miss), miss


def main() -> int:
    self_test()
    out = subprocess.run(
        ["git", "-C", str(ROOT), "grep", "-n", "build-script-executed", "--", ":!docs", ":!*.md"],
        capture_output=True, text=True,
    ).stdout
    bad = [
        line for line in out.splitlines()
        if not line.startswith(HOME + ":") and not line.startswith("scripts/check/check-cargo-json-reader.py:")
        and TEST.search(line.split(":", 2)[2])
    ]
    if bad:
        print("check-cargo-json-reader: FAIL — a second reader of cargo's JSON stream:", file=sys.stderr)
        print("\n".join(f"  {b}" for b in bad), file=sys.stderr)
        print(f"  Use {HOME} (`out_dirs`, `package_matches`).", file=sys.stderr)
        return 1
    print(f"check-cargo-json-reader: OK — {HOME} is the only reader of `build-script-executed`")
    return 0


if __name__ == "__main__":
    sys.exit(main())

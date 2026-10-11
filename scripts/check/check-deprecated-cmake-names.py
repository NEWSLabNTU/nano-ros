#!/usr/bin/env python3
"""check-deprecated-cmake-names — a deprecated `NANO_ROS_*` name has one reader.

phase-484 W7b (decided 2026-10-11): cmake's `NANO_ROS_*` variables converge on
`NROS_*`; the old user-facing names stay as DEPRECATED ALIASES, adopted by
`cmake/NanoRosDeprecatedNames.cmake` at every entry point. That works only
while nothing else reads an old name — a second reader would see a value the
shim never mapped. So: outside the shim, its table's OLD names, and every
retired internal prefix (`NANO_ROS_EXPORT_`, `_LEAF_`, `_PROVIDER_`,
`_LOCATOR_`, `_DEFAULT_`), appear in no tracked file. History (`docs/design`,
`docs/roadmap`, `docs/issues`) and the one book page that tells a user the old
name is deprecated are exempt.

The W8 retirement pass narrows this to "no tracked file at all".
Buildless, ~0.3 s; self-tests its pattern on each run.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import git_grep  # noqa: E402  issue 1816 — a failed search is not "no match"
SHIM = ROOT / "cmake/NanoRosDeprecatedNames.cmake"
EXEMPT_PREFIXES = ("docs/design/", "docs/roadmap/", "docs/issues/")
EXEMPT_FILES = {
    "cmake/NanoRosDeprecatedNames.cmake",
    "scripts/check/check-deprecated-cmake-names.py",
    "book/src/reference/deprecated-names.md",
    "tests/cmake-deprecated-names-tests.sh",
}
RETIRED_PREFIXES = ["NANO_ROS_EXPORT_", "NANO_ROS_LEAF_", "NANO_ROS_PROVIDER_",
                    "NANO_ROS_LOCATOR_", "NANO_ROS_DEFAULT_"]


def aliased() -> list[str]:
    return re.findall(r"^\s+(NANO_ROS_[A-Z0-9_]+)=NROS_", SHIM.read_text(), re.M)


def pattern(olds: list[str]) -> re.Pattern[str]:
    # The left edge is "not inside a longer identifier" OR "right after `-D`":
    # `\b` alone cannot see `-DNANO_ROS_X` (D and N are both word characters),
    # which is exactly the spelling a command line uses — the first version of
    # this gate was blind to 315 of them.
    edge = r"(?:(?<=-D)|(?<![A-Za-z0-9_]))"
    parts = [rf"{edge}{re.escape(o)}\b" for o in olds] + [rf"{edge}{p}" for p in RETIRED_PREFIXES]
    return re.compile("|".join(parts))


def self_test(pat: re.Pattern[str]) -> None:
    assert pat.search("set(NANO_ROS_PLATFORM posix)")
    assert pat.search("cmake -S . -DNANO_ROS_PLATFORM=posix")
    assert not pat.search("_NANO_ROS_PLATFORM_PRIVATE")
    assert pat.search("${NANO_ROS_EXPORT_BOARD}")
    assert not pat.search("set(NROS_PLATFORM posix)")
    assert not pat.search("NANO_ROS_ROOT")  # W7b-2's, not this table's (yet)


def main() -> int:
    olds = aliased()
    assert olds, "the shim's table parsed empty — the gate would pass on nothing"
    pat = pattern(olds)
    self_test(pat)
    out = git_grep.run(["-n", "NANO_ROS_"], ROOT)
    bad = [
        line for line in out.splitlines()
        if pat.search(line.split(":", 2)[-1])
        and not line.startswith(EXEMPT_PREFIXES)
        and line.split(":", 1)[0] not in EXEMPT_FILES
    ]
    if bad:
        print("check-deprecated-cmake-names: FAIL — a deprecated NANO_ROS_* name outside the shim:",
              file=sys.stderr)
        print("\n".join(f"  {b[:160]}" for b in bad), file=sys.stderr)
        print("  Use the NROS_* name; cmake/NanoRosDeprecatedNames.cmake is the only reader "
              "of the old ones.", file=sys.stderr)
        return 1
    print(f"check-deprecated-cmake-names: OK — {len(olds)} aliased name(s) and "
          f"{len(RETIRED_PREFIXES)} retired prefix(es) appear only in the shim")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""check-retired-cmake-names — no tracked file names a retired `NANO_ROS_*` name.

phase-484 W7b renamed cmake's `NANO_ROS_*` variables to `NROS_*` and kept the
old user-facing ones as deprecated aliases for one release; W8 retired them:
`cmake/NanoRosRetiredNames.cmake` now REFUSES each one, naming its
replacement. So an old name anywhere in the live tree is either a configure
that now fails (code, an example, a workflow, a build script) or a document
teaching a spelling that no longer works (the book, docs/reference, an RFC
describing live behaviour, AGENTS.md, CLAUDE.md).

Rule: the error table's OLD names, and every retired internal prefix
(`NANO_ROS_EXPORT_`, `_LEAF_`, `_PROVIDER_`, `_LOCATOR_`, `_DEFAULT_`) and the
`NANO_ROS_*` family wildcard, appear
in no tracked file, except

  * EXEMPT_FILES — the error table itself, this gate, its cmake test, the
    user-facing "retired names" page (old -> new is its content), and the env
    gate that refuses `$NANO_ROS_ROOT` (a different rule, RFC-0103 D6);
  * HISTORY — dated records that quote the spelling of their time: archived
    issues, every phase doc (each W-item is a dated record of what was run and
    measured; rewriting its commands would falsify it), the generated issue
    index's "recently resolved" history, and the changelog (release notes
    must name what they removed). OPEN issues are NOT history: their
    reproduction commands are instructions a reader will run;
  * a line that says the name is no longer live — the word "retired" or
    "deprecated" on the same line (how an RFC keeps a sentence of history),
    the `$ENV{NANO_ROS_ROOT}` refusal, and a `cmake_parse_arguments` keyword
    list that still parses the retired `NANO_ROS_ROOT` keyword only to refuse
    it (`;NANO_ROS_ROOT;` / `;NANO_ROS_ROOT"`).

`NANO_ROS_DIR` / `nano_ros_ROOT` are CMake's own `find_package` conventions and
are not retired. Buildless, ~0.3 s; self-tests its pattern and its exemptions.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import git_grep  # noqa: E402  issue 1816 — a failed search is not "no match"
TABLE = ROOT / "cmake/NanoRosRetiredNames.cmake"
EXEMPT_FILES = {
    "cmake/NanoRosRetiredNames.cmake",          # the error table
    "scripts/check/check-retired-cmake-names.py",
    "tests/cmake-retired-names-tests.sh",       # asserts each old name fails
    "book/src/reference/retired-names.md",      # old -> new IS the page
    # RFC-0103 D6: `NANO_ROS_ROOT` as an ENVIRONMENT variable is refused by a
    # different rule; that gate names it.
    "scripts/check/check-retired-store-vars.py",
    # Generated "recently resolved" history of the issue series.
    "docs/issues/README.md",
    "CHANGELOG.md",
}
HISTORY_PREFIXES = ("docs/issues/archived/", "docs/roadmap/", "changelog.d/")
LIVE_MARKER = re.compile(
    r"\bretired\b|\bdeprecated\b|ENV\{NANO_ROS_ROOT\}|;NANO_ROS_ROOT(?:;|\")", re.I)
RETIRED_PREFIXES = ["NANO_ROS_EXPORT_", "NANO_ROS_LEAF_", "NANO_ROS_PROVIDER_",
                    "NANO_ROS_LOCATOR_", "NANO_ROS_DEFAULT_"]


def retired() -> list[str]:
    return re.findall(r"^\s+(NANO_ROS_[A-Z0-9_]+)=NROS_", TABLE.read_text(), re.M)


def pattern(olds: list[str]) -> re.Pattern[str]:
    # The left edge is "not inside a longer identifier" OR "right after `-D`":
    # `\b` alone cannot see `-DNANO_ROS_X` (D and N are both word characters),
    # which is exactly the spelling a command line uses — the W7b version of
    # this gate was blind to 315 of them.
    edge = r"(?:(?<=-D)|(?<![A-Za-z0-9_]))"
    parts = [rf"{edge}{re.escape(o)}\b" for o in olds] + [rf"{edge}{p}" for p in RETIRED_PREFIXES]
    # `NANO_ROS_*` names the whole retired family — in a current doc that is a
    # spelling the reader is being told to type (`-DNANO_ROS_*` > system.toml).
    parts.append(rf"{edge}NANO_ROS_\*")
    return re.compile("|".join(parts))


def offends(pat: re.Pattern[str], path: str, text: str) -> bool:
    if path in EXEMPT_FILES or path.startswith(HISTORY_PREFIXES):
        return False
    return bool(pat.search(text)) and not LIVE_MARKER.search(text)


def self_test(pat: re.Pattern[str]) -> None:
    assert offends(pat, "CMakeLists.txt", "set(NANO_ROS_PLATFORM posix)")
    assert offends(pat, "ci/x.yml", "cmake -S . -DNANO_ROS_PLATFORM=posix")
    assert offends(pat, "x.cmake", "${NANO_ROS_EXPORT_BOARD}")
    assert offends(pat, "x.cmake", "set(NANO_ROS_ROOT /x)")
    assert offends(pat, "docs/issues/" + "9999-open.md", "-DNANO_ROS_PLATFORM=threadx")  # open issue
    assert offends(pat, "book/src/x.md", "-DNANO_ROS_RMW=zenoh")
    assert offends(pat, "book/src/x.md", "flag (`-DNANO_ROS_*`) > system.toml")
    assert not offends(pat, "x.cmake", "_NANO_ROS_PLATFORM_PRIVATE")
    assert not offends(pat, "x.cmake", "set(NROS_PLATFORM posix)")
    assert not offends(pat, "x.cmake", "find_package(nano_ros) # NANO_ROS_DIR")
    assert not offends(pat, "docs/design/x.md", "the retired `NANO_ROS_BOARD` guard")
    assert not offends(pat, "x.cmake", 'if(DEFINED ENV{NANO_ROS_ROOT})')
    assert not offends(pat, "x.cmake", '"SYSTEM;NROS_REPO_DIR;NANO_ROS_ROOT;BACKEND"')
    assert not offends(pat, "docs/issues/archived/" + "0001-old.md", "-DNANO_ROS_RMW=x")
    assert not offends(pat, "docs/roadmap/phase-1.md", "-DNANO_ROS_RMW=x")


def main() -> int:
    olds = retired()
    assert olds, "the error table parsed empty — the gate would pass on nothing"
    pat = pattern(olds)
    self_test(pat)
    out = git_grep.run(["-n", "NANO_ROS_"], ROOT)
    bad = []
    for line in out.splitlines():
        path, _, rest = line.partition(":")
        text = rest.split(":", 1)[-1]
        if offends(pat, path, text):
            bad.append(line)
    if bad:
        print("check-retired-cmake-names: FAIL — a retired NANO_ROS_* name in the live tree:",
              file=sys.stderr)
        print("\n".join(f"  {b[:160]}" for b in bad), file=sys.stderr)
        print("  Use the NROS_* name (old -> new: cmake/NanoRosRetiredNames.cmake). "
              "A configure that sets an old name now FAILS; a doc naming one teaches "
              "a spelling that no longer works. History may keep it on a line that "
              "says it is retired.", file=sys.stderr)
        return 1
    print(f"check-retired-cmake-names: OK — {len(olds)} retired name(s) and "
          f"{len(RETIRED_PREFIXES)} retired prefix(es) appear only in the error table, "
          f"history, and lines marking them retired")
    return 0


if __name__ == "__main__":
    sys.exit(main())

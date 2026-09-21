#!/usr/bin/env python3
"""issue 1429 -- a `get_property()` result tested with an UNQUOTED `STREQUAL`.

`get_property()` on a property that was never set leaves the variable
**UNDEFINED**, not empty. CMake's `if()` dereferences an unquoted argument only
when a variable of that name is DEFINED, and otherwise compares the TOKEN
ITSELF. So

    get_property(_tl GLOBAL PROPERTY NROS_ENTITY_TL_PUBLISHERS_MAX)
    if(NOT _tl STREQUAL "")                  # "_tl" != "" -> ALWAYS TRUE
        list(APPEND _env "NROS_DECLARED_TL_PUBLISHERS=${_tl}")

asks whether the string `_tl` differs from the empty string, which is always
true, and the guard written to SUPPRESS the row is exactly what appends it --
with `${_tl}` expanding to nothing.

That is issue 1429. The empty `NROS_DECLARED_TL_PUBLISHERS=` reached
`nros-zpico-build`, whose reader is deliberately three-valued (a count,
`refused`, or absent) and panics on a fourth, because `Some("")` is not `None`.
Three of six copy-out templates could not be built by a user, and tier 1 could
not build its workspace fixtures.

**The fix is to QUOTE the value**, `if(NOT "${_tl}" STREQUAL "")`: a quoted
argument is always a string, so an undefined variable expands to `""` and
compares equal.

Scope, deliberately narrow: only variables THIS FILE fills with
`get_property()`. An unquoted `STREQUAL` on a variable from `set()`,
`string()`, `file(READ)` or `list(GET)` is defined-though-possibly-empty and
the idiom is safe there, so flagging it would be noise -- the reach-wider-than-
the-rule shape that makes a gate's output unreadable (issue 0196's mirror).

Self-test is a NEGATIVE CONTROL on the normal path (issue 1167 -- a guard that
exists is not a guard that fires).
"""

from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

SKIP_PARTS = {"build", "target", "third-party", ".git", ".claude", "node_modules"}

GET_PROPERTY = re.compile(r"get_property\(\s*([A-Za-z_][A-Za-z0-9_]*)\s")
# An `if`/`elseif` comparing a BARE identifier against "" — the unsafe spelling.
# `"${x}" STREQUAL ""` does not match, because the argument is not a bare name.
UNQUOTED = re.compile(r"(?:elseif|if)\s*\(.*?(?<![\"$\{])\b([A-Za-z_][A-Za-z0-9_]*)\s+STREQUAL\s+\"\"")


def cmake_sources() -> list[Path]:
    """Every TRACKED cmake file.

    Tracked, not globbed: a provisioned SDK (esp-idf under `esp-idf-workspace/`
    and `external/`) carries this same idiom in upstream code that is not ours
    to change, and a gate that reports it is a gate nobody reads. `git ls-files`
    is also self-maintaining — a new vendored tree needs no skip entry here.
    """
    import subprocess

    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z", "*.cmake", "CMakeLists.txt",
         "**/CMakeLists.txt"],
        capture_output=True, text=True, check=True,
    ).stdout
    paths = [REPO / f for f in out.split("\0") if f]
    return sorted(p for p in paths if p.is_file()
                  and not (SKIP_PARTS & set(p.relative_to(REPO).parts)))


def scan(path: Path, text: str) -> list[str]:
    filled = set(GET_PROPERTY.findall(text))
    if not filled:
        return []
    found: list[str] = []
    for i, line in enumerate(text.splitlines(), 1):
        for m in UNQUOTED.finditer(line):
            name = m.group(1)
            if name not in filled:
                continue
            try:
                shown = path.relative_to(REPO)
            except ValueError:
                shown = path
            found.append(
                f"{shown}:{i}: `{name}` is filled by `get_property()` and tested "
                f'with an UNQUOTED `STREQUAL ""`.\n'
                f"    {line.strip()}\n"
                f'    An unset property leaves `{name}` UNDEFINED, so CMake compares the '
                f'TOKEN `{name}` against "" — always true — and the guard fires when it '
                f'should not (issue 1429).\n'
                f'    Write: NOT "${{{name}}}" STREQUAL ""'
            )
    return found


def self_test() -> None:
    bad = (
        'get_property(_tl GLOBAL PROPERTY SOME_PROP)\n'
        'if(NOT _tl STREQUAL "")\n'
        '    list(APPEND _env "X=${_tl}")\n'
        'endif()\n'
    )
    good = (
        'get_property(_tl GLOBAL PROPERTY SOME_PROP)\n'
        'if(NOT "${_tl}" STREQUAL "")\n'
        '    list(APPEND _env "X=${_tl}")\n'
        'endif()\n'
    )
    unrelated = 'set(_x "")\nif(NOT _x STREQUAL "")\n    message(hi)\nendif()\n'
    with tempfile.TemporaryDirectory() as td:
        v = Path(td) / "victim.cmake"
        v.write_text(bad)
        assert scan(v, bad), (
            "NEGATIVE CONTROL FAILED: the unquoted `get_property` guard that IS "
            "issue 1429 was reported clean"
        )
        assert not scan(v, good), "the quoted spelling must pass"
        assert not scan(v, unrelated), (
            "a `set()` variable is defined-though-empty and the idiom is safe "
            "there; flagging it is noise"
        )


def main() -> int:
    self_test()
    findings: list[str] = []
    files = cmake_sources()
    for p in files:
        findings += scan(p, p.read_text(errors="ignore"))
    if findings:
        print("check-cmake-get-property-guards: FAIL\n")
        for f in findings:
            print(f"  {f}\n")
        return 1
    print(
        f"check-cmake-get-property-guards: OK "
        f"({len(files)} cmake file(s); every `get_property()` value tested "
        f'against "" is quoted)'
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

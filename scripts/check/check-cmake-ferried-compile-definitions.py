#!/usr/bin/env python3
"""check-cmake-ferried-compile-definitions — a target's COMPILE_DEFINITIONS are
read in ONE place.

Issue 1908. A board that rebuilds a target's sources OUTSIDE CMake (the NuttX
boards hand them to the cargo cc-rs build of `nros-nuttx-ffi`) must pass on
every definition CMake would have used. Both NuttX boards read the TARGET's
`COMPILE_DEFINITIONS` and nothing else, while phase-499 W5 set its
`main=nros_ported_main` rename on each SOURCE. So every NuttX C/C++ image linked
an `nros_app_main` calling an `nros_ported_main` its own `main.c` had never been
renamed to: `undefined reference to 'nros_ported_main'`, on qemu-armv7a and
rv-virt, in a family no merge lane builds.

The fix is one helper, `nros_target_ferried_compile_definitions` in
`cmake/NanoRosVerbs.cmake`, which unions the target's definitions with every
source's. This gate keeps it the only reader: a raw read of a target's
`COMPILE_DEFINITIONS` anywhere else is the shape that dropped the rename, and
the next board ported to an out-of-CMake build would copy it.

Refused, outside that function's body:
  * `get_target_property(<var> <target> COMPILE_DEFINITIONS)`
  * `get_property(<var> TARGET <target> PROPERTY COMPILE_DEFINITIONS)`
  * `$<TARGET_PROPERTY:<target>,COMPILE_DEFINITIONS>`

Not refused: `INTERFACE_COMPILE_DEFINITIONS` (usage requirements a consumer
inherits, a different fact) and a SOURCE's `COMPILE_DEFINITIONS`.

Scope: tracked `*.cmake` / `CMakeLists.txt` outside `third-party/`. Buildless;
self-tests its scanner on every run.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HELPER = "nros_target_ferried_compile_definitions"

CALL = re.compile(r"\b(get_target_property|get_property)\s*\(([^)]*)\)", re.I | re.S)
GENEX = re.compile(r"\$<TARGET_PROPERTY:[^,>]+,COMPILE_DEFINITIONS>")
FUNC = re.compile(r"^\s*function\s*\(\s*([A-Za-z0-9_]+)", re.I)
ENDFUNC = re.compile(r"^\s*endfunction\s*\(", re.I)


def strip_comments(text: str) -> str:
    """Drop `#` comments, keeping line numbers. Not quote-aware; a `#` inside a
    string in these files does not precede a property read."""
    return "\n".join(line.split("#", 1)[0] for line in text.splitlines())


def helper_lines(text: str) -> set[int]:
    """Line numbers inside the helper's own body."""
    inside, lines = False, set()
    for n, line in enumerate(text.splitlines(), 1):
        m = FUNC.match(line)
        if m:
            inside = m.group(1) == HELPER
        elif ENDFUNC.match(line):
            inside = False
        elif inside:
            lines.add(n)
    return lines


def scan(text: str) -> list[int]:
    code = strip_comments(text)
    own = helper_lines(code)
    hits = []
    for m in CALL.finditer(code):
        args = m.group(2).split()
        kind = m.group(1).lower()
        if kind == "get_target_property":
            raw = len(args) >= 3 and args[2] == "COMPILE_DEFINITIONS"
        else:
            up = [a.upper() for a in args]
            raw = ("TARGET" in up and "PROPERTY" in up
                   and up.index("PROPERTY") + 1 < len(args)
                   and args[up.index("PROPERTY") + 1] == "COMPILE_DEFINITIONS")
        if raw:
            hits.append(code.count("\n", 0, m.start()) + 1)
    for m in GENEX.finditer(code):
        hits.append(code.count("\n", 0, m.start()) + 1)
    return sorted(n for n in hits if n not in own)


def self_test() -> None:
    cases = [
        ("get_target_property(_d ${target} COMPILE_DEFINITIONS)\n", [1]),
        ("get_target_property(_d\n    ${target}\n    COMPILE_DEFINITIONS)\n", [1]),
        ("get_property(_d TARGET app PROPERTY COMPILE_DEFINITIONS)\n", [1]),
        ('set(x "$<TARGET_PROPERTY:app,COMPILE_DEFINITIONS>")\n', [1]),
        # Not a target's own definitions.
        ("get_target_property(_d app INTERFACE_COMPILE_DEFINITIONS)\n", []),
        ('set(x "$<TARGET_PROPERTY:app,INTERFACE_COMPILE_DEFINITIONS>")\n', []),
        ("get_property(_d SOURCE main.c PROPERTY COMPILE_DEFINITIONS)\n", []),
        ("# get_target_property(_d app COMPILE_DEFINITIONS)\n", []),
        # The helper itself is the one sanctioned reader.
        (f"function({HELPER} target out)\n"
         "    get_target_property(_d ${target} COMPILE_DEFINITIONS)\n"
         "endfunction()\n", []),
        ("function(other target)\n"
         "    get_target_property(_d ${target} COMPILE_DEFINITIONS)\n"
         "endfunction()\n", [2]),
    ]
    for text, want in cases:
        got = scan(text)
        if got != want:
            sys.exit(f"check-cmake-ferried-compile-definitions self-test FAILED: "
                     f"{text!r} -> {got}, want {want}")


def main() -> int:
    self_test()
    files = [f for f in subprocess.run(
        ["git", "ls-files", "-z", "--", "*.cmake", "*CMakeLists.txt", ":!third-party"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split("\0") if f]
    assert files, "git ls-files named no cmake file — the gate would pass on nothing"
    bad = []
    for rel in files:
        try:
            text = (ROOT / rel).read_text(errors="replace")
        except OSError:
            continue
        if "COMPILE_DEFINITIONS" not in text:
            continue
        bad += [f"{rel}:{n}" for n in scan(text)]
    if bad:
        print("check-cmake-ferried-compile-definitions: a target's COMPILE_DEFINITIONS "
              "read outside the helper:")
        for b in bad:
            print(f"  {b}")
        print(f"  Call {HELPER}(<target> <out_var>) (cmake/NanoRosVerbs.cmake): it adds "
              "every SOURCE's definitions, which a target read drops (issue 1908).")
        return 1
    print(f"check-cmake-ferried-compile-definitions: OK ({len(files)} cmake file(s), "
          "self-test OK)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

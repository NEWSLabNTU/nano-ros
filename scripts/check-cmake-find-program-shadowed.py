#!/usr/bin/env python3
"""`find_program(VAR ...)` is a NO-OP when `VAR` is already a defined variable.

Issue 0726. CMake's `find_*` commands short-circuit on an already-set result
variable — that is the caching contract, and an EMPTY STRING counts as set. So
the natural-looking "initialise, then search" spelling never searches:

    set(_NROS_CLI "")            # normal variable, now DEFINED
    find_program(_NROS_CLI nros) # <-- does nothing; _NROS_CLI stays ""

Demonstrated, not inferred:

    -- after find_program: []                                      # pre-set
    -- no pre-set:         [/…/packages/cli/target/release/nros]   # same call

## Why this is worth a gate rather than three fixes

All three sites in this tree paired the dead search with a QUIET FALLBACK, so
none of them failed loudly:

* `cmake/NanoRosCorrosion.cmake` — fell through to the FetchContent branch,
  cloning Corrosion from GitHub at configure time while a provisioned copy sat
  in the SDK store. 116 build dirs in the tree had `_deps/corrosion-src`. It
  also made the configure REQUIRE the network, and made issue 0500's whole
  store-ordering apparatus dead code on that path.
* `cmake/toolchain/riscv64-threadx.cmake` — never asked the store for
  libstdc++, silently taking the "no SDK libstdc++" fallback. That file's own
  comment predicted this: "the failure is quiet by design … which is precisely
  why it could sit unnoticed."
* `zephyr/cmake/nros_rmw_cyclonedds.cmake` — skipped the PATH rung and fell to
  the host-idlc search, reaching (per its own comment) a FATAL_ERROR "advising
  three remedies that were all already satisfied".

A wrong spelling that is silent in all three instances is one that comes back.

## What is flagged

A plain `set(VAR …)` — no `CACHE`, no `PARENT_SCOPE` — followed within the same
file by `find_program|find_path|find_file|find_library(VAR …)`, with no
intervening `unset(VAR)`. That is exactly the shape that cannot work.

Correct spellings, none of which this gate flags:

    find_program(VAR nros)                  # no pre-set at all
    unset(VAR)
    find_program(VAR nros)                  # explicitly cleared first
    find_program(VAR_FOUND nros)            # distinct result variable
    if(VAR_FOUND)
      set(VAR "${VAR_FOUND}")
    endif()

Best of all, where the module system allows it: call the shared resolver
`nros_resolve_cli(<out> OPTIONAL CONTEXT "…")` (issues 0219 / 0325), which owns
the precedence and the stale-path drop. It is not reachable from a toolchain
file — those are re-evaluated per `try_compile`, so an include there is paid
once per probe — which is why the distinct-name form above stays legal.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

FIND = re.compile(
    r"^\s*(find_program|find_path|find_file|find_library)\s*\(\s*"
    r"([A-Za-z_][A-Za-z0-9_]*)"
)
# A plain assignment: `set(VAR)` or `set(VAR "")` or `set(VAR value)`. The
# CACHE / PARENT_SCOPE forms are excluded on the line itself, below.
SETV = re.compile(r"^\s*set\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\b")
UNSET = re.compile(r"^\s*unset\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\b")


def cmake_files() -> list[Path]:
    """Tracked cmake sources, via the git index (issue 0721 — never a walk)."""
    out = subprocess.run(
        ["git", "ls-files", "-z", "*.cmake", "*CMakeLists.txt"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout
    return [ROOT / p for p in out.split("\0") if p]


def scan(text: str) -> list[tuple[int, int, str, str]]:
    """Return (set_line, find_line, var, command) for each dead search."""
    lines = text.split("\n")
    # Last plain `set` of each var, invalidated by a later `unset`.
    pending: dict[str, int] = {}
    hits: list[tuple[int, int, str, str]] = []
    for i, line in enumerate(lines):
        stripped = line.lstrip()
        if stripped.startswith("#"):
            continue

        m = FIND.match(line)
        if m:
            var = m.group(2)
            if var in pending:
                hits.append((pending[var] + 1, i + 1, var, m.group(1)))
            # After a find_*, the var holds a cache result; a later plain set
            # would have to be flagged on its own merits, so clear either way.
            pending.pop(var, None)
            continue

        m = UNSET.match(line)
        if m:
            pending.pop(m.group(1), None)
            continue

        m = SETV.match(line)
        if m:
            if "CACHE" in line or "PARENT_SCOPE" in line:
                pending.pop(m.group(1), None)
            else:
                pending[m.group(1)] = i
    return hits


SELF_TESTS: list[tuple[str, str, int]] = [
    ("the bug, empty pre-set", 'set(V "")\nfind_program(V nros)\n', 1),
    ("the bug, valued pre-set", 'set(V "/a")\nfind_program(V nros)\n', 1),
    ("the bug, bare set", "set(V)\nfind_program(V nros)\n", 1),
    ("no pre-set", "find_program(V nros)\n", 0),
    ("unset first", 'set(V "")\nunset(V)\nfind_program(V nros)\n', 0),
    ("distinct result var", 'set(V "")\nfind_program(V_FOUND nros)\n', 0),
    ("CACHE set is not a shadow", 'set(V "" CACHE INTERNAL "d")\nfind_program(V nros)\n', 0),
    ("PARENT_SCOPE set is not a shadow", 'set(V "" PARENT_SCOPE)\nfind_program(V nros)\n', 0),
    ("commented-out set", '# set(V "")\nfind_program(V nros)\n', 0),
    ("other find_ commands too", 'set(V "")\nfind_library(V m)\n', 1),
    ("set AFTER the find is not this bug", 'find_program(V nros)\nset(V "")\n', 0),
    ("intervening lines still count", 'set(V "")\nif(X)\nendif()\nfind_program(V nros)\n', 1),
]


def self_test() -> int:
    bad = 0
    for name, src, want in SELF_TESTS:
        got = len(scan(src))
        if got != want:
            print(f"  SELF-TEST FAIL: {name}: expected {want} hit(s), got {got}")
            bad += 1
    if bad:
        print(f"check-cmake-find-program-shadowed: {bad} self-test(s) failed")
        return 1
    print(f"check-cmake-find-program-shadowed self-test: {len(SELF_TESTS)} case(s) OK")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    findings = []
    for path in cmake_files():
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        for set_line, find_line, var, cmd in scan(text):
            findings.append((path.relative_to(ROOT), set_line, find_line, var, cmd))

    if findings:
        print("check-cmake-find-program-shadowed: dead search(es) found\n")
        for rel, set_line, find_line, var, cmd in findings:
            print(f"  {rel}:{find_line}: {cmd}({var} ...) can never run —")
            print(f"      `{var}` was already defined at {rel}:{set_line}, and")
            print(f"      find_* does nothing when its result variable is set.")
            print(f"      Use a distinct result variable, `unset({var})` first,")
            print(f"      or the shared `nros_resolve_cli(<out> OPTIONAL ...)`.\n")
        return 1

    print("check-cmake-find-program-shadowed OK — no find_* shadowed by a prior set.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

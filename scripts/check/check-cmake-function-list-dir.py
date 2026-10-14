#!/usr/bin/env python3
"""check-cmake-function-list-dir — no `CMAKE_CURRENT_LIST_DIR` in a function body.

Inside `function()` … `endfunction()`, `CMAKE_CURRENT_LIST_DIR` is the directory
of the file being processed when the function is CALLED, not the file that
defines it. So `${CMAKE_CURRENT_LIST_DIR}/../../cmake/X.cmake` written in
`zephyr/cmake/nros_rmw_cyclonedds.cmake` resolved against `zephyr/` when
`zephyr/CMakeLists.txt` called the function. That took down every Zephyr
CycloneDDS image configure (`include could not find requested file`), and
two older sites (the bare-metal board module, the ThreadX strip script) held
the same latent defect. CMake 3.17+ names the right directory:
`CMAKE_CURRENT_FUNCTION_LIST_DIR`, and the tree requires 3.22.

Scope: tracked `*.cmake` / `CMakeLists.txt` outside `third-party/`. Macros are
NOT checked — a macro body runs in the caller's scope by design, so there the
caller's directory is sometimes the intent. Buildless, ~0.2 s; self-tests its
scanner.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FUNC = re.compile(r"^\s*function\s*\(", re.I)
ENDFUNC = re.compile(r"^\s*endfunction\s*\(", re.I)
NEEDLE = re.compile(r"\bCMAKE_CURRENT_LIST_DIR\b")


def scan(text: str) -> list[int]:
    hits, depth = [], 0
    for n, line in enumerate(text.splitlines(), 1):
        code = line.split("#", 1)[0]
        if FUNC.match(code):
            depth += 1
        elif ENDFUNC.match(code):
            depth = max(0, depth - 1)
        elif depth and NEEDLE.search(code):
            hits.append(n)
    return hits


def self_test() -> None:
    bad = 'function(f)\n  include("${CMAKE_CURRENT_LIST_DIR}/x.cmake")\nendfunction()\n'
    ok_file = 'include("${CMAKE_CURRENT_LIST_DIR}/x.cmake")\nfunction(f)\nendfunction()\n'
    ok_fn = 'function(f)\n  include("${CMAKE_CURRENT_FUNCTION_LIST_DIR}/x.cmake")\nendfunction()\n'
    comment = 'function(f)\n  # CMAKE_CURRENT_LIST_DIR is the caller here\nendfunction()\n'
    macro = 'macro(m)\n  set(d "${CMAKE_CURRENT_LIST_DIR}")\nendmacro()\n'
    cases = [(bad, [2]), (ok_file, []), (ok_fn, []), (comment, []), (macro, [])]
    for text, want in cases:
        got = scan(text)
        if got != want:
            sys.exit(f"check-cmake-function-list-dir self-test FAILED: {text!r} -> {got}, want {want}")


def main() -> int:
    self_test()
    files = subprocess.run(
        ["git", "ls-files", "-z", "--", "*.cmake", "*CMakeLists.txt", ":!third-party"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split("\0")
    bad = []
    for rel in filter(None, files):
        try:
            text = (ROOT / rel).read_text(errors="replace")
        except OSError:
            continue
        bad += [f"{rel}:{n}" for n in scan(text)]
    if bad:
        print("check-cmake-function-list-dir: CMAKE_CURRENT_LIST_DIR inside a function body "
              "is the CALLER's directory:")
        for b in bad:
            print(f"  {b}")
        print("  Use CMAKE_CURRENT_FUNCTION_LIST_DIR (the file the function is defined in).")
        return 1
    print(f"check-cmake-function-list-dir: OK ({len([f for f in files if f])} cmake file(s), self-test OK)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

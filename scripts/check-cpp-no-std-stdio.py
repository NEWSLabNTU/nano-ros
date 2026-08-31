#!/usr/bin/env python3
"""No `std::`-qualified stdio in a cross-compiled LIBRARY translation unit.

Issue 0942. `nros-rmw-cyclonedds/src/graph.cpp` called `std::fprintf` behind an
`NROS_GRAPH_DUMP` env gate. It compiled on every hosted target and on every
embedded target anyone had built recently, and failed only on
threadx-riscv64-cyclonedds:

    graph.cpp:234:14: error: 'fprintf' is not a member of 'std'

## Why the `std::` spelling is the whole bug

`<cstdio>` is required to declare the C names in namespace `std`. Whether it
ALSO puts them in the global namespace is explicitly unspecified — and the
freestanding libstdc++ shipped with the riscv64 cross toolchain does the
reverse: the C library's `<stdio.h>` provides `::fprintf`, and nothing hoists it
into `std`. So `std::fprintf` is not a portability-neutral style choice. It is
the one spelling that depends on a guarantee a freestanding C++ library does not
have to make, which is why this survived every hosted build.

`#include <stdio.h>` plus unqualified `fprintf` works everywhere, hosted and
freestanding alike, and is what the sibling TU in the same crate
(`descriptors.cpp`) already did. The fix was to match it.

## Why this width and not the repo

68 tracked C/C++ files use `std::printf`/`std::fprintf`. Almost all are host-only
— `examples/native/cpp/**`, `tests/**` — where the guarantee holds and the
spelling is harmless. Banning it there would be ~60 files of churn to prevent
nothing, and would teach people to write exemptions.

What matters is the LIBRARY code linked into every embedded image: the RMW
backends, the C/C++ API, the core, and the board C shims. Those TUs are compiled
for whatever freestanding toolchain a board brings, so they must not depend on an
unspecified guarantee. Under this rule the tree is at ZERO after the graph.cpp
fix, so the gate is a tripwire rather than a cleanup.

`tests/` under those same crates is excluded deliberately: it is host-only and
already uses the `std::` spelling in ~10 files.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# Library trees compiled for freestanding targets. Globs are matched against the
# repo-relative path of every TRACKED C/C++ source.
LIBRARY_GLOBS = (
    "packages/rmw/*/*/src/**",
    "packages/api/*/src/**",
    "packages/core/*/src/**",
    "packages/boards/*/c/**",
    "packages/boards/*/cpp/**",
    "packages/drivers/*/*/src/**",
)

SUFFIXES = {".c", ".cc", ".cpp", ".h", ".hpp"}

# `<cstdio>` names that a freestanding libstdc++ is not obliged to put in `std`.
STDIO = (
    "printf", "fprintf", "vprintf", "vfprintf", "sprintf", "snprintf",
    "puts", "fputs", "putchar", "fputc", "fwrite", "perror",
)
PATTERN = re.compile(r"\bstd::(" + "|".join(STDIO) + r")\s*\(")

EXEMPT = "nros-allow-std-stdio"


def tracked_sources():
    out = subprocess.run(
        ["git", "ls-files", "-z", *[g.replace("/**", "/*") for g in ()]],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout
    for name in out.split("\0"):
        if name and Path(name).suffix in SUFFIXES:
            yield name


def in_library_tree(rel: str) -> bool:
    p = Path(rel)
    if "/tests/" in f"/{rel}" or "/test/" in f"/{rel}":
        return False
    return any(p.match(g) or p.match(g.replace("/**", "/*/*")) or p.match(g.replace("/**", "/*"))
               for g in LIBRARY_GLOBS)


def violations_in(text: str):
    lines = text.splitlines()
    for i, line in enumerate(lines, start=1):
        if not PATTERN.search(line):
            continue
        # A line-above exemption, same convention as check-no-std-stdio.py.
        if i >= 2 and EXEMPT in lines[i - 2]:
            continue
        if EXEMPT in line:
            continue
        yield i, line.strip()


def check() -> int:
    bad = []
    for rel in tracked_sources():
        if not in_library_tree(rel):
            continue
        try:
            text = (REPO / rel).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for lineno, line in violations_in(text):
            bad.append((rel, lineno, line))

    if not bad:
        print("check-cpp-no-std-stdio: OK (no `std::`-qualified stdio in a "
              "cross-compiled library TU)")
        return 0

    print("check-cpp-no-std-stdio: FAIL\n")
    for rel, lineno, line in bad:
        print(f"  {rel}:{lineno}: {line}")
    print(
        "\n`<cstdio>` must declare these in `std`; whether it ALSO declares them\n"
        "globally is unspecified, and a freestanding libstdc++ does the reverse —\n"
        "there `std::fprintf` does not exist. A TU compiled for a board's own\n"
        "toolchain cannot depend on that guarantee (issue 0942: this built on every\n"
        "hosted target and failed only on threadx-riscv64).\n\n"
        "Write `#include <stdio.h>` and call it unqualified, as `descriptors.cpp`\n"
        "does. If a file is genuinely host-only, put it under `tests/` or mark the\n"
        f"line with `{EXEMPT}` and say why."
    )
    return 1


def self_test() -> int:
    ok = True

    def expect(name, got, want):
        nonlocal ok
        if got != want:
            ok = False
            print(f"  self-test FAIL {name}: got {got!r} want {want!r}")

    expect("flags qualified call", list(violations_in("  std::fprintf(stderr, x);")),
           [(1, "std::fprintf(stderr, x);")])
    expect("ignores unqualified", list(violations_in("  fprintf(stderr, x);")), [])
    expect("ignores prose", list(violations_in("// std::fprintf is not portable")), [])
    expect("honours exemption above",
           list(violations_in(f"// {EXEMPT}: host-only\nstd::printf(x);")), [])
    expect("library tree matches",
           in_library_tree("packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/graph.cpp"), True)
    expect("crate tests excluded",
           in_library_tree("packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/x.cpp"), False)
    expect("examples excluded",
           in_library_tree("examples/native/cpp/parameters/src/main.cpp"), False)

    print("check-cpp-no-std-stdio --self-test: OK" if ok else "check-cpp-no-std-stdio --self-test: FAILED")
    return 0 if ok else 1


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    sys.exit(check())

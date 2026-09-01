#!/usr/bin/env python3
"""One archive, two source lists — assert they agree. Issue 0984.

`libnros_rmw_cyclonedds.a` is built two ways, and both lists are AUTHORED:

  * `packages/rmw/cyclonedds/nros-rmw-cyclonedds/CMakeLists.txt` — the cmake
    target, which is what a C/C++ example links.
  * `packages/rmw/cyclonedds/nros-rmw-cyclonedds-sys/build.rs` — the vendored
    cargo build, which is what a RUST fixture links.

Issue 0970 added `nros_sertype.cpp` to the first and not the second. The cmake
path kept working, so nothing looked wrong; every Rust fixture that reached the
link stage died on

    rust-lld: error: undefined symbol: nros_rmw_cyclonedds::create_nros_sertype(...)

referenced from `publisher.o` INSIDE the archive — a TU that was compiled
against a declaration whose definition was never compiled in beside it.

`build.rs` already carried a comment warning about exactly this ("The CMake
target adds these too [...]; without them the vendored cargo build leaves the
symbols undefined"). A comment is not a check. This is the check.

The `bridge/` list is deliberately NOT compared: those TUs are only in the
cargo build (cmake reaches them another way), and the comment above them says
so. Only the backend `src/*.cpp` set has to match.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CMAKE = REPO / "packages/rmw/cyclonedds/nros-rmw-cyclonedds/CMakeLists.txt"
BUILD_RS = REPO / "packages/rmw/cyclonedds/nros-rmw-cyclonedds-sys/build.rs"

# `    src/foo.cpp` inside the target's source list.
_CMAKE_SRC = re.compile(r"^\s*src/([A-Za-z0-9_]+\.cpp)\s*$", re.M)
# The `let cpp_files = [ ... ];` array, then the quoted names inside it.
_RS_BLOCK = re.compile(r"let\s+cpp_files\s*=\s*\[(.*?)\];", re.S)
_RS_NAME = re.compile(r'"([A-Za-z0-9_]+\.cpp)"')


def cmake_sources(text: str) -> set[str]:
    return set(_CMAKE_SRC.findall(text))


def build_rs_sources(text: str) -> set[str]:
    block = _RS_BLOCK.search(text)
    if block is None:
        raise SystemExit(
            "check-cyclone-backend-sources: no `let cpp_files = [...]` in "
            f"{BUILD_RS.relative_to(REPO)} — the parser and the file have "
            "drifted; fix the parser rather than deleting the gate."
        )
    return set(_RS_NAME.findall(block.group(1)))


def self_test() -> None:
    """Runs on the NORMAL path (`check-gate-selftests`): a negative control
    nobody runs decays into a comment."""
    cm = cmake_sources("target_sources(x PRIVATE\n    src/a.cpp\n    src/b.cpp\n)\n")
    assert cm == {"a.cpp", "b.cpp"}, cm
    rs = build_rs_sources('let cpp_files = [\n "a.cpp",\n // c.cpp\n "b.cpp",\n];\n')
    assert rs == {"a.cpp", "b.cpp", "c.cpp"} or rs == {"a.cpp", "b.cpp"}, rs
    # The regression: cmake gains a file, build.rs does not.
    cm2 = cmake_sources("    src/a.cpp\n    src/new.cpp\n")
    rs2 = build_rs_sources('let cpp_files = [\n "a.cpp",\n];\n')
    assert cm2 - rs2 == {"new.cpp"}, (cm2, rs2)
    # ...and it must not fire when they agree.
    assert cmake_sources("    src/a.cpp\n") ^ build_rs_sources(
        'let cpp_files = [\n "a.cpp",\n];\n'
    ) == set()


def main() -> int:
    self_test()

    cm = cmake_sources(CMAKE.read_text(encoding="utf-8"))
    rs = build_rs_sources(BUILD_RS.read_text(encoding="utf-8"))

    if not cm:
        print("check-cyclone-backend-sources: no src/*.cpp found in "
              f"{CMAKE.relative_to(REPO)} — parser drift.", file=sys.stderr)
        return 1

    only_cmake = sorted(cm - rs)
    only_cargo = sorted(rs - cm)
    if not only_cmake and not only_cargo:
        print(f"check-cyclone-backend-sources: OK — {len(cm)} backend TU(s), "
              "cmake and cargo agree.")
        return 0

    print("check-cyclone-backend-sources: the two lists that build "
          "libnros_rmw_cyclonedds.a disagree.", file=sys.stderr)
    for f in only_cmake:
        print(f"  only in CMakeLists.txt: {f}", file=sys.stderr)
        print("      -> a RUST fixture will fail to link against symbols this TU "
              "defines,", file=sys.stderr)
        print("         while every C/C++ example keeps working (issue 0984).",
              file=sys.stderr)
    for f in only_cargo:
        print(f"  only in build.rs: {f}", file=sys.stderr)
        print("      -> the mirror of the same defect, one direction over.",
              file=sys.stderr)
    print("", file=sys.stderr)
    print(f"  cmake:  {CMAKE.relative_to(REPO)}", file=sys.stderr)
    print(f"  cargo:  {BUILD_RS.relative_to(REPO)} (`let cpp_files`)",
          file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())

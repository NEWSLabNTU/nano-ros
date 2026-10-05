#!/usr/bin/env python3
"""Issue 1633 — every road that COMPILES Cyclone DDS routes ddsrt's heap
through the nano-ros platform funnel.

`NROS_DDSRT_PLATFORM_FUNNEL` (issue 0832) is read by one fork TU family,
`src/ddsrt/src/heap/*/heap.c`: defined, `heap/nros/heap.c` is the ddsrt heap
and each port's own heap.c compiles out; undefined, a port's heap (libc
`malloc` on posix) is live. The switch has TWO producers, one per road that
builds Cyclone from source:

  * `packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/ProvideCycloneDDS.cmake`
    (the cmake road: a PRIVATE define on `ddsc`);
  * `zephyr/cmake/nros_rmw_cyclonedds.cmake` (the Zephyr west road, which
    globs Cyclone's sources itself).

Only the first set it until issue 1633, so a Zephyr Cyclone image allocated
from picolibc's arena while every other allocation in it used the nros heap.
This gate holds BOTH producers to the switch, and the Zephyr one to compiling
`heap/nros/heap.c` as well (the cmake road gets that file from Cyclone's own
source list). A third road that compiles Cyclone must be added here.

Run: python3 scripts/check-ddsrt-funnel-producers.py
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
GATE = "check-ddsrt-funnel-producers"
SWITCH = "NROS_DDSRT_PLATFORM_FUNNEL"
NROS_HEAP = "src/ddsrt/src/heap/nros/heap.c"

# producer -> what it must contain beyond the switch
PRODUCERS: dict[str, tuple[str, ...]] = {
    "packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/ProvideCycloneDDS.cmake": (),
    "zephyr/cmake/nros_rmw_cyclonedds.cmake": (NROS_HEAP,),
}


def code_lines(text: str) -> str:
    """The CMake text with `#` comments removed, so prose cannot satisfy it."""
    out = []
    for line in text.splitlines():
        i = line.find("#")
        out.append(line if i < 0 else line[:i])
    return "\n".join(out)


def problems(sources: dict[str, str]) -> list[str]:
    out: list[str] = []
    for rel, extra in PRODUCERS.items():
        text = sources.get(rel)
        if text is None:
            out.append(f"  {rel}: MISSING — a producer this gate names is gone; "
                       f"update PRODUCERS rather than letting it pass on absence")
            continue
        code = code_lines(text)
        if SWITCH not in code:
            out.append(f"  {rel}: compiles Cyclone without `{SWITCH}`, so ddsrt's "
                       f"heap is the port's own (libc malloc on posix), not the "
                       f"platform funnel (issue 1633)")
        for need in extra:
            if need not in code:
                out.append(f"  {rel}: does not compile `{need}`, the funnel TU")
    return out


def self_test() -> None:
    good = {
        "packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/ProvideCycloneDDS.cmake":
            f"target_compile_definitions(ddsc PRIVATE {SWITCH})\n",
        "zephyr/cmake/nros_rmw_cyclonedds.cmake":
            f"set_source_files_properties(x PROPERTIES COMPILE_DEFINITIONS {SWITCH})\n"
            f"zephyr_library_sources(${{D}}/{NROS_HEAP})\n",
    }
    assert not problems(good), problems(good)
    # The issue's own state: the Zephyr road without the switch.
    bad = dict(good)
    bad["zephyr/cmake/nros_rmw_cyclonedds.cmake"] = f"zephyr_library_sources(${{D}}/{NROS_HEAP})\n"
    assert any(SWITCH in p for p in problems(bad)), problems(bad)
    # A mention in a COMMENT is not a define.
    prose = dict(good)
    prose["zephyr/cmake/nros_rmw_cyclonedds.cmake"] = (
        f"# we should set {SWITCH} here\nzephyr_library_sources(${{D}}/{NROS_HEAP})\n")
    assert any(SWITCH in p for p in problems(prose)), problems(prose)
    # The switch without the funnel TU links nothing to the platform.
    no_tu = dict(good)
    no_tu["zephyr/cmake/nros_rmw_cyclonedds.cmake"] = (
        f"set_source_files_properties(x PROPERTIES COMPILE_DEFINITIONS {SWITCH})\n")
    assert any("funnel TU" in p for p in problems(no_tu)), problems(no_tu)
    # A producer that disappeared fails, never passes on absence.
    gone = {k: v for k, v in good.items() if not k.startswith("zephyr/")}
    assert any("MISSING" in p for p in problems(gone)), problems(gone)


def main() -> int:
    self_test()
    sources = {rel: (REPO / rel).read_text(encoding="utf-8")
               for rel in PRODUCERS if (REPO / rel).exists()}
    found = problems(sources)
    if found:
        print(f"{GATE}: FAIL")
        for p in found:
            print(p)
        return 1
    print(f"{GATE}: OK — {len(PRODUCERS)} Cyclone-compiling road(s) route ddsrt's "
          f"heap through the platform funnel (self-test: 5 cases)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

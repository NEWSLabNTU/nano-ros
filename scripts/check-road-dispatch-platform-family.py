#!/usr/bin/env python3
"""Issue 1795 — a build ROAD is chosen per RTOS FAMILY, never per raw spelling.

The just recipes and toolchain files pass the VARIANT spelling of
NROS_PLATFORM (`threadx_riscv64`, `threadx_linux`, `nuttx_armv7a`,
`freertos_armcm3`). `nano_ros_entry()` normalised it inline before choosing; the
three RTOS typed-entry carriers in `NanoRosNodeRegister.cmake` compared the raw
value to the bare family token (`STREQUAL "threadx"`). So on every lane-built
rv-virt-threadx leaf the carrier arm never fired: configure succeeded, no image
was produced, and the `nros-c` staticlib — which no image had claimed a panic
policy for — failed with `#[panic_handler] function required`. Nothing had ever
built a carrier through a lane, so nothing saw it.

R1  In the road dispatchers (NanoRosEntry.cmake, NanoRosNodeRegister.cmake),
    `NROS_PLATFORM` is never compared with a bare family token. Compare the
    family from `nros_platform_family()` instead.
R2  The variant -> family regex lives once, in NanoRosPlatformFamily.cmake: no
    other cmake file under `cmake/` re-spells `^(threadx|freertos|nuttx)_`.

Scope is deliberately the DISPATCHERS: other files compare raw values on
purpose and spell both forms (`freertos_armcm3 OR freertos`), or branch on the
variant itself (the zenoh staticlib's std/no_std ThreadX split).
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
DISPATCHERS = ("cmake/NanoRosEntry.cmake", "cmake/NanoRosNodeRegister.cmake")
HOME = "cmake/NanoRosPlatformFamily.cmake"
FAMILIES = ("threadx", "freertos", "nuttx")

_RAW = re.compile(
    r'\$?\{?(?:NANO_ROS|NROS)_PLATFORM\}?"?\s+STREQUAL\s+"(' + "|".join(FAMILIES) + r')"')
_REGEX = re.compile(r'\^\((?:threadx\|freertos\|nuttx|threadx\|nuttx\|freertos|'
                    r'freertos\|threadx\|nuttx|freertos\|nuttx\|threadx|'
                    r'nuttx\|threadx\|freertos|nuttx\|freertos\|threadx)\)_')


def strip(text: str) -> str:
    return re.sub(r"#[^\n]*", "", text)


def r1(text: str) -> list[tuple[int, str]]:
    body = strip(text)
    return [(body.count("\n", 0, m.start()) + 1, m.group(0)) for m in _RAW.finditer(body)]


def r2(text: str) -> list[int]:
    body = strip(text)
    return [body.count("\n", 0, m.start()) + 1 for m in _REGEX.finditer(body)]


def self_test() -> None:
    bad = 'if(_x AND NROS_PLATFORM STREQUAL "threadx")\nendif()\n'
    assert r1(bad), "a raw family compare must be caught"
    assert r1('if("${NROS_PLATFORM}" STREQUAL "nuttx")\n'), "quoted form too"
    assert not r1('if(_nrc_family STREQUAL "threadx")\n'), "a family compare is the fix"
    assert not r1('if(NROS_PLATFORM STREQUAL "posix")\n'), "posix has no variants"
    assert not r1('# NROS_PLATFORM STREQUAL "threadx" in a comment\n')
    assert r2('if(_p MATCHES "^(threadx|freertos|nuttx)_")\n'), "a second normaliser"
    assert not r2('# "^(threadx|freertos|nuttx)_" in a comment\n')


def main() -> int:
    self_test()
    problems = []
    for p in DISPATCHERS:
        path = REPO / p
        if not path.is_file():
            problems.append(f"{p}: dispatcher missing — update DISPATCHERS")
            continue
        for line, what in r1(path.read_text(encoding="utf-8")):
            problems.append(f"R1 {p}:{line}: `{what}` — compare nros_platform_family(), "
                            "the recipes pass the variant spelling")
    tracked = subprocess.run(["git", "ls-files", "-z", "--", "cmake/*.cmake", "cmake/**/*.cmake"],
                             cwd=REPO, capture_output=True, text=True, check=True).stdout
    files = sorted({f for f in tracked.split("\0") if f})
    if HOME not in files:
        problems.append(f"{HOME}: the one normaliser is missing")
    for p in files:
        if p == HOME:
            continue
        for line in r2((REPO / p).read_text(encoding="utf-8", errors="replace")):
            problems.append(f"R2 {p}:{line}: a second variant->family normaliser — "
                            f"call nros_platform_family() ({HOME})")
    if problems:
        print("check-road-dispatch-platform-family: a build road is chosen on a raw "
              "platform spelling (issue 1795):", file=sys.stderr)
        for w in problems:
            print(f"  {w}", file=sys.stderr)
        return 1
    print(f"check-road-dispatch-platform-family OK — {len(DISPATCHERS)} dispatcher(s) "
          f"compare families; one normaliser across {len(files)} cmake file(s).")
    return 0


if __name__ == "__main__":
    sys.exit(main())

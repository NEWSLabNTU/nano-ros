#!/usr/bin/env python3
"""A FreeRTOS `config*` macro reaches the compiler through FreeRTOSConfig.h ONLY.

Issue 1657. `configUSE_TRACE_FACILITY` changes the layout of `TCB_t` /
`StaticTask_t` and `Queue_t` / `StaticQueue_t`. The cmake road passed it as
`target_compile_definitions(freertos_kernel PUBLIC configUSE_TRACE_FACILITY=1)`,
which reached the kernel and the cmake targets linking it — and not the C that
cargo compiles (zenoh-pico in `zpico-sys`), which reads the SAME
FreeRTOSConfig.h without the `-D`. Two views of one struct: zenoh-pico embedded
a 72-byte `StaticSemaphore_t` that the kernel initialised as 80, the overrun
rewrote the neighbouring mutex's type byte, and every cmake-built zenoh C/C++
FreeRTOS image deadlocked in session open. It stayed latent until issue 1598
turned on `configSUPPORT_STATIC_ALLOCATION` (kernel objects embedded in caller
memory), and stayed hidden for two days more because no merge-gating lane boots
one of those images.

The class is "a kernel config fact with more than one carrier": a build file can
only ever reach SOME of the compilers that include FreeRTOSConfig.h, so any
`config*` macro on a command line is a split waiting for a layout-sensitive
value. The rule is therefore about the carrier, not about this one macro.

Scope: every TRACKED build file — cmake, `CMakeLists.txt`, Rust build scripts and
board build helpers, `just` recipes, shell/python scripts, workflows.

    check-freertos-config-single-carrier.py             # the gate (selftest first)
    check-freertos-config-single-carrier.py --selftest  # controls only
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# A path is ALLOWED only with a reason that says why it cannot split a layout.
ALLOWED = {
    "scripts/check-sched-dim-arms-compile.sh": (
        "a -fsyntax-only probe of the SMP scheduling arms: ONE translation unit, "
        "no image, no second compiler to disagree with"
    ),
    "scripts/check-freertos-config-single-carrier.py": "this gate (its own controls)",
}

PATTERNS = [
    # -DconfigFOO / -DconfigFOO=1 on any command line or flags string
    re.compile(r"-Dconfig[A-Z][A-Z0-9_]*"),
    # cc-rs: build.define("configFOO", ...)
    re.compile(r"\.define\(\s*\"config[A-Z][A-Z0-9_]*\""),
    # cmake: target_compile_definitions / add_compile_definitions / add_definitions
    re.compile(
        r"(?:target_compile_definitions|add_compile_definitions|add_definitions)"
        r"\s*\([^)]*?\bconfig[A-Z][A-Z0-9_]*",
        re.S,
    ),
]


def _is_build_file(path: str) -> bool:
    if path.startswith(("third-party/", "docs/", "book/")):
        return False
    name = path.rsplit("/", 1)[-1]
    return (
        path.endswith((".cmake", ".just", ".sh", ".py", ".yml", ".yaml"))
        or name in ("CMakeLists.txt", "justfile", "build.rs")
        or (path.startswith("packages/boards/") and path.endswith(".rs"))
        or (path.startswith("packages/platform/") and path.endswith(".rs"))
        or (path.startswith("packages/rmw/") and "build" in path and path.endswith(".rs"))
    )


def scan_text(text: str) -> list[tuple[int, str]]:
    hits = []
    for pat in PATTERNS:
        for m in pat.finditer(text):
            line = text.count("\n", 0, m.start()) + 1
            hits.append((line, m.group(0).splitlines()[-1].strip()))
    return sorted(set(hits))


def tracked_build_files() -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z"],
        check=True,
        capture_output=True,
    ).stdout.decode()
    return [p for p in out.split("\0") if p and _is_build_file(p)]


def selftest() -> None:
    bad = [
        "target_compile_definitions(freertos_kernel PUBLIC configUSE_TRACE_FACILITY=1)",
        "target_compile_definitions(k\n    PUBLIC\n    configSUPPORT_STATIC_ALLOCATION=1)",
        'set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} -DconfigUSE_TRACE_FACILITY=1")',
        'build.define("configUSE_TRACE_FACILITY", "1");',
    ]
    good = [
        "target_compile_definitions(freertos_kernel PUBLIC NROS_TRACE=1)",
        "# configUSE_TRACE_FACILITY is stated in FreeRTOSConfig.h",
        'build.define("NROS_TRACE", "1");',
        "-D__int64_t_defined=1",
    ]
    for s in bad:
        if not scan_text(s):
            raise SystemExit(f"selftest: missed a carrier: {s!r}")
    for s in good:
        if scan_text(s):
            raise SystemExit(f"selftest: false positive: {s!r}")


def main(argv: list[str]) -> int:
    selftest()
    if "--selftest" in argv:
        print("check-freertos-config-single-carrier: selftest OK")
        return 0
    problems = []
    files = tracked_build_files()
    for path in files:
        if path in ALLOWED:
            continue
        try:
            text = (ROOT / path).read_text(errors="replace")
        except OSError:
            continue
        for line, what in scan_text(text):
            problems.append(f"  {path}:{line}: {what}")
    if problems:
        print(
            "check-freertos-config-single-carrier: a FreeRTOS `config*` macro is passed\n"
            "by a build file. State it in FreeRTOSConfig.h instead: a `-D` reaches only\n"
            "the compilers that one file drives, and a layout-changing value split that\n"
            "way deadlocked every cmake-built zenoh FreeRTOS image (issue 1657).\n"
            + "\n".join(problems),
            file=sys.stderr,
        )
        return 1
    print(f"check-freertos-config-single-carrier: OK ({len(files)} build files scanned)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

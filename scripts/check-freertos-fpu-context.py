#!/usr/bin/env python3
"""A FreeRTOS board on an FPU-optional port gives every task an FPU context — issue 0997.

The A/R-profile FreeRTOS ports (`ARM_CRx_No_GIC`, `ARM_CR5`, `ARM_CA9`,
`ARM_AARCH64`, ...) default `configUSE_TASK_FPU_SUPPORT` to 1: a task is created
WITHOUT a floating-point context and its VFP/NEON registers are saved across a
context switch only after it calls `vPortTaskUsesFPU()`. Nothing in this tree
calls it, and nothing could call it reliably: these boards compile every object
`-mfloat-abi=hard`, so GCC uses the d-registers in code with no floating point
in it. On mps3-an536 that let tasks clobber each other's `d8`, Cyclone stamped
its events `tsched = 0` instead of `DDS_NEVER`, the periodic SPDP/PMD events
were never rescheduled, and the participant went silent ~100 ms after
discovery (issue 0997, three weeks of hunting in the wrong layer).

WHAT IT REQUIRES, for every `cmake/board/*.cmake` that selects such a port with
`nros_freertos_board_port(<board> "GCC/<port>")`:

  context   the board's `packages/boards/<board>/config/FreeRTOSConfig.h`
            defines `configUSE_TASK_FPU_SUPPORT 2`;
  d32       the board file states `configFPU_D32` through
            `nros_freertos_config_define(configFPU_D32=<0|1>)` — a compile
            definition, because `portASM.S` saves the registers and never
            includes `FreeRTOSConfig.h`;
  split     `FreeRTOSConfig.h` does NOT define `configFPU_D32`: there it would
            reach `port.c` (which sizes the task frame) and not the assembly
            (which fills it), and the first switch into a new task would
            restore a misaligned frame.

Usage::

    check-freertos-fpu-context.py [--selftest] [--list]
"""

from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Ports whose task FPU context is opt-in (`configUSE_TASK_FPU_SUPPORT`).
# Cortex-M ports with an FPU (ARM_CM4F, ARM_CM7, ...) stack it lazily in
# hardware and are not in this set. A board names its port by upstream's
# `FREERTOS_PORT` enum (`GCC_ARM_CRX_NOGIC`, phase-349 W3); the `portable/`
# path spelling (`GCC/ARM_CRx_No_GIC`) is still accepted, so both match.
FPU_OPTIONAL_PORTS = re.compile(
    r"GCC/(ARM_CRx\w*|ARM_CR5|ARM_CA9|ARM_CA53\w*|ARM_AARCH64\w*)$"
    r"|^GCC_(ARM_CRX\w*|ARM_CR5|ARM_CA9|ARM_CA53\w*|ARM_AARCH64\w*)$"
)

BOARD_PORT = re.compile(r'nros_freertos_board_port\(\s*(nano-ros-board-[\w-]+)\s+"([^"]+)"')
D32_DEFINE = re.compile(r"nros_freertos_config_define\([^)]*\bconfigFPU_D32=[01]\b")
FPU_SUPPORT_2 = re.compile(r"^\s*#\s*define\s+configUSE_TASK_FPU_SUPPORT\s+2\b", re.M)
D32_IN_HEADER = re.compile(r"^\s*#\s*define\s+configFPU_D32\b", re.M)


def strip_cmake_comments(text: str) -> str:
    return "\n".join(line.split("#", 1)[0] for line in text.splitlines())


def check(root: str) -> tuple[list[str], list[str]]:
    """Return (boards examined, problems)."""
    examined: list[str] = []
    problems: list[str] = []
    board_dir = os.path.join(root, "cmake", "board")
    for name in sorted(os.listdir(board_dir)) if os.path.isdir(board_dir) else []:
        if not name.endswith(".cmake"):
            continue
        path = os.path.join(board_dir, name)
        with open(path, encoding="utf-8") as fh:
            body = strip_cmake_comments(fh.read())
        for target, port in BOARD_PORT.findall(body):
            if not FPU_OPTIONAL_PORTS.search(port):
                continue
            board = target[len("nano-ros-"):]  # nano-ros-board-x -> board-x
            crate = "nros-" + board
            examined.append(f"{crate} ({port})")
            rel_cmake = os.path.relpath(path, root)
            header = os.path.join(root, "packages", "boards", crate, "config", "FreeRTOSConfig.h")
            rel_header = os.path.relpath(header, root)
            if not os.path.isfile(header):
                problems.append(f"{rel_cmake}: selects {port} but {rel_header} does not exist")
                continue
            with open(header, encoding="utf-8") as fh:
                htext = fh.read()
            if not FPU_SUPPORT_2.search(htext):
                problems.append(
                    f"{rel_header}: {port} needs `#define configUSE_TASK_FPU_SUPPORT 2` — "
                    "without it no task has an FPU context and hard-float code "
                    "clobbers other tasks' d-registers (issue 0997)"
                )
            if D32_IN_HEADER.search(htext):
                problems.append(
                    f"{rel_header}: defines configFPU_D32, which portASM.S never sees "
                    "(it does not include FreeRTOSConfig.h) — state it in "
                    f"{rel_cmake} with nros_freertos_config_define(configFPU_D32=<0|1>)"
                )
            if not D32_DEFINE.search(body):
                problems.append(
                    f"{rel_cmake}: {port} needs nros_freertos_config_define(configFPU_D32=<0|1>) "
                    "— 1 for a D32 unit (neon-fp-armv8), 0 for a D16 one; the port saves "
                    "d16-d31 only when it is 1"
                )
    return examined, problems


def selftest() -> int:
    """Negative control: a board missing each half must fail, a complete one must pass."""
    good_cmake = 'nros_freertos_board_port(nano-ros-board-x-freertos "GCC/ARM_CRx_No_GIC")\n' \
                 "nros_freertos_config_define(configFPU_D32=1)\n"
    cases = {
        "complete": (good_cmake, "#define configUSE_TASK_FPU_SUPPORT 2\n", 0),
        "no-context": (good_cmake, "/* nothing */\n", 1),
        "no-d32": ('nros_freertos_board_port(nano-ros-board-x-freertos "GCC/ARM_CRx_No_GIC")\n'
                   "# nros_freertos_config_define(configFPU_D32=1)  <- a comment is not a definition\n",
                   "#define configUSE_TASK_FPU_SUPPORT 2\n", 1),
        "d32-in-header": (good_cmake, "#define configUSE_TASK_FPU_SUPPORT 2\n#define configFPU_D32 1\n", 1),
        "cortex-m-ignored": ('nros_freertos_board_port(nano-ros-board-x-freertos "GCC/ARM_CM3")\n',
                             "/* nothing */\n", 0),
        "enum-spelling": ('nros_freertos_board_port(nano-ros-board-x-freertos "GCC_ARM_CRX_NOGIC")\n',
                          "/* nothing */\n", 2),
        "enum-cortex-m-ignored": ('nros_freertos_board_port(nano-ros-board-x-freertos "GCC_ARM_CM3")\n',
                                  "/* nothing */\n", 0),
    }
    failed = 0
    for label, (cmake, header, want) in cases.items():
        with tempfile.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "cmake", "board"))
            os.makedirs(os.path.join(tmp, "packages", "boards", "nros-board-x-freertos", "config"))
            with open(os.path.join(tmp, "cmake", "board", "nano-ros-board-x-freertos.cmake"), "w") as fh:
                fh.write(cmake)
            with open(os.path.join(tmp, "packages", "boards", "nros-board-x-freertos", "config",
                                   "FreeRTOSConfig.h"), "w") as fh:
                fh.write(header)
            _, problems = check(tmp)
            got = len(problems)
            if got != want:
                failed += 1
                print(f"selftest {label}: expected {want} problem(s), got {got}: {problems}")
    if failed:
        print(f"check-freertos-fpu-context selftest FAILED ({failed} case(s))")
        return 1
    print(f"check-freertos-fpu-context selftest OK ({len(cases)} cases)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--selftest", action="store_true", help="run the negative control only")
    ap.add_argument("--list", action="store_true", help="list the boards examined")
    args = ap.parse_args()
    if args.selftest:
        return selftest()
    if selftest() != 0:
        return 1
    examined, problems = check(ROOT)
    if args.list:
        for b in examined:
            print(b)
    if not examined:
        print("check-freertos-fpu-context: no board selects an FPU-optional port — "
              "the board-port pattern may have changed; refusing to pass vacuously")
        return 1
    if problems:
        for p in problems:
            print(f"  {p}")
        print(f"check-freertos-fpu-context FAILED — {len(problems)} problem(s) "
              f"across {len(examined)} board(s)")
        return 1
    print(f"check-freertos-fpu-context OK — {len(examined)} board(s) on an FPU-optional port "
          "give every task an FPU context")
    return 0


if __name__ == "__main__":
    sys.exit(main())

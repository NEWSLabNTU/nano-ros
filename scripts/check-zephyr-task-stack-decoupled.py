#!/usr/bin/env python3
"""nano-ros's Zephyr task stacks are their own budget, never main's (issue 2365).

Every task nano-ros creates on Zephyr (`nros_zephyr_task_create`: on a zenoh
image `zpico_exec` and `zpico_flush`) is sized NROS_ZEPHYR_STACK_SIZE. It used
to default to CONFIG_MAIN_STACK_SIZE, so raising main's stack for a reason that
is only main's multiplied into every task: issue 2359's 16 -> 32 KiB main raise
on nucleo-f767zi grew the image 48 KiB instead of 16 and the C talker overflowed
RAM by 7,872 bytes, which stopped tier 2.

This gate holds the decoupling, in the three places it lives:

  * zephyr/nros_platform_zephyr_shims.c never defines NROS_ZEPHYR_STACK_SIZE
    from CONFIG_MAIN_STACK_SIZE;
  * zephyr/Kconfig's NROS_ZEPHYR_TASK_STACK_SIZE has its own numeric default
    (at least 1024) and is not inside an RMW block, so every RMW gets it;
  * zephyr/CMakeLists.txt forwards it whenever it is defined (no "0 means
    inherit" branch).

Buildless, milliseconds. The self-test runs on the normal path.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SHIM = "zephyr/nros_platform_zephyr_shims.c"
KCONFIG = "zephyr/Kconfig"
CMAKE = "zephyr/CMakeLists.txt"

SHIM_COUPLED = re.compile(r"#\s*define\s+NROS_ZEPHYR_STACK_SIZE\b[^\n]*MAIN_STACK_SIZE")
KNOB = re.compile(r"^config NROS_ZEPHYR_TASK_STACK_SIZE\n(?:(?!^config |^if |^endif).*\n)*?"
                  r"\s+default (\d+)\s*\n", re.M)
FORWARD = re.compile(r"if\(DEFINED CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE\)\s*\n\s*"
                     r"zephyr_compile_definitions\(NROS_ZEPHYR_STACK_SIZE=")
INHERIT_BRANCH = re.compile(r"CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE\s+EQUAL\s+0")


def enclosing_ifs(text: str, pos: int) -> list[str]:
    stack: list[str] = []
    for m in re.finditer(r"^(if (\S+)|endif\b.*)$", text[:pos], re.M):
        if m.group(2):
            stack.append(m.group(2))
        elif stack:
            stack.pop()
    return stack


def check(shim: str, kconfig: str, cmake: str) -> list[str]:
    problems = []
    if SHIM_COUPLED.search(shim):
        problems.append(f"  {SHIM}: NROS_ZEPHYR_STACK_SIZE is defined from "
                        "CONFIG_MAIN_STACK_SIZE again")
    m = KNOB.search(kconfig)
    if not m:
        problems.append(f"  {KCONFIG}: NROS_ZEPHYR_TASK_STACK_SIZE has no numeric default")
    else:
        if int(m.group(1)) < 1024:
            problems.append(f"  {KCONFIG}: NROS_ZEPHYR_TASK_STACK_SIZE defaults to "
                            f"{m.group(1)}; 0 used to mean \"inherit main's\"")
        rmw = [c for c in enclosing_ifs(kconfig, m.start()) if c.startswith("NROS_RMW_")]
        if rmw:
            problems.append(f"  {KCONFIG}: NROS_ZEPHYR_TASK_STACK_SIZE is inside `if "
                            f"{rmw[-1]}`, so other RMWs fall back to the shim default")
    if not FORWARD.search(cmake) or INHERIT_BRANCH.search(cmake):
        problems.append(f"  {CMAKE}: CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE is not forwarded "
                        "unconditionally (an `EQUAL 0` inherit branch is back, or the "
                        "forward is gone)")
    return problems


def self_test() -> None:
    shim, kc, cm = (REPO / SHIM).read_text(), (REPO / KCONFIG).read_text(), (REPO / CMAKE).read_text()
    assert check(shim, kc, cm) == [], check(shim, kc, cm)
    bad = shim.replace("#define NROS_ZEPHYR_STACK_SIZE 16384",
                       "#define NROS_ZEPHYR_STACK_SIZE CONFIG_MAIN_STACK_SIZE")
    assert bad != shim and check(bad, kc, cm), "self-test: re-coupled shim passed"
    m = KNOB.search(kc)
    bad = kc[:m.start(1)] + "0" + kc[m.end(1):]
    assert check(shim, bad, cm), "self-test: a default of 0 passed"
    knob_start = kc.index("config NROS_ZEPHYR_TASK_STACK_SIZE\n")
    bad = kc[:knob_start] + "if NROS_RMW_ZENOH\n" + kc[knob_start:].replace(
        "\nif NROS_RMW_CYCLONEDDS\n", "\nendif # NROS_RMW_ZENOH\n\nif NROS_RMW_CYCLONEDDS\n", 1)
    assert check(shim, bad, cm), "self-test: the knob inside an RMW block passed"
    bad = cm.replace("if(DEFINED CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE)",
                     "if(DEFINED CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE AND NOT "
                     "CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE EQUAL 0)")
    assert bad != cm and check(shim, kc, bad), "self-test: the inherit branch passed"


def main() -> int:
    self_test()
    problems = check((REPO / SHIM).read_text(), (REPO / KCONFIG).read_text(),
                     (REPO / CMAKE).read_text())
    if problems:
        print("check-zephyr-task-stack-decoupled: the zenoh task stacks follow main's "
              "again (issue 2365):", file=sys.stderr)
        print("\n".join(problems), file=sys.stderr)
        return 1
    print("check-zephyr-task-stack-decoupled OK -- task stacks have their own default, "
          "for every RMW, and never read CONFIG_MAIN_STACK_SIZE")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""A Zephyr conf that raises the main stack also states the zenoh task stack.

Issue 2365. On a zenoh image, nano-ros sizes every task it creates
(`zpico_exec`, `zpico_flush`; `NROS_ZEPHYR_STACK_SIZE` in
zephyr/nros_platform_zephyr_shims.c) at CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE, and
when that is unset (0) it inherits CONFIG_MAIN_STACK_SIZE. So raising main's
stack, for a reason that is only main's, multiplies into every zenoh task:
issue 2359 raised the nucleo-f767zi board's main stack 16 -> 32 KiB for the
Rust entry, the two zenoh stacks grew with it, the image grew 48 KiB instead
of 16, and the C talker overflowed RAM by 7,872 bytes, which stopped every
tier-2 run in the fixture build.

The rule: a tracked `*.conf` that sets CONFIG_MAIN_STACK_SIZE above Zephyr's
zenoh default (16384, zephyr/Kconfig `configdefault MAIN_STACK_SIZE`) also
sets CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE to a nonzero value, so the two budgets
are decided separately and visibly. A conf whose file name selects a non-zenoh
RMW (`*cyclonedds*`, `*xrce*`) is exempt: the task-stack symbol is inside
`if NROS_RMW_ZENOH` and has no effect there.

Buildless, milliseconds; reads `git ls-files`. The self-test runs on the
normal path.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
ZENOH_MAIN_DEFAULT = 16384
NON_ZENOH = re.compile(r"cyclonedds|xrce", re.I)
MAIN = re.compile(r"^CONFIG_MAIN_STACK_SIZE=(\d+)\s*$", re.M)
TASK = re.compile(r"^CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE=([1-9]\d*)\s*$", re.M)

# Boards whose large main stack predates issue 2365 and is ACCEPTED to size the
# zenoh tasks too. A board conf merges after a leaf's (last wins), so a board
# line would beat a leaf that states its own task stack, which is why these
# state none. A new entry needs a reason; never add one to make a red pass.
ACCEPTED = {
    "packages/boards/nros-board-mps2-an385-zephyr/prj.conf":
        "4 MB SRAM; 128 KiB main for the construction path, and a leaf "
        "(sched-task-stacks-dtcm-zephyr-mps2) states its own task stack, "
        "which a board line would beat",
    "packages/boards/nros-board-qemu-cortex-a53/prj.conf":
        "aarch64 frames: 32 KiB main measured on this board, with RAM to spare",
}


def check(files: dict[str, str]) -> list[str]:
    problems = []
    for path, text in sorted(files.items()):
        if NON_ZENOH.search(Path(path).name) or path in ACCEPTED:
            continue
        mains = [int(v) for v in MAIN.findall(text)]
        if not mains or max(mains) <= ZENOH_MAIN_DEFAULT:
            continue
        if not TASK.search(text):
            problems.append(
                f"  {path}: CONFIG_MAIN_STACK_SIZE={max(mains)} with no "
                "CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE, so every zenoh task inherits "
                f"{max(mains)} bytes too. State the task stack beside it (issue 2365).")
    return problems


def self_test() -> None:
    raised = "CONFIG_MAIN_STACK_SIZE=32768\n"
    stated = raised + "CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE=16384\n"
    assert check({"b/prj.conf": stated}) == []
    assert check({"b/prj.conf": raised}), "self-test: a raised main stack alone passed"
    assert check({"b/prj.conf": raised + "CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE=0\n"}), (
        "self-test: a task stack of 0 (= inherit) passed")
    assert check({"b/prj.conf": "CONFIG_MAIN_STACK_SIZE=16384\n"}) == [], (
        "self-test: the default main stack was refused")
    assert check({"b/prj-cyclonedds.conf": "CONFIG_MAIN_STACK_SIZE=524288\n"}) == [], (
        "self-test: a Cyclone-only conf was refused")
    assert check({"b/prj.conf": "# CONFIG_MAIN_STACK_SIZE=65536\n"}) == [], (
        "self-test: a commented-out line counted")
    accepted = next(iter(ACCEPTED))
    assert check({accepted: raised}) == [], "self-test: an accepted board was refused"
    for path, why in ACCEPTED.items():
        assert why.strip(), f"self-test: {path} is accepted with no reason"


def main() -> int:
    self_test()
    listed = subprocess.run(["git", "ls-files", "*.conf"], cwd=REPO, check=True,
                            capture_output=True, text=True).stdout.split()
    files = {}
    for rel in listed:
        if rel.startswith("third-party/"):
            continue
        f = REPO / rel
        if f.is_file():
            files[rel] = f.read_text(errors="replace")
    stale = [p for p in ACCEPTED if p not in files]
    if stale:
        print("check-zephyr-task-stack-stated: ACCEPTED names a conf that is gone: "
              + ", ".join(stale) + " -- delete the entry", file=sys.stderr)
        return 1
    problems = check(files)
    if problems:
        print("check-zephyr-task-stack-stated: a raised main stack would resize every "
              "zenoh task:", file=sys.stderr)
        print("\n".join(problems), file=sys.stderr)
        return 1
    raised = sum(1 for p, t in files.items()
                 if not NON_ZENOH.search(Path(p).name) and p not in ACCEPTED
                 and any(int(v) > ZENOH_MAIN_DEFAULT for v in MAIN.findall(t)))
    print(f"check-zephyr-task-stack-stated OK -- {raised} conf(s) raise the main stack, "
          f"each states the zenoh task stack; {len(ACCEPTED)} accepted board(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Issue 0708 — a board's boot funnel must publish the nros_log sink list.

`nros_log::dispatch_to_sinks` returns when no sink list has been published, so
until some code calls `init`, every record is constructed, dispatched and
DROPPED. An application notices its own missing output. A LIBRARY record cannot:
its author has no way to know whether the board initialised the facade, and issue
0589 put the zenoh session-pool diagnostic in exactly that position "so it would
reach `no_std` targets" — where, on ThreadX and NuttX, it reached nothing.

Measured before the fix: the threadx-linux logging fixture with only its own
`init` removed booted fully (banner, byte pool, network init, app thread) and
emitted 0 of 6 records.

# The rule

Every `pub fn run*` in a board crate — the boot funnels — must reach
`nros_log::init_default()`, either directly or by delegating to another funnel
that does.

# Why per-FUNNEL and not per-crate

Because per-crate is what hid this. Both `nros-board-freertos` and
`nros-board-threadx` DO contain the call, so any check asking "does this crate
initialise the facade" passes them — while `nros-board-freertos::run_bare` and
`run_entry` (2 of its 3 funnels) and `nros-board-threadx::run_bare` had none.
A first attempt at the fix made the same mistake one level down, patching next to
`install_uart_logger` (an inner helper) and leaving `run_bare` silent.

# Delegation counts

`nros-board-threadx-qemu-riscv64::run_app_thread` immediately calls
`nros_board_threadx::run_app_thread`, which initialises. A funnel whose body
names another `run*` is credited to it.

Run: python3 scripts/check-board-log-sink.py [--self-test]
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BOARDS = "packages/boards"

FUNNEL = re.compile(r"^pub fn (run[a-z_0-9]*)\s*[<(]")
INIT = "init_default"
DELEGATE = re.compile(r"\b(?:nros_board_\w+|self|Self)::(run[a-z_0-9]*)\s*(?:::<|\()")

# `nros-board-common` is a BUILD-SCRIPT crate: its `run*` functions drive image
# links and platform compiles at build time and never boot anything. Excluded by
# path, with the reason here rather than as a bare name in a list.
EXCLUDED_CRATES = {"nros-board-common"}


def tracked_rs():
    out = subprocess.run(
        ["git", "ls-files", f"{BOARDS}/*/src/*.rs", f"{BOARDS}/*/src/**/*.rs"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    )
    return [ROOT / p for p in out.stdout.split() if p]


def body_of(lines, start):
    """Lines of the fn body beginning at the signature line index `start`."""
    i = start
    while i < len(lines) and lines[i] != "{":
        i += 1
    depth, out = 0, []
    while i < len(lines):
        depth += lines[i].count("{") - lines[i].count("}")
        out.append(lines[i])
        if depth == 0 and out:
            break
        i += 1
    return out


def audit():
    problems = []
    for path in tracked_rs():
        crate = path.relative_to(ROOT / BOARDS).parts[0]
        if crate in EXCLUDED_CRATES:
            continue
        lines = path.read_text(encoding="utf-8").split("\n")
        for i, line in enumerate(lines):
            m = FUNNEL.match(line)
            if not m:
                continue
            body = "\n".join(body_of(lines, i))
            if INIT in body or DELEGATE.search(body):
                continue
            rel = path.relative_to(ROOT)
            problems.append(f"{rel}:{i + 1}: pub fn {m.group(1)}")
    return problems


def self_test():
    """The two shapes this check learned by getting them wrong."""
    ok = True
    lines = ["pub fn run_bare<B>(x: u8) -> ! ", "where", "    B: Board,", "{",
             "    banner();", "    ::nros_log::init_default();", "}"]
    if INIT not in "\n".join(body_of(lines, 0)):
        print("  FAIL  a multi-line signature must still find its body"); ok = False
    else:
        print("  ok    a multi-line signature finds its body")
    lines = ["pub fn run_app_thread<F, E>(setup: F) -> !", "{",
             "    nros_board_threadx::run_app_thread::<B, C, F, E>(setup)", "}"]
    if not DELEGATE.search("\n".join(body_of(lines, 0))):
        print("  FAIL  delegation to another funnel must count"); ok = False
    else:
        print("  ok    delegation to another funnel counts")
    lines = ["pub fn run_bare<B>(x: u8) -> !", "{", "    install_uart_logger::<B>();", "}"]
    b = "\n".join(body_of(lines, 0))
    if INIT in b or DELEGATE.search(b):
        print("  FAIL  a neighbouring logger install must NOT count"); ok = False
    else:
        print("  ok    a neighbouring logger install does not count")
    return ok


def main():
    if "--self-test" in sys.argv:
        sys.exit(0 if self_test() else 1)
    problems = audit()
    if problems:
        print("[FAIL] board boot funnel does not publish the nros_log sink list:",
              file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        print("\n  Until some code calls `init`, every record is DROPPED — silently,",
              file=sys.stderr)
        print("  and a library record's author cannot know (issues 0708, 0589).",
              file=sys.stderr)
        print("  Fix: `::nros_log::init_default();` as the funnel's first statement.",
              file=sys.stderr)
        sys.exit(1)
    print("check-board-log-sink: OK (every board boot funnel publishes a sink list)")


if __name__ == "__main__":
    main()

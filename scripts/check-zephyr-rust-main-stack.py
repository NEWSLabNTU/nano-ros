#!/usr/bin/env python3
"""Every Rust-capable Zephyr board package gives main enough stack (issue 2359).

A Rust nano-ros entry registers its nodes on Zephyr's MAIN thread, and the
Rust API's registration frames are deep: a `nros new --lang rust` talker on
nucleo-f767zi peaked at 28,280 bytes of main stack, against the 16,384 the
board's `prj.conf` stated. The overflow corrupted the idle thread's stack, and
the image halted at its first park with nothing on the console, which read as
a spin that never returns. The C and C++ talkers on the same board fit in
16 KiB, so only the Rust cell was red, and nothing in the tree builds that
cell.

The module refuses such an image at configure (`zephyr/CMakeLists.txt`, the
`NROS_RUST_MAIN_STACK_FLOOR` cache variable), but only an image somebody
builds reaches that refusal. This gate asks the same question of every board
package that says it builds Rust, without building anything:

  * a board package is in scope when its `nros-board.toml` has a
    `[board.zephyr]` table with a `rust_target`, and its `west_board` is not
    native_sim (on CONFIG_ARCH_POSIX a Zephyr thread runs on a host pthread's
    stack, so MAIN_STACK_SIZE bounds nothing, and the module skips it too);
  * its `prj.conf` must assign `CONFIG_MAIN_STACK_SIZE`, and the last
    assignment must be at least the floor. No assignment is refused: Zephyr's
    default is 1 KiB.

The floor is read from the module's own line, so the gate and the refusal
cannot disagree.

Usage::

    check-zephyr-rust-main-stack.py [--selftest]
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODULE = "zephyr/CMakeLists.txt"
FLOOR_RE = re.compile(r"^\s*set\(NROS_RUST_MAIN_STACK_FLOOR\s+(\d+)\b", re.M)
STACK_RE = re.compile(r"^\s*CONFIG_MAIN_STACK_SIZE\s*=\s*(\d+)\s*$", re.M)
POSIX_BOARDS = ("native_sim", "native_posix")


def floor_from(module_text: str) -> int | None:
    m = FLOOR_RE.search(module_text)
    return int(m.group(1)) if m else None


def in_scope(board_toml: str) -> bool:
    """Whether a board descriptor builds Rust on a real (non-POSIX) target."""
    # A descriptor spells its board as `[[board]]` (one or more rows) or, in
    # older files, as a `[board]` table.
    boards = tomllib.loads(board_toml).get("board")
    if isinstance(boards, dict):
        boards = [boards]
    for board in boards if isinstance(boards, list) else []:
        zephyr = board.get("zephyr") if isinstance(board, dict) else None
        if not isinstance(zephyr, dict) or "rust_target" not in zephyr:
            continue
        if not str(zephyr.get("west_board", "")).startswith(POSIX_BOARDS):
            return True
    return False


def offence(conf_path: str, conf_text: str | None, floor: int) -> str | None:
    """Why a board's `prj.conf` breaks the rule, or None."""
    if conf_text is None:
        return f"{conf_path}: missing, so CONFIG_MAIN_STACK_SIZE is Zephyr's 1 KiB default"
    found = STACK_RE.findall(conf_text)
    if not found:
        return (f"{conf_path}: assigns no CONFIG_MAIN_STACK_SIZE, so main gets Zephyr's "
                f"1 KiB default; state {floor} or more")
    value = int(found[-1])
    if value < floor:
        return (f"{conf_path}: CONFIG_MAIN_STACK_SIZE={value} is below the {floor}-byte "
                f"floor a Rust entry needs ({MODULE}, NROS_RUST_MAIN_STACK_FLOOR)")
    return None


def board_tomls() -> list[str]:
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "--", "packages/boards/**/nros-board.toml",
         "packages/boards/*/nros-board.toml"],
        check=True, capture_output=True, text=True).stdout
    return sorted(set(out.split()))


def read(rel: str) -> str | None:
    try:
        with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
            return f.read()
    except FileNotFoundError:
        return None


def selftest() -> int:
    bad = []
    if floor_from("    set(NROS_RUST_MAIN_STACK_FLOOR 32768 CACHE STRING\n") != 32768:
        bad.append("floor line not parsed")
    if floor_from("# no floor here\n") is not None:
        bad.append("a module with no floor line parsed one")
    rust = '[[board]]\n[board.zephyr]\nwest_board = "nucleo_f767zi"\nrust_target = "thumbv7em-none-eabi"\n'
    table = '[board.zephyr]\nwest_board = "nucleo_f767zi"\nrust_target = "thumbv7em-none-eabi"\n'
    c_only = '[board.zephyr]\nwest_board = "nucleo_f767zi"\n'
    posix = '[board.zephyr]\nwest_board = "native_sim/native/64"\nrust_target = "x"\n'
    if not in_scope(rust) or not in_scope(table) or in_scope(c_only) or in_scope(posix):
        bad.append("scope: rust board must be in, C-only and native_sim out")
    cases = [
        ("CONFIG_MAIN_STACK_SIZE=16384\n", True),     # issue 2359's board
        ("CONFIG_MAIN_STACK_SIZE=32768\n", False),
        ("CONFIG_MAIN_STACK_SIZE=65536\nCONFIG_MAIN_STACK_SIZE=16384\n", True),  # last wins
        ("CONFIG_FOO=y\n", True),                     # unset: 1 KiB default
        (None, True),
    ]
    for text, refused in cases:
        if (offence("prj.conf", text, 32768) is not None) != refused:
            bad.append(f"conf {text!r}: expected refused={refused}")
    for b in bad:
        print(f"check-zephyr-rust-main-stack selftest: {b}")
    if bad:
        return 1
    print("check-zephyr-rust-main-stack selftest: OK")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if selftest() != 0:
        return 1
    if args.selftest:
        return 0
    floor = floor_from(read(MODULE) or "")
    if floor is None:
        print(f"check-zephyr-rust-main-stack: {MODULE} has no "
              "`set(NROS_RUST_MAIN_STACK_FLOOR <n> ...)` line; the gate reads its "
              "floor from there")
        return 1
    errors, checked = [], 0
    for toml_path in board_tomls():
        text = read(toml_path)
        if text is None or not in_scope(text):
            continue
        checked += 1
        conf = os.path.join(os.path.dirname(toml_path), "prj.conf")
        why = offence(conf, read(conf), floor)
        if why:
            errors.append(why)
    if checked == 0:
        print("check-zephyr-rust-main-stack: no Rust-capable Zephyr board package "
              "found; the scope query is broken")
        return 1
    if errors:
        print("check-zephyr-rust-main-stack: a Rust entry registers on the main "
              "thread and overflows a small main stack silently (issue 2359):")
        for e in errors:
            print(f"  {e}")
        return 1
    print(f"check-zephyr-rust-main-stack: OK, {checked} Rust-capable Zephyr board "
          f"package(s) give main at least {floor} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())

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

The module refuses such an image at configure, against its BOARD's measured
floor (`cmake/NanoRosStackFloor.cmake`, `nros_stack_floor_zephyr_rust_main`;
phase-557 W10): the largest Rust template peak measured on the board plus the
table's margin, or the table's fallback on a board with no measured row. But
only an image somebody builds reaches that refusal. This gate asks the same question of every board
package that says it builds Rust, without building anything:

  * a board package is in scope when its `nros-board.toml` has a
    `[board.zephyr]` table with a `rust_target`, and its `west_board` is not
    native_sim (on CONFIG_ARCH_POSIX a Zephyr thread runs on a host pthread's
    stack, so MAIN_STACK_SIZE bounds nothing, and the module skips it too);
  * the main stack its Rust image gets must be at least THAT BOARD's floor,
    read from the same producer the configure calls
    (`nros-stack-floor.py --board-floor <name>`, `names[0]` of the board), so
    the two cannot disagree: the
    `prj.conf`'s last `CONFIG_MAIN_STACK_SIZE` assignment when it has one,
    otherwise the module's Rust default. Since issue 2365 the module decides
    main's stack per image (`zephyr/Kconfig`, `configdefault MAIN_STACK_SIZE`
    `default <N> if NROS_RUST_API && !ARCH_POSIX` in the zenoh and the XRCE
    block), and a board states none, because one board number reached every
    language. Each of those module defaults must itself be at least the floor,
    and both blocks must have one. Each must be at least the table's fallback,
    which a board with no measured row and no `prj.conf` value gets.

Usage::

    check-zephyr-rust-main-stack.py [--selftest]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WALKER = os.path.join(ROOT, "scripts", "nros-stack-floor.py")
TABLE = os.path.join(ROOT, "config", "stack-floors.json")
STACK_RE = re.compile(r"^\s*CONFIG_MAIN_STACK_SIZE\s*=\s*(\d+)\s*$", re.M)
KCONFIG = "zephyr/Kconfig"
RMW_BLOCKS = ("NROS_RMW_ZENOH", "NROS_RMW_XRCE")
# One `default` line of a `configdefault MAIN_STACK_SIZE` block: the generic
# Rust default (`if NROS_RUST_API && !ARCH_POSIX`), or a board's Rust-only one
# (`... && BOARD_<SYM>`, phase-557 W10).
RUST_LINE_RE = re.compile(
    r"^\s+default (\d+) if NROS_RUST_API && !ARCH_POSIX(?: && (BOARD_[A-Z0-9_]+))?\s*$")


def rust_defaults(kconfig_text: str) -> dict[str, dict]:
    """Per RMW block that has one: {"generic": N, "boards": {BOARD_SYM: N}},
    read from its `configdefault MAIN_STACK_SIZE` block (comments skipped)."""
    found: dict[str, dict] = {}
    for rmw in RMW_BLOCKS:
        for block in re.findall(rf"^if {rmw}\n(.*?)^endif # {rmw}\n", kconfig_text,
                                re.M | re.S):
            lines = block.splitlines()
            for i, line in enumerate(lines):
                if line.strip() != "configdefault MAIN_STACK_SIZE":
                    continue
                entry: dict = {"generic": None, "boards": {}}
                for body in lines[i + 1:]:
                    st = body.strip()
                    if st.startswith("#"):
                        continue
                    if not st.startswith("default "):
                        break
                    m = RUST_LINE_RE.match(body)
                    if m and m.group(2):
                        entry["boards"][m.group(2)] = int(m.group(1))
                    elif m:
                        entry["generic"] = int(m.group(1))
                if entry["generic"] is not None:
                    found[rmw] = entry
    return found


def board_symbol(board_toml: str) -> str | None:
    """The board's Zephyr Kconfig symbol (hwmv2: `BOARD_<name>`, the
    `west_board` up to its first `/`, upper-cased)."""
    boards = tomllib.loads(board_toml).get("board")
    if isinstance(boards, dict):
        boards = [boards]
    for board in boards if isinstance(boards, list) else []:
        z = board.get("zephyr") if isinstance(board, dict) else None
        if isinstance(z, dict) and z.get("west_board"):
            name = str(z["west_board"]).split("/", 1)[0].split("@", 1)[0]
            return "BOARD_" + re.sub(r"[^A-Za-z0-9]", "_", name).upper()
    return None


POSIX_BOARDS = ("native_sim", "native_posix")


def board_floor(name: str) -> dict:
    """The board's floor, from the ONE producer the configure also calls."""
    out = subprocess.run([sys.executable, WALKER, "--board-floor", name, "--table", TABLE],
                         check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def board_name(board_toml: str) -> str | None:
    """The nano-ros board name (`names[0]`) of the descriptor's in-scope row."""
    boards = tomllib.loads(board_toml).get("board")
    if isinstance(boards, dict):
        boards = [boards]
    for board in boards if isinstance(boards, list) else []:
        if isinstance(board, dict) and board.get("names"):
            return str(board["names"][0])
    return None


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


def offence(conf_path: str, conf_text: str | None, floor: int,
            module_default: int | None = None, basis: str = "") -> str | None:
    """Why a board's `prj.conf` breaks the rule, or None. With no assignment
    the module's Rust default applies (`module_default`; None = none found)."""
    if conf_text is None:
        return f"{conf_path}: missing"
    found = STACK_RE.findall(conf_text)
    if not found:
        if module_default is None:
            return (f"{conf_path}: assigns no CONFIG_MAIN_STACK_SIZE and {KCONFIG} has no "
                    "Rust main-stack default, so main gets Zephyr's 1 KiB default")
        found = [str(module_default)]
    value = int(found[-1])
    if value < floor:
        return (f"{conf_path}: CONFIG_MAIN_STACK_SIZE={value} is below the {floor}-byte "
                f"floor a Rust entry needs on this board{basis}")
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
    if board_name('[[board]]\nnames = ["x-zephyr", "x"]\n') != "x-zephyr":
        bad.append("board name is not names[0]")
    nb = board_floor("no-such-board-for-the-selftest")
    if nb.get("basis") != "fallback" or not isinstance(nb.get("floor"), int):
        bad.append(f"a board with no row did not get the fallback: {nb}")
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
        ("CONFIG_FOO=y\n", True),                     # unset, no module default
        (None, True),
    ]
    for text, refused in cases:
        if (offence("prj.conf", text, 32768) is not None) != refused:
            bad.append(f"conf {text!r}: expected refused={refused}")
    # Issue 2365: unset now means the module's Rust default.
    if offence("prj.conf", "CONFIG_FOO=y\n", 32768, 32768) is not None:
        bad.append("an unset board with a module default of 32768 was refused")
    if offence("prj.conf", "CONFIG_FOO=y\n", 32768, 16384) is None:
        bad.append("an unset board with a module default under the floor passed")
    kc = ("if NROS_RMW_ZENOH\n\nconfigdefault MAIN_STACK_SIZE\n"
          "    default 32768 if NROS_RUST_API && !ARCH_POSIX\n    default 16384\n\n"
          "endif # NROS_RMW_ZENOH\n\nif NROS_RMW_XRCE\n\nconfigdefault MAIN_STACK_SIZE\n"
          "    default 16384\n\nendif # NROS_RMW_XRCE\n")
    if rust_defaults(kc) != {"NROS_RMW_ZENOH": {"generic": 32768, "boards": {}}}:
        bad.append(f"Kconfig parse: {rust_defaults(kc)} (XRCE block has no Rust default)")
    kb = ("if NROS_RMW_ZENOH\n\nconfigdefault MAIN_STACK_SIZE\n    # a comment\n"
          "    default 33288 if NROS_RUST_API && !ARCH_POSIX && BOARD_NUCLEO_F767ZI\n"
          "    default 32768 if NROS_RUST_API && !ARCH_POSIX\n    default 16384\n\n"
          "endif # NROS_RMW_ZENOH\n")
    if rust_defaults(kb) != {"NROS_RMW_ZENOH": {"generic": 32768,
                                                 "boards": {"BOARD_NUCLEO_F767ZI": 33288}}}:
        bad.append(f"Kconfig parse of a board's Rust-only line: {rust_defaults(kb)}")
    if board_symbol(rust) != "BOARD_NUCLEO_F767ZI" or board_symbol(
            '[board.zephyr]\nwest_board = "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp"\n'
    ) != "BOARD_FVP_BASER_AEMV8R":
        bad.append("board symbol: the west_board up to its first /, upper-cased")
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
    fallback = board_floor("").get("fallback")
    if not isinstance(fallback, int):
        print("check-zephyr-rust-main-stack: config/stack-floors.json states no "
              "`fallback_rust_main_stack`")
        return 1
    errors, checked, floors = [], 0, []
    defaults = rust_defaults(read(KCONFIG) or "")
    for rmw in RMW_BLOCKS:
        if rmw not in defaults:
            errors.append(f"{KCONFIG}: the `if {rmw}` block has no `configdefault "
                          "MAIN_STACK_SIZE` Rust default (`default <N> if NROS_RUST_API "
                          "&& !ARCH_POSIX`), so a Rust image gets the C number")
        elif defaults[rmw]["generic"] < fallback:
            errors.append(f"{KCONFIG}: the {rmw} Rust main-stack default "
                          f"{defaults[rmw]['generic']} is below the {fallback}-byte fallback floor")
    if len({repr(d["boards"]) for d in defaults.values()}) > 1:
        errors.append(f"{KCONFIG}: the RMW blocks state different per-board Rust main-stack "
                      "defaults; a board's floor does not depend on its RMW, so they must agree")
    for toml_path in board_tomls():
        text = read(toml_path)
        if text is None or not in_scope(text):
            continue
        checked += 1
        name = board_name(text)
        if name is None:
            errors.append(f"{toml_path}: a Rust-capable board with no `names`")
            continue
        bf = board_floor(name)
        floor = int(bf["floor"])
        basis = (f" (its measured Rust template rows: {', '.join(bf['rows'])}, peak "
                 f"{bf['peak']} B + {bf['margin_pct']} %)" if bf["basis"] == "rows"
                 else " (no measured Rust template row: the table's fallback)")
        floors.append(f"{name} {floor}")
        sym = board_symbol(text)
        # The main stack an image on this board gets with no conf of its own:
        # the board's Rust-only line if the module states one, else the
        # generic Rust default (the least over the RMW blocks).
        per = [d["boards"].get(sym, d["generic"]) for d in defaults.values()]
        module_default = min(per) if per else None
        conf = os.path.join(os.path.dirname(toml_path), "prj.conf")
        why = offence(conf, read(conf), floor, module_default, basis)
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
          f"package(s) give main at least their board's floor ({', '.join(floors)})")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Every Zephyr board states its west lines, and its modules agree (issue 2804).

A Zephyr board's descriptor states `[board.zephyr] lines = [...]`, the lines
it builds on. The fixture build skips a west leaf for a board off the run's
line, and the test lane deselects it (`[SKIPPED:lane]`) before any probe. A
board that stated nothing would silently mean "every line", which is what
reported a 3.7-only board UNMET on 4.4.

Checked, per Zephyr board:

1. it STATES `lines`, non-empty, each one a line nano-ros pins a manifest for;
2. every `[zephyr_module.*]` whose `needed_by` names the board is on at least
   one of the board's lines (a board claiming only lines where its HAL does not
   exist is the defect, e.g. the Nucleo-F767ZI on 4.4 without `hal_stm32`);
3. on each line the board states, every module that names it AND is on that
   line is in that line's west manifest allowlist.

What it cannot check, said here so nobody reads more into green: that a board
WORKS on a line it states. Only a build answers that; the gate keeps the
statement present and consistent with the index.

Run:  python3 scripts/check-zephyr-board-lines.py [--self-test]
"""
from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "scripts" / "lib"))
import zephyr_board_lines as zbl  # noqa: E402


def _allowlist_gate():
    spec = importlib.util.spec_from_file_location(
        "zephyr_module_allowlist", REPO / "scripts" / "check-zephyr-module-allowlist.py")
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def index_names(west_board: str) -> set[str]:
    """The spellings the index's `needed_by` uses for a west board: its first
    segment (`native_sim`, `nucleo_f767zi`) and its first two joined by `_`
    (`mps2_an385`). A revision (`@D`) is not part of either."""
    segs = [s.split("@", 1)[0] for s in west_board.split("/")]
    names = {segs[0]}
    if len(segs) > 1:
        names.add(f"{segs[0]}_{segs[1]}")
    return names


def board_errors(rel: str, west_board: str, lines, modules: dict, allow: dict) -> list[str]:
    """`modules`: name -> (needed_by, lines). `allow`: line -> allowlist set."""
    where = f"{rel} ({west_board})"
    if lines is None:
        return [f"{where}: states no `[board.zephyr] lines`; name the Zephyr lines it builds on"]
    if not lines:
        return [f"{where}: `lines = []` builds on no line; name at least one"]
    errs = [f"{where}: line {l!r} is not one nano-ros pins ({', '.join(zbl.LINES)})"
            for l in lines if l not in zbl.LINES]
    names = index_names(west_board)
    for mod, (needed_by, mlines) in sorted(modules.items()):
        if not names & set(needed_by):
            continue
        if not set(mlines) & set(lines):
            errs.append(f"{where}: needs module `{mod}`, which is on {mlines or 'no line'}, "
                        f"and the board states only {lines}")
        for l in sorted(set(mlines) & set(lines)):
            if l in allow and mod not in allow[l]:
                errs.append(f"{where}: needs `{mod}` on {l}, and {zbl.LINES[l]}'s allowlist "
                            "does not name it")
    return errs


def index_modules(text: str) -> dict:
    """name -> (needed_by, lines), by the allowlist gate's hand parser plus
    `needed_by` read the same way."""
    import re
    lines = _allowlist_gate().parse_index_modules(text)
    needed: dict[str, list] = {}
    current = None
    for raw in text.splitlines():
        s = raw.strip()
        if s.startswith("[") and s.endswith("]"):
            sec = s[1:-1]
            current = sec[len("zephyr_module."):] if sec.startswith("zephyr_module.") else None
            if current is not None:
                needed.setdefault(current, [])
            continue
        if current is not None:
            m = re.match(r"needed_by\s*=\s*\[(.*)\]\s*$", s)
            if m:
                needed[current] = re.findall(r'"([^"]+)"', m.group(1))
    return {n: (needed.get(n, []), lines.get(n, [])) for n in set(lines) | set(needed)}


def self_test() -> None:
    mods = {"hal_stm32": (["nucleo_f767zi"], ["3.7"]),
            "tinycrypt": (["mps2_an385"], ["3.7"]),
            "cmsis_6": (["mps2_an385"], ["4.4"])}
    allow = {"3.7": {"hal_stm32", "tinycrypt"}, "4.4": {"cmsis_6"}}
    ok = lambda *a: board_errors("d", *a, mods, allow)  # noqa: E731
    assert ok("nucleo_f767zi", ["3.7"]) == []
    # negative control: the Nucleo stated on 4.4 only, where its HAL is not
    assert len(ok("nucleo_f767zi", ["4.4"])) == 1
    # per-line components on both lines: a board on both is fine
    assert ok("mps2/an385", ["3.7", "4.4"]) == []
    assert "states no" in ok("x/y", None)[0]
    assert "no line" in ok("x/y", [])[0]
    assert "not one nano-ros pins" in ok("x/y", ["9.9"])[0]
    # a module on the line but missing from that line's allowlist
    assert "allowlist" in board_errors("d", "nucleo_f767zi", ["3.7"], mods, {"3.7": set()})[0]
    assert index_names("s32z2xxdc2@D/s32z270/rtu0") == {"s32z2xxdc2", "s32z2xxdc2_s32z270"}
    print("check-zephyr-board-lines self-test: OK (8 cases)")


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        return 0
    gate = _allowlist_gate()
    modules = index_modules((REPO / "nros-sdk-index.toml").read_text())
    allow = {l: set(gate.parse_allowlist((REPO / m).read_text())) for l, m in zbl.LINES.items()}
    errs = []
    boards = zbl.zephyr_boards(REPO)
    for rel, wb, lines in boards:
        errs += board_errors(rel, wb, lines, modules, allow)
    if errs:
        print("check-zephyr-board-lines: FAIL")
        for e in errs:
            print(f"  {e}")
        return 1
    print(f"check-zephyr-board-lines: OK — {len(boards)} Zephyr board(s), each states "
          "its lines, and the modules naming it agree")
    return 0


if __name__ == "__main__":
    sys.exit(main())

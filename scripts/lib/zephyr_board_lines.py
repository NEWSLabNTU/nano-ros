"""Which Zephyr west lines each Zephyr board builds on (issue 2804).

A board STATES its lines in its descriptor, `[board.zephyr] lines = [...]`,
once per board. They are not derived from the index: `[zephyr_module.*]
needed_by` mixes a board's HAL (needed on every line the board is on) with
per-line components (`tinycrypt` on 3.7 only, `cmsis_6` on 4.4 only, both
naming `mps2_an385`), so no rule over it says which lines a board is on.

This is the one reader. `fixtures-manifest.py` uses it to skip a west leaf on a
line its board is not on (`west-leaves --line`) and to export each leaf's lines
for the test lane (`west-leaf-lines`). `check-zephyr-board-lines.py` uses it to
refuse a Zephyr board that states none.

`LINES` is the line -> west manifest table. `check-zephyr-module-allowlist.py`
asserts it against the CLI's `ZEPHYR_MANIFEST_LINES`.
"""
from __future__ import annotations

import sys
from pathlib import Path

_LIB = Path(__file__).resolve().parent
if str(_LIB) not in sys.path:
    sys.path.insert(0, str(_LIB))

import board_registry  # noqa: E402  the tomllib/tomli loader every gate uses
import board_roots  # noqa: E402  the tracked descriptor walk

REPO = _LIB.parent.parent

# Line -> the west manifest that pins it.
LINES = {"3.7": "west.yml", "4.4": "west-4.4.yml"}


def zephyr_boards(root: Path = REPO) -> list[tuple[str, str, list[str] | None]]:
    """(descriptor path, west_board, stated lines or None) per Zephyr board.

    A descriptor holds one or more `[[board]]` tables. A Zephyr board is one
    whose `platform` is `zephyr`. `None` means it states no `lines` key at all.
    """
    out = []
    for rel in board_roots.tracked_board_definitions(str(root)):
        doc = board_registry.load(root / rel)
        for b in doc.get("board", []):
            if b.get("platform") != "zephyr":
                continue
            z = b.get("zephyr") or {}
            lines = z.get("lines")
            out.append((rel, z.get("west_board", ""), list(lines) if lines is not None else None))
    return out


def board_lines(root: Path = REPO) -> dict[str, list[str]]:
    """west_board -> its stated lines, for every Zephyr board that states them."""
    return {wb: lines for _, wb, lines in zephyr_boards(root) if wb and lines}


def leaf_lines(west_board: str, table: dict[str, list[str]]) -> list[str] | None:
    """The lines a west leaf for `west_board` builds on, or None if no board
    descriptor names that board (the caller refuses that: a leaf for a board
    nobody describes is a modelling gap, never "every line")."""
    return table.get(west_board)

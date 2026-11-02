"""The book's contributor-block exemption, in one place.

A user-track page may carry contributor material, and a gate about what a user
reads must not count it. `check-book-no-just` defined what "contributor-marked"
means first; `check-user-surface-ids` reuses the same rule, so the two cannot
drift apart. A line's context is contributor-marked when any of these holds
(checked in this order):

* the line itself or any of the LOOKBACK lines above it contains
  "contributor" (case-insensitive), or one of the caller's extra markers;
* the nearest preceding markdown heading contains "contributor", so a
  "## Contributor setup" section licenses its whole body;
* for a table row: the current table's header row contains "contributor".

A marker at the top of the page does NOT exempt the page; only the nearest
heading does, and only when the heading itself says so.
"""

from __future__ import annotations

import re
from collections.abc import Iterator
from dataclasses import dataclass

LOOKBACK = 8
HEADING = re.compile(r"^#{1,6}\s")
TABLE_ROW = re.compile(r"^\s*\|")
CONTRIBUTOR = "contributor"


@dataclass
class Line:
    number: int  # 1-based
    text: str
    in_fence: bool
    exempt: bool


def walk(lines: list[str], extra_markers: tuple[str, ...] = ()) -> Iterator[Line]:
    """Yield every line of a markdown page that is not a fence delimiter,
    with whether it sits in a fence and whether its context is
    contributor-marked."""
    marker = re.compile("|".join([CONTRIBUTOR, *map(re.escape, extra_markers)]), re.I)
    in_fence = False
    heading = ""
    table_head = ""
    for i, line in enumerate(lines):
        if not in_fence and HEADING.match(line):
            heading = line
        if TABLE_ROW.match(line):
            if not table_head:
                table_head = line  # first row of this table = its header
        else:
            table_head = ""
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        window = lines[max(0, i - LOOKBACK) : i + 1]
        exempt = (
            any(marker.search(w) for w in window)
            or bool(marker.search(heading))
            or bool(TABLE_ROW.match(line) and marker.search(table_head))
        )
        yield Line(i + 1, line, in_fence, exempt)

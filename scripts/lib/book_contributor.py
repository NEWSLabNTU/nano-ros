"""The book's verbatim-text escape, in one place.

Until phase-532 W3 this module licensed CONTRIBUTOR material inside user
chapters: a "contributor" within eight lines, a heading saying so, or a table
column saying so exempted a line from the user-surface gates. RFC-0123 D3
retired that. Contributor material lives in the Internals book
(`book-internals/`), the user-surface gates read every user page, and nothing
on a user page is exempt for its audience.

What remains is the escape for VERBATIM text: a caller's explicit marker
(`check-book-no-just` passes `no-just-ok`, an HTML comment that marks quoted
tool output) within LOOKBACK lines above, or in the nearest heading, or in the
current table's header row. The word "contributor" is not a marker.
"""

from __future__ import annotations

import re
from collections.abc import Iterator
from dataclasses import dataclass

LOOKBACK = 8
HEADING = re.compile(r"^#{1,6}\s")
TABLE_ROW = re.compile(r"^\s*\|")


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
    # RFC-0123 D3: only the caller's explicit verbatim markers; no audience marker.
    marker = re.compile("|".join(map(re.escape, extra_markers)) or r"(?!)", re.I)
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

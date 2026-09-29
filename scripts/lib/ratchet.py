"""A count ratchet moves in ONE direction, and every move is written down — phase-472 W9.

A ratchet baseline records, per key (a file, a crate/kind, an override tuple),
the count present when it was taken. The 2026-09-28 audit found the same hole
in three of them (`check-wait-evidence-discarded`, `check-unsafe-census`,
`check-kconfig-overridden-values`): they failed when a count ROSE above its row
and merely NOTED when one fell. So the progress was never locked in:

    baseline says 5, a PR converts two sites (tree: 3), nobody lowers the row,
    and a later PR adds two back (tree: 5) — green, because 5 <= 5.

The debt regrew to its old count with no gate ever objecting. A row the tree is
no longer at is a row no file is held to. The fix is the one rule both ways:

* a count ABOVE its recorded value fails (a key the baseline lacks is recorded
  as 0, so a NEW key is just the largest possible rise);
* a count BELOW its recorded value ALSO fails, naming the exact baseline edit —
  the change that made the progress is the change that records it, in the same
  commit, so the ratchet tightens by construction rather than when someone
  remembers.

`judge()` is the whole comparison, so every member gate's selftest drives the
same function its scan does — a selftest over a copied comparator proves
nothing about the gate (the class W9 also names).
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Hashable, Iterable, Mapping


@dataclass(frozen=True)
class Move:
    key: Hashable
    was: int  # recorded in the baseline (0 when the baseline has no such key)
    now: int  # measured in the tree (0 when the tree has no such key)


def judge(now: Mapping[Hashable, int], base: Mapping[Hashable, int]):
    """(rose, fell) — every key whose measured count differs from its record.

    Missing on either side counts as 0, so a key new to the tree is a rise and
    a key gone from the tree is a fall. Both lists are sorted by key text.
    """
    rose, fell = [], []
    for k in sorted(set(now) | set(base), key=str):
        n, b = int(now.get(k, 0)), int(base.get(k, 0))
        if n > b:
            rose.append(Move(k, b, n))
        elif n < b:
            fell.append(Move(k, b, n))
    return rose, fell


def fell_instructions(
    fell: Iterable[Move],
    baseline: str,
    spell: Callable[[Hashable, int], "str | None"],
    regenerate: str,
) -> list[str]:
    """The exact edit that records each fall — lines for a failure message.

    `spell(key, n)` renders the baseline row for `key` at count `n` (None when
    a count of `n` means the row is DELETED). The message names the row as it
    is, the row as it must become, and the command that rewrites the file.
    """
    out = [
        f"  a count FELL, and {baseline} still records the old value — lower it",
        "  in THIS change, or the progress is not locked in and the debt can",
        "  regrow to the recorded count with no gate objecting (phase-472 W9):",
    ]
    for m in fell:
        old, new = spell(m.key, m.was), spell(m.key, m.now)
        if new is None:
            out.append(f"      delete:   {old}")
        else:
            out.append(f"      replace:  {old}")
            out.append(f"      with:     {new}")
    out.append(f"  or regenerate it: {regenerate}")
    return out

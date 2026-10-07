#!/usr/bin/env python3
"""Exemptions keyed on EXACTLY what their rationale names — phase-472 W8.

An exemption states a reason about one thing ("`scripts/install.sh` may fetch
`nros`", "this single-executor loop has no second tier to starve"). The audit
found nine gates whose exemption MATCHED more than that thing, so it covered
the neighbouring case for free:

* `check-one-producer-per-tool` — forwarding ANY tool excused producing any
  other in the same body;
* `check-posix-platform-purity` — any `__linux__` within 12 lines counted as the
  guard, even one closed by an `#endif` above the call;
* `check-lane-scope-consumers` — exempted every `native_*` file, which the host
  lane RUNS;
* `check-tier-has-ci-owner` — a step's `name:` line counted as an owner, and a
  build-only `just ci matrix build` as owning tier 2;
* `check-ros2-daemon-queries` — an allowlist keyed on the path, not path+verb.

So an exemption here is a TABLE keyed by the exact tuple its reason is about,
with the reason as the value, and every table ships NEIGHBOURS: keys one step
away that it must NOT cover. `check()` proves both on the gate's normal path.

API
    Exemptions(table, *, what)        {key tuple: reason}; reasons must be non-empty
      .covers(key) -> bool            exact match; records the key as SEEN
      .stale() -> [key]               entries never seen by the scan
      .check(neighbours) -> [problem] reason-less entries + neighbours it covers
    self_test()
"""

from __future__ import annotations


class Exemptions:
    def __init__(self, table, *, what="site"):
        self.table = dict(table)
        self.what = what
        self.seen = set()

    def covers(self, key) -> bool:
        if key in self.table:
            self.seen.add(key)
            return True
        return False

    def reason(self, key):
        return self.table.get(key)

    def stale(self):
        return sorted((k for k in self.table if k not in self.seen), key=str)

    def check(self, neighbours=()):
        """Problems with the TABLE itself: reason-less entries, and any neighbour
        (a key one step from an entry) that the table covers. Does not mark
        anything seen.

        Runs the helper's own `self_test()` once per process first. The
        2026-10-07 gate-reach re-audit disarmed the neighbour test below and
        every member stayed green: members drive `check()` over their own
        tables, which cover no neighbour, so an empty answer looked the same
        as a correct one. Only the helper's controls can tell them apart, and
        no member ran them."""
        _self_test_once()
        problems = [f"exemption {k!r} carries no reason" for k, r in self.table.items()
                    if not str(r or "").strip()]
        problems += [f"exemption table covers the NEIGHBOUR {n!r} — key it on exactly "
                     f"what its reason names" for n in neighbours if n in self.table]
        if not neighbours and self.table:
            problems.append(f"{self.what} exemptions ship no neighbour rows — nothing "
                            f"shows they are narrow")
        return problems


_SELF_TESTED = False


def _self_test_once():
    global _SELF_TESTED
    if not _SELF_TESTED:
        _SELF_TESTED = True  # set first: self_test() calls check()
        self_test()


def self_test():
    ex = Exemptions({("install.sh", "nros"): "bootstraps the producer"}, what="producer")
    assert ex.check([("install.sh", "qemu")]) == []
    assert ex.check([("install.sh", "nros")]), "a neighbour equal to an entry must be refused"
    assert ex.check([]), "a table with no neighbour rows must be refused"
    assert Exemptions({("a",): ""}).check([("b",)]), "a reason-less entry must be refused"
    assert ex.covers(("install.sh", "nros")) and not ex.covers(("install.sh", "qemu"))
    assert ex.stale() == []
    assert Exemptions({("x",): "r"}).stale() == [("x",)]


if __name__ == "__main__":
    self_test()
    print("exemptions self-test: OK")

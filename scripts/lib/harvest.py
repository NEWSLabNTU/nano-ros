#!/usr/bin/env python3
"""Harvested populations and REASONED exemptions — phase-472 W7.

A gate whose population is a HAND-MAINTAINED LIST goes stale the day the tree
grows past it, and it goes stale in the quiet direction: the new item is simply
not asked. The audit found ten: `check-msg-dep-is-path` knew 16 message crates
and not `px4_msgs` or `custom_msgs`; `check-ffi-struct-mirrors` checked 2 of 3
mirrored structs; `check-decoupling` listed a crate that no longer exists and
missed four that do; `check-rmw-ret-sign` watched 43 of 66 status slots.

The fix has one shape, so it is one helper:

  * the POPULATION is HARVESTED from the source of truth (every manifest,
    every header, every vtable slot, every crate of a kind);
  * the authored list becomes an EXEMPTION list — `{name: reason}` — and every
    entry must (a) carry a reason and (b) still name something the harvest
    found, or it is STALE and fails. An exemption that matches nothing is how an
    allow-list quietly stops covering what it claims.

API
    reconcile(population, exemptions, *, what) -> (checked, problems)
        `checked` = the population minus the exempt names, sorted;
        `problems` = empty population, reason-less entries, stale entries.
    self_test()
"""

from __future__ import annotations


def reconcile(population, exemptions=None, *, what="item"):
    exemptions = exemptions or {}
    pop = sorted(set(population))
    problems = []
    if not pop:
        problems.append(f"the harvest found NO {what} — a harvest that finds nothing "
                        f"is a gate that is off, not a clean tree")
    for name, reason in sorted(exemptions.items()):
        if not str(reason or "").strip():
            problems.append(f"exemption {name!r} carries no reason — say why, or delete it")
        if name not in pop:
            problems.append(f"STALE exemption {name!r}: the harvest has no such {what}; "
                            f"delete it (an allow-list checked one way stops covering "
                            f"what it claims)")
    return [p for p in pop if p not in exemptions], problems


def self_test():
    checked, probs = reconcile(["a", "b", "c"], {"b": "why"}, what="crate")
    assert checked == ["a", "c"] and probs == [], (checked, probs)
    _c, probs = reconcile(["a"], {"gone": "was here"}, what="crate")
    assert any("STALE" in p and "'gone'" in p for p in probs), probs
    _c, probs = reconcile(["a"], {"a": ""}, what="crate")
    assert any("no reason" in p for p in probs), probs
    _c, probs = reconcile([], {}, what="crate")
    assert any("NO crate" in p for p in probs), probs


if __name__ == "__main__":
    self_test()
    print("harvest self-test: OK")

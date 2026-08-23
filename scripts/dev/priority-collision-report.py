#!/usr/bin/env python3
"""RFC-0079 evidence: what would a priority ALLOCATOR say about the pins we
already have?

Every `[tiers.<name>.<platform>] priority = N` in the tree is a STATIC LEASE in
RFC-0079's terms — a hand-chosen address in a space that also holds system
tasks nobody enumerated. This reports each pin against the system bands we can
actually cite from code, so the migration section argues from counted
collisions rather than from a guess.

It reports; it does not gate. The bands below are what is DISCOVERABLE today,
and two ports have none at all, which is itself the finding.

Run: python3 scripts/dev/priority-collision-report.py
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "lib"))
from priority_plan import load_plans, scan_pins  # RFC-0079: ONE plan reader

ROOT = Path(__file__).resolve().parent.parent.parent

# Ports that have no `[board.priority_plan]` yet. Named here ONLY to explain
# WHY, since "no plan" is the report's most common verdict and a bare count
# would read as an oversight rather than a capability gap.
NO_PLAN_REASON = {
    "zephyr": "settable since issue 0626 (zpico_posix_set_priority, NORMALISED "
              "0-31) but no port declares a band",
    "posix": "zpico_set_task_config DISCARDS priority on Linux/macOS (a hosted "
             "privilege concern) — no band exists to collide with",
    "threadx": "carries the platform ABI's attributes (issue 0626); no band "
               "declared",
}


def more_urgent(a, b, direction):
    return a > b if direction == "bigger-is-urgent" else a < b


def verdict(plans, plat, prio):
    plan = plans.get(plat)
    if plan is None:
        why = NO_PLAN_REASON.get(plat, "port is not described at all")
        return "UNPLANNED", why
    d = plan["direction"]
    worst = None
    for name, (lo, hi) in plan["reserved"].items():
        if lo <= prio <= hi:
            return "COLLIDES", f"lands ON the {name} band {lo}..{hi}"
        floor = lo if d == "bigger-is-urgent" else hi
        if more_urgent(prio, floor, d):
            worst = f"PREEMPTS the {name} band ({prio} vs {floor}, {d})"
    return ("PREEMPTS", worst) if worst else ("below bands", "")


def main():
    plans = load_plans()
    pins = scan_pins()
    rows = [(rel, tier, plat, prio, *verdict(plans, plat, prio))
            for rel, tier, plat, prio in pins]
    bringups = {r[0] for r in rows}

    print(f"RFC-0079 collision report — {len(rows)} pin(s) over "
          f"{len(bringups)} bringup(s), {len(plans)} declared plan(s)\n")

    tally = {}
    for r in rows:
        tally[r[4]] = tally.get(r[4], 0) + 1
    for v in sorted(tally, key=lambda k: -tally[k]):
        print(f"  {tally[v]:3d}  {v}")

    print("\n--- pins whose verdict is not 'below bands' ---")
    any_bad = False
    for rel, tier, plat, prio, v, why in rows:
        if v == "below bands":
            continue
        any_bad = True
        print(f"  [{v}] {rel}: tiers.{tier}.{plat} = {prio}")
        if why:
            print(f"          {why}")
    if not any_bad:
        print("  (none)")

    print("\n--- ambiguous ORDER (same platform, same value, one bringup) ---")
    found = False
    per = {}
    for rel, tier, plat, prio, _, _ in rows:
        per.setdefault(rel, {}).setdefault((plat, prio), []).append(tier)
    for rel, seen in sorted(per.items()):
        for (plat, prio), tiers in sorted(seen.items()):
            if len(tiers) > 1:
                found = True
                print(f"  {rel}: {plat} {prio} <- {', '.join(tiers)}")
    if not found:
        print("  (none)")

    print("\n--- ports ---")
    plats = sorted({r[2] for r in rows})
    for plat in plats:
        n = sum(1 for r in rows if r[2] == plat)
        plan = plans.get(plat)
        if plan:
            bands = ", ".join(f"{k} {v[0]}..{v[1]}"
                              for k, v in plan["reserved"].items())
            pool = ", ".join(f"{k} {v[0]}..{v[1]}"
                             for k, v in plan["pool"].items())
            print(f"  {plat:9s} {n:3d} pin(s)  {plan['direction']:18s} "
                  f"reserved: {bands}  pool: {pool}")
            print(f"            plan: {plan['source']}")
        else:
            print(f"  {plat:9s} {n:3d} pin(s)  NO PLAN")
            print(f"            {NO_PLAN_REASON.get(plat, 'undescribed')}")


if __name__ == "__main__":
    main()

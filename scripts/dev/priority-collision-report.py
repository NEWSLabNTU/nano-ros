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
from tracked import tracked  # issue 0721: index lookup, not a walk

ROOT = Path(__file__).resolve().parent.parent.parent

# Per-port address plans AS THEY EXIST TODAY. Every band carries the source it
# was read from — an uncited number here would be the same guess this report
# exists to expose.
PLANS = {
    "freertos": {
        "direction": "bigger-is-urgent",
        "bands": {
            "transport": (4, 4),  # zenoh read + lease + netif poll
        },
        # CORRECTION (2026-08-23): this table also listed `app` at 3..3 as
        # reserved, which produced a COLLIDES verdict for `tiers.mid.freertos =
        # 3`. That was wrong. `app_priority = 3` is the priority app_task is
        # CREATED at, and `run_tiers` immediately replaces it with the boot
        # tier's own (freertos_run_tiers.c `freertos_apply_tier_priority`,
        # entry.rs `nros_freertos_set_current_task_priority`). A starting value
        # is not a standing occupant, so 3 belongs to the POOL and the report
        # over-counted by one. Recorded rather than quietly deleted: the point
        # of citing a source per band is to make exactly this checkable.
        "source": "nros-board-common/src/freertos_config.rs "
                  "(zenoh_read/lease 4, poll 4); `app_priority` 3 is a creation "
                  "value the boot tier overwrites, NOT a reserved band",
    },
    "nuttx": {
        "direction": "bigger-is-urgent",
        "bands": {
            # Not a choice — an inheritance. zenoh-pico's read/lease threads are
            # pthreads created with the session-opening thread's priority, which
            # is the app_main default. Until issue 0736 the port could not state
            # anything else: zpico_set_task_config DISCARDED the priority here.
            "transport (INHERITED, not declared)": (100, 100),
        },
        "source": "SCHED_PRIORITY_DEFAULT via pthread inheritance; "
                  "zpico_set_task_config gained a NuttX arm in issue 0736",
    },
    "zephyr": {
        "direction": "smaller-is-urgent",
        "bands": {},
        "source": "settable since issue 0626 (zpico_posix_set_priority, "
                  "NORMALISED 0-31) but no port declares a band",
    },
    "posix": {
        "direction": "bigger-is-urgent",
        "bands": {},
        "source": "zpico_set_task_config DISCARDS priority on Linux/macOS "
                  "(a hosted privilege concern) — no band exists to collide with",
    },
    "threadx": {
        "direction": "smaller-is-urgent",
        "bands": {},
        "source": "carries the platform ABI's attributes (issue 0626); "
                  "no band declared",
    },
}

TIER_RE = re.compile(r"^\[tiers\.([A-Za-z0-9_]+)\]\s*$")
TIER_PLAT_RE = re.compile(r"^\[tiers\.([A-Za-z0-9_]+)\.([A-Za-z0-9_]+)\]\s*$")
PRIO_RE = re.compile(r"^priority\s*=\s*(-?\d+)")


def parse(path):
    """-> [(tier, platform, priority)] for one system.toml."""
    out, tier, plat = [], None, None
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = TIER_PLAT_RE.match(line)
        if m:
            tier, plat = m.group(1), m.group(2)
            continue
        m = TIER_RE.match(line)
        if m:
            tier, plat = m.group(1), None
            continue
        if line.startswith("["):
            tier = plat = None
            continue
        m = PRIO_RE.match(line)
        if m and tier and plat:
            out.append((tier, plat, int(m.group(1))))
    return out


def more_urgent(a, b, direction):
    return a > b if direction == "bigger-is-urgent" else a < b


def verdict(plat, prio):
    plan = PLANS.get(plat)
    if plan is None:
        return "NO PLAN", f"port {plat!r} is not described at all"
    if not plan["bands"]:
        return "UNPLANNED", "port declares no system band — nothing to allocate around"
    d = plan["direction"]
    worst = None
    for name, (lo, hi) in plan["bands"].items():
        if lo <= prio <= hi:
            return "COLLIDES", f"lands ON the {name} band {lo}..{hi}"
        floor = lo if d == "bigger-is-urgent" else hi
        if more_urgent(prio, floor, d):
            worst = f"PREEMPTS the {name} band ({prio} vs {floor}, {d})"
    return ("PREEMPTS", worst) if worst else ("below bands", "")


def main():
    files = sorted(tracked(ROOT / "examples", name="system.toml"))
    rows, per_bringup = [], {}
    for f in files:
        pins = parse(f)
        if not pins:
            continue
        rel = f.relative_to(ROOT)
        per_bringup[rel] = pins
        for tier, plat, prio in pins:
            v, why = verdict(plat, prio)
            rows.append((rel, tier, plat, prio, v, why))

    print(f"RFC-0079 collision report — {len(rows)} pin(s) over "
          f"{len(per_bringup)} bringup(s)\n")

    tally = {}
    for _, _, _, _, v, _ in rows:
        tally[v] = tally.get(v, 0) + 1
    for v in sorted(tally, key=lambda k: -tally[k]):
        print(f"  {tally[v]:3d}  {v}")

    print("\n--- pins whose verdict is not 'below bands' ---")
    any_bad = False
    for rel, tier, plat, prio, v, why in rows:
        if v in ("below bands",):
            continue
        any_bad = True
        print(f"  [{v}] {rel}: tiers.{tier}.{plat} = {prio}")
        if why:
            print(f"          {why}")
    if not any_bad:
        print("  (none)")

    # Two tiers pinned to the SAME number on one platform have no defined order
    # between them — the allocator would have to invent one.
    print("\n--- ambiguous ORDER (same platform, same value, one bringup) ---")
    found = False
    for rel, pins in per_bringup.items():
        seen = {}
        for tier, plat, prio in pins:
            seen.setdefault((plat, prio), []).append(tier)
        for (plat, prio), tiers in sorted(seen.items()):
            if len(tiers) > 1:
                found = True
                print(f"  {rel}: {plat} {prio} <- {', '.join(tiers)}")
    if not found:
        print("  (none)")

    print("\n--- ports, and what they can say about themselves ---")
    for plat, plan in PLANS.items():
        n = sum(1 for r in rows if r[2] == plat)
        bands = ", ".join(f"{k} {v[0]}..{v[1]}" for k, v in plan["bands"].items())
        print(f"  {plat:9s} {n:3d} pin(s)  {plan['direction']:18s} "
              f"{bands or 'NO BAND DECLARED'}")
        print(f"            source: {plan['source']}")


if __name__ == "__main__":
    main()

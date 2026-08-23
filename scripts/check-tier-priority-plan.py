#!/usr/bin/env python3
"""RFC-0079 — a tier priority pin must respect its port's address plan.

Two checks, and the second is the one that keeps the first honest:

1. **Pins against the plan.** A `[tiers.<name>.<platform>] priority = N` that
   lands in a RESERVED band is an ERROR — it puts an application tier on top of
   a system task that has to keep running for the application to work. A pin
   that is more urgent than a reserved band is a WARNING today: RFC-0079 makes
   that legal only by NAMING the band (`above = "transport"`), and that syntax
   does not exist yet, so warning is the honest state.

2. **The plan against the code.** Every band in a `[board.priority_plan]` is a claim
   about numbers that live in Rust. A plan free to drift from what the port
   actually does would be a second spelling of the same fact — the failure mode
   this codebase keeps paying for. So the transport band is cross-referenced
   against `FreertosScheduling::default()` and `configMAX_PRIORITIES`.

Ports with no `[board.priority_plan]` are REPORTED, not failed: most cannot express
one yet (issue 0736 gave NuttX the ability days ago; Linux/macOS still discard
priority entirely). Silence there would read as approval.

Gate: just check-tier-priority-plan
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # issue 0721: index lookup, not a walk
from priority_plan import load_plans, scan_pins  # RFC-0079: ONE plan reader

ROOT = Path(__file__).resolve().parent.parent

def cross_reference(plans, errors):
    """Check 2 — the plan must match the code it claims to describe."""
    plan = plans.get("freertos")
    if plan is None:
        return
    cfg = (ROOT / "packages/boards/nros-board-common/src/freertos_config.rs")
    text = cfg.read_text(encoding="utf-8")
    vals = {}
    for key in ("zenoh_read_priority", "zenoh_lease_priority", "poll_priority"):
        m = re.search(rf"{key}:\s*(\d+)", text)
        if m:
            vals[key] = int(m.group(1))
    if not vals:
        errors.append(f"{cfg.relative_to(ROOT)}: could not read the transport "
                      "defaults the plan claims to mirror")
        return
    lo, hi = plan["reserved"].get("transport", (None, None))
    for key, v in vals.items():
        if lo is None or not (lo <= v <= hi):
            errors.append(
                f"priority_plan reserved.transport = [{lo}, {hi}] but "
                f"{key} = {v} in {cfg.relative_to(ROOT)} — the plan does not "
                "describe the port it belongs to")

    hdr = ROOT / "packages/boards/nros-board-freertos/config/FreeRTOSConfig.h"
    m = re.search(r"define\s+configMAX_PRIORITIES\s+(\d+)", hdr.read_text(encoding="utf-8"))
    if m and plan.get("range"):
        top = int(m.group(1)) - 1
        if plan["range"][1] > top:
            errors.append(
                f"priority_plan range tops out at {plan['range'][1]} but "
                f"configMAX_PRIORITIES = {m.group(1)} allows at most {top}")


def main():
    plans = load_plans()
    errors, warnings, unplanned = [], [], {}

    cross_reference(plans, errors)

    checked, declared = 0, []
    for rel, tier, plat, prio, above in scan_pins():
        plan = plans.get(plat)
        if plan is None:
            unplanned[plat] = unplanned.get(plat, 0) + 1
            continue
        checked += 1
        where = f"{rel}: tiers.{tier}.{plat} = {prio}"
        bigger = plan.get("direction") == "bigger-is-urgent"
        for name, (lo, hi) in plan["reserved"].items():
            if lo <= prio <= hi:
                errors.append(
                    f"{where} lands ON the reserved `{name}` band "
                    f"[{lo}, {hi}] — an application tier cannot share a "
                    f"priority with a system task it depends on "
                    f"({plan['source']})")
            elif (prio > hi) if bigger else (prio < lo):
                if above == name:
                    # A STATED choice. RFC-0079 §6: both orderings are
                    # legitimate; what was never acceptable is choosing by
                    # accident. Reported, not silent — the consequence is real
                    # and a reader of the build log should see it.
                    declared.append(
                        f"{where} preempts `{name}` [{lo}, {hi}] BY DECLARATION "
                        f"(`above = \"{name}\"`): this tier can outrun the "
                        "link it publishes over, and inbound traffic waits on "
                        "it.")
                else:
                    errors.append(
                        f"{where} is MORE URGENT than the reserved `{name}` "
                        f"band [{lo}, {hi}] and does not say so. A tier that "
                        f"outranks the transport cannot be drained or refilled "
                        f"by it (issue 0623, measured again in 0736).\n"
                        f"      Either move it into the pool "
                        f"{plan['pool']}, or state the choice on the tier:\n"
                        f"          [tiers.{tier}]\n"
                        f"          above = \"{name}\"")

    for d in declared:
        print(f"  DECLARED  {d}")
    for w in warnings:
        print(f"  WARN  {w}")
    if unplanned:
        print("\n  ports with NO [priority_plan], so their pins are unchecked:")
        for plat, n in sorted(unplanned.items()):
            print(f"        {plat:9s} {n:3d} pin(s)")
        print("        (reported, not failed — most cannot express a plan yet)")

    if errors:
        print("\ntier-priority-plan: FAILED")
        for e in errors:
            print(f"  {e}")
        return 1
    print(f"\ntier-priority-plan: OK ({checked} pin(s) checked against "
          f"{len(plans)} declared plan(s), {len(declared)} declared "
          f"preemption(s), {len(warnings)} warning(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())

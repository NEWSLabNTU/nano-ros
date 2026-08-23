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

ROOT = Path(__file__).resolve().parent.parent

TIER_PLAT_RE = re.compile(r"^\[tiers\.([A-Za-z0-9_]+)\.([A-Za-z0-9_]+)\]\s*$")
TIER_RE = re.compile(r"^\[tiers\.([A-Za-z0-9_]+)\]\s*$")
PRIO_RE = re.compile(r"^priority\s*=\s*(-?\d+)")
PAIR_RE = re.compile(r"\[\s*(-?\d+)\s*,\s*(-?\d+)\s*\]")


def load_plans():
    """platform -> plan, from every board descriptor that declares one."""
    plans = {}
    for desc in sorted(tracked(ROOT / "packages/boards", name="nros-board.toml")):
        text = desc.read_text(encoding="utf-8")
        if "[board.priority_plan]" not in text:
            continue
        platform, in_plan = None, False
        plan = {"reserved": {}, "pool": {}, "source": desc.relative_to(ROOT)}
        for raw in text.splitlines():
            line = raw.split("#", 1)[0].strip()
            if not line:
                continue
            if line.startswith("platform =") and platform is None:
                platform = line.split("=", 1)[1].strip().strip('"')
            if line == "[board.priority_plan]":
                in_plan = True
                continue
            if in_plan and line.startswith("["):
                in_plan = False
            if not in_plan:
                continue
            if line.startswith("direction"):
                plan["direction"] = line.split("=", 1)[1].strip().strip('"')
            elif line.startswith("range"):
                m = PAIR_RE.search(line)
                plan["range"] = (int(m.group(1)), int(m.group(2)))
            elif line.startswith("reserved."):
                name = line.split("=", 1)[0].strip().split(".", 1)[1]
                m = PAIR_RE.search(line)
                plan["reserved"][name] = (int(m.group(1)), int(m.group(2)))
            elif line.startswith("pool."):
                name = line.split("=", 1)[0].strip().split(".", 1)[1]
                m = PAIR_RE.search(line)
                plan["pool"][name] = (int(m.group(1)), int(m.group(2)))
        if platform:
            plans[platform] = plan
    return plans


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

    checked = 0
    for f in sorted(tracked(ROOT / "examples", name="system.toml")):
        tier = plat = None
        for raw in f.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            m = TIER_PLAT_RE.match(line)
            if m:
                tier, plat = m.group(1), m.group(2)
                continue
            if TIER_RE.match(line):
                tier, plat = TIER_RE.match(line).group(1), None
                continue
            if line.startswith("["):
                tier = plat = None
                continue
            m = PRIO_RE.match(line)
            if not (m and tier and plat):
                continue
            prio = int(m.group(1))
            plan = plans.get(plat)
            if plan is None:
                unplanned[plat] = unplanned.get(plat, 0) + 1
                continue
            checked += 1
            where = f"{f.relative_to(ROOT)}: tiers.{tier}.{plat} = {prio}"
            bigger = plan.get("direction") == "bigger-is-urgent"
            for name, (lo, hi) in plan["reserved"].items():
                if lo <= prio <= hi:
                    errors.append(
                        f"{where} lands ON the reserved `{name}` band "
                        f"[{lo}, {hi}] — an application tier cannot share a "
                        f"priority with a system task it depends on "
                        f"({plan['source']})")
                elif (prio > hi) if bigger else (prio < lo):
                    warnings.append(
                        f"{where} is MORE URGENT than the reserved `{name}` "
                        f"band [{lo}, {hi}] — this tier preempts it. RFC-0079 "
                        f"makes this legal only by naming it "
                        f"(`above = \"{name}\"`); that syntax does not exist "
                        "yet, so this is a warning.")

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
          f"{len(plans)} declared plan(s), {len(warnings)} warning(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())

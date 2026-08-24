#!/usr/bin/env python3
"""RFC-0079 §4.1 — check a DERIVED priority plan against ONE built image.

`check-tier-priority-plan` judges ports whose reserved band is a literal. Zephyr's
is computed from Kconfig, per image, so that checker defers and this one finishes
the job: it resolves the band from a real `.config` and evaluates the tier pins
against it.

Two things it must not do, both of which would defeat the point:

  * Guess a `.config`. Without one there is nothing to resolve, and a default
    would be a literal band by another name.
  * Treat "the priority is never applied in this image" as a pass. If the
    Kconfig gates are off, the transport INHERITS its creator and there IS no
    band — that is the NuttX pre-0736 state, and it is reported as such.

Usage:
    python3 scripts/check-tier-priority-plan-image.py <path/to/zephyr/.config> [tier_key]
"""

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from priority_plan import load_plans, scan_pins, resolve_zephyr_plan

RESOLVERS = {"zephyr": resolve_zephyr_plan}


def discover():
    """Every built Zephyr image's `.config`, newest first.

    With no argument the checker examines EVERY image this tree has built
    rather than one the caller happened to name. A derived band is a property
    of an IMAGE, so "the plan holds" is a claim about all of them, and letting
    the caller pick which one to prove is how a green comes to mean less than
    it looks.
    """
    ws = os.environ.get("NROS_ZEPHYR_WORKSPACE") or ""
    roots = [Path(ws)] if ws else []
    roots += [ROOT / "zephyr-workspace", ROOT.parent / "nano-ros-workspace"]
    found = []
    for r in roots:
        if not r.is_dir():
            continue
        found += sorted(r.glob("build-*/zephyr/.config"))
        if found:
            break
    return found


def check_one(dotconfig, tier_key, plans):
    plan = plans.get(tier_key)
    resolved = RESOLVERS[plan["derived"]](dotconfig)
    if "error" in resolved:
        print(f"  cannot resolve {dotconfig}: {resolved['error']}")
        return 2, 0
    if "unapplied" in resolved:
        # Not a pass and not a failure: the image applies no band at all, so
        # there is nothing for a pin to collide with. Reported so it cannot be
        # mistaken for a checked image (issue 0766).
        print(f"  [NO BAND] {dotconfig.parent.parent.name}: {resolved['unapplied']}")
        return 0, 0
    lo, hi = resolved["reserved"]["transport"]
    plo, phi = resolved["pool"]["app"]
    errs, ok = [], 0
    for rel, tier, plat, prio, above in scan_pins():
        if plat != tier_key:
            continue
        where = f"{rel}: tiers.{tier}.{plat} = {prio}"
        if lo <= prio <= hi:
            errs.append(f"{where} lands ON the reserved transport band [{lo}, {hi}]")
        elif prio < lo and above != "transport":
            errs.append(
                f"{where} is MORE URGENT than the transport band [{lo}, {hi}] and does "
                f"not say so.\n        Move it into pool.app [{plo}, {phi}], or state "
                f'the choice with `above = "transport"` on [tiers.{tier}].')
        else:
            ok += 1
    name = dotconfig.parent.parent.name
    if errs:
        print(f"  [FAIL] {name}: transport [{lo}, {hi}], pool [{plo}, {phi}]")
        for e in errs:
            print(f"        {e}")
        return 1, ok
    print(f"  [ok]   {name}: transport [{lo}, {hi}], pool [{plo}, {phi}] — {ok} pin(s)")
    return 0, ok


def main(argv):
    plans = load_plans()
    tier_key = "zephyr"
    if len(argv) < 2:
        # Lane mode: check every image this tree has built. A tree with no
        # Zephyr workspace SKIPS loudly rather than passing — issue 0599's
        # rule, and the difference between "checked nothing" and "checked".
        configs = discover()
        if not configs:
            print("check-tier-priority-plan-image: SKIPPED — no built Zephyr image "
                  "found (run `just zephyr setup` then `just zephyr build-fixtures`).")
            print("  The DEFERRED pins reported by check-tier-priority-plan stay "
                  "unchecked on this host.")
            return 0
        plan = plans.get(tier_key)
        if plan is None or not plan.get("derived"):
            print(f"{tier_key!r} has no DERIVED plan")
            return 2
        rc, total = 0, 0
        print(f"check-tier-priority-plan-image: {len(configs)} built image(s)")
        for c in configs:
            r, n = check_one(c, tier_key, plans)
            rc = max(rc, r)
            total += n
        print(f"\ntier-priority-plan-image: {'FAILED' if rc else 'OK'} "
              f"({total} pin-check(s) over {len(configs)} image(s))")
        return rc
    dotconfig = Path(argv[1])
    tier_key = argv[2] if len(argv) > 2 else "zephyr"
    if not dotconfig.is_file():
        print(f"no such .config: {dotconfig}")
        return 2

    plans = load_plans()
    plan = plans.get(tier_key)
    if plan is None or not plan.get("derived"):
        print(f"{tier_key!r} has no DERIVED plan — use check-tier-priority-plan")
        return 2

    resolved = RESOLVERS[plan["derived"]](dotconfig)
    if "error" in resolved:
        print(f"cannot resolve: {resolved['error']}")
        return 2
    if "unapplied" in resolved:
        print(f"tier-priority-plan-image ({tier_key}): NO BAND in this image")
        print(f"  {resolved['unapplied']}")
        print(f"  from: {resolved['derived_from']}")
        return 1

    d = resolved["detail"]
    print(f"resolved from {resolved['derived_from']}")
    for name, v in sorted(d.items()):
        print(f"  {name:6s} band {v['band']:3d} -> posix {v['posix']:3d} "
              f"-> k_thread {v['kthread']:3d}")
    lo, hi = resolved["reserved"]["transport"]
    plo, phi = resolved["pool"]["app"]
    print(f"  reserved.transport = [{lo}, {hi}]   pool.app = [{plo}, {phi}]   "
          f"range = {resolved['range']}")

    errors, ok = [], 0
    for rel, tier, plat, prio, above in scan_pins():
        if plat != tier_key:
            continue
        where = f"{rel}: tiers.{tier}.{plat} = {prio}"
        if lo <= prio <= hi:
            errors.append(f"{where} lands ON the reserved transport band "
                          f"[{lo}, {hi}] resolved for this image")
        elif prio < lo:  # smaller-is-urgent
            if above == "transport":
                print(f"  DECLARED  {where} preempts transport by declaration")
            else:
                errors.append(
                    f"{where} is MORE URGENT than the transport band [{lo}, {hi}] "
                    f"and does not say so.\n"
                    f"      Move it into pool.app [{plo}, {phi}], or state the "
                    f'choice with `above = "transport"` on [tiers.{tier}].')
        else:
            ok += 1

    if errors:
        print(f"\ntier-priority-plan-image ({tier_key}): FAILED")
        for e in errors:
            print(f"  {e}")
        return 1
    print(f"\ntier-priority-plan-image ({tier_key}): OK ({ok} pin(s) "
          f"checked against this image)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

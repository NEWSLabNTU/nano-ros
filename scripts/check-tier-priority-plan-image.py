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
  * Judge an image the tree has moved past. A `.config` configured before a
    nano-ros default changed resolves the OLD band, and a verdict on it is a
    verdict about a museum. In discovery mode it is reported `[STALE]` and
    never judged — and a workspace where nothing was judged and something is
    stale FAILS, because "checked nothing current" must not read as "checked".
    In `--images-from` mode the caller BUILT it, so a stale one FAILS. The
    rule and its measurements live beside `stale_band_reasons` in
    `scripts/lib/priority_plan.py`.

Usage:
    python3 scripts/check-tier-priority-plan-image.py <path/to/zephyr/.config> [tier_key]
    python3 scripts/check-tier-priority-plan-image.py --images-from <list> [tier_key]
    python3 scripts/check-tier-priority-plan-image.py            # discovery
    python3 scripts/check-tier-priority-plan-image.py --selftest
"""

import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from priority_plan import (BAND_DEFAULTED, kconfig_default, load_plans,
                           resolve_zephyr_plan, scan_pins, stale_band_reasons)

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
    reasons = stale_band_reasons(dotconfig)
    if reasons:
        print(f"  [STALE]   {dotconfig.parent.parent.name}: not evidence about this tree")
        for r in reasons:
            print(f"        {r}")
        return 0, 0, "stale"
    resolved = RESOLVERS[plan["derived"]](dotconfig)
    if "error" in resolved:
        print(f"  cannot resolve {dotconfig}: {resolved['error']}")
        return 2, 0, "error"
    if "unapplied" in resolved:
        # Not a pass and not a failure: the image applies no band at all, so
        # there is nothing for a pin to collide with. Reported so it cannot be
        # mistaken for a checked image (issue 0766).
        print(f"  [NO BAND] {dotconfig.parent.parent.name}: {resolved['unapplied']}")
        return 0, 0, "noband"
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
        return 1, ok, "judged"
    print(f"  [ok]   {name}: transport [{lo}, {hi}], pool [{plo}, {phi}] — {ok} pin(s)")
    return 0, ok, "judged"


def images_from(listing):
    """`.config` paths for the build dirs named in `listing`, one per line.

    The LANE's mode. `discover()` answers "what does this workspace hold",
    which on a shared or long-lived workspace includes images no run of this
    tree produced — and a derived band read off a museum `.config` fails pins
    that pass against every image the run actually built. Tier 2 was red three
    nights on exactly that (images still carrying the pre-0852 0-31 zenoh band,
    transport [14, 14]). The lane knows which leaves it built, so it says so.

    A listed dir with no `.config` is an ERROR, not a skip: the lane only gets
    here after every listed leaf built, so a missing `.config` means the list
    and the build disagree, and silently checking fewer images is how a green
    comes to mean less than it looks.
    """
    configs, missing = [], []
    for raw in Path(listing).read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line:
            continue
        p = Path(line)
        cfg = p if p.name == ".config" else p / "zephyr" / ".config"
        (configs if cfg.is_file() else missing).append(cfg)
    return configs, missing


def check_many(configs, tier_key, plans, origin, listed):
    """Judge `configs`; `listed` says whether the caller BUILT them.

    A STALE image means different things in the two modes. Discovered, it is
    residue the workspace happens to hold: reported, never judged. Listed, the
    caller just built it, so a `.config` still carrying an old default means the
    build did not re-run configure after the default moved — a missing edge,
    and a failure, not residue.
    """
    plan = plans.get(tier_key)
    if plan is None or not plan.get("derived"):
        print(f"{tier_key!r} has no DERIVED plan")
        return 2
    if not selftest():
        return 1
    rc, total, failed = 0, 0, []
    states = {"judged": 0, "stale": 0, "noband": 0, "error": 0}
    print(f"check-tier-priority-plan-image: {len(configs)} image(s) ({origin})")
    for c in configs:
        r, n, state = check_one(c, tier_key, plans)
        if state == "stale" and listed:
            r = 1
        if r:
            failed.append(c.parent.parent.name)
        rc = max(rc, r)
        total += n
        states[state] += 1
    if states["stale"] and listed:
        print(f"\n  {states['stale']} image(s) this run BUILT still carry an old "
              f"default: the build did not re-run configure after it moved. "
              f"`cmake <build-dir>` re-runs it in place.")
    elif states["stale"] and not states["judged"]:
        # issue 0599's rule one step on: "checked nothing" must not read as
        # "checked". Nothing here was judged, and the images that COULD
        # carry a band are the stale ones — the rest apply none — so
        # rebuilding them is what would produce evidence. A precondition,
        # and unmet preconditions fail.
        print(f"\n  no image was judged: the {states['stale']} that could carry "
              f"a band are STALE, and the other {states['noband']} apply none. "
              f"Rebuild the stale ones (`just zephyr build-fixtures`), or "
              f"`cmake <build-dir>` to re-run configure in place.")
        rc = max(rc, 1)
    verdict = "FAILED" if rc else "OK"
    print(f"\ntier-priority-plan-image: {verdict} "
          f"({total} pin-check(s) over {states['judged']} current image(s); "
          f"{states['stale']} STALE, {states['noband']} NO BAND)")
    if failed:
        # Named in the LAST lines on purpose: a failing lane prints the tail of
        # this output, and the per-image [FAIL] blocks are above it. The tier-2
        # log showed forty `[ok]` lines and a bare FAILED, with every failing
        # image scrolled off.
        print(f"  failing image(s): {', '.join(failed)}")
    return rc


def main(argv):
    if len(argv) > 1 and argv[1] == "--selftest":
        return 0 if selftest() else 1
    plans = load_plans()
    tier_key = "zephyr"
    if len(argv) >= 3 and argv[1] == "--images-from":
        configs, missing = images_from(argv[2])
        if len(argv) > 3:
            tier_key = argv[3]
        if missing:
            print("check-tier-priority-plan-image: listed image(s) have no .config:")
            for m in missing:
                print(f"  {m}")
            return 2
        if not configs:
            print(f"check-tier-priority-plan-image: {argv[2]} names no image — "
                  "the caller built nothing, so there is nothing to check")
            return 2
        return check_many(configs, tier_key, plans, f"listed in {argv[2]}",
                          listed=True)
    if len(argv) < 2:
        # Discovery mode (the operator's `just check tier-priority-plan-image`):
        # every image the workspace holds. A tree with no Zephyr workspace
        # SKIPS loudly rather than passing — issue 0599's rule, and the
        # difference between "checked nothing" and "checked". Lanes do NOT use
        # this mode: they name the images they built (`--images-from`).
        configs = discover()
        if not configs:
            print("check-tier-priority-plan-image: SKIPPED — no built Zephyr image "
                  "found (run `just zephyr setup` then `just zephyr build-fixtures`).")
            print("  The DEFERRED pins reported by check-tier-priority-plan stay "
                  "unchecked on this host.")
            return 0
        return check_many(configs, tier_key, plans, "every image in the workspace",
                          listed=False)
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

    reasons = stale_band_reasons(dotconfig)
    if reasons:
        print(f"tier-priority-plan-image ({tier_key}): STALE image — cannot judge "
              f"it against this tree")
        for r in reasons:
            print(f"  {r}")
        print(f"  `cmake {dotconfig.parent.parent}` re-runs configure from the "
              f"cached settings and regenerates the .config in place.")
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


def selftest():
    """The staleness rule's own negative control, run on the normal path.

    This rule fails SILENT if it breaks: `kconfig_default` returning None turns
    every comparison off and every image back into a judged one. So the REAL
    zephyr/Kconfig must yield an integer for both band inputs, and the synthetic
    cases must come out the way they are written — including the three
    "cannot tell" cases that must answer "not stale".
    """
    import tempfile
    fails = []

    def expect(name, got, want):
        if got != want:
            fails.append(f"{name}: got {got!r}, want {want!r}")

    for key in BAND_DEFAULTED:
        v = kconfig_default(key)
        if not isinstance(v, int):
            fails.append(f"real zephyr/Kconfig: no integer default for {key} "
                         f"(got {v!r}) — the stale check would be silently OFF")

    with tempfile.TemporaryDirectory() as tmp:
        d = Path(tmp)
        kc = d / "Kconfig"
        kc.write_text(
            "config NROS_ZENOH_READ_PRIORITY\n"
            "    int \"read\"\n"
            "    default 200\n"
            "    help\n"
            "      default 999 is help text, not an attribute.\n"
            "\n"
            "config NROS_ZENOH_LEASE_PRIORITY\n"
            "    int \"lease\"\n"
            "    default 100 if SOMETHING\n"
            "    default 200\n")
        expect("a plain default",
               kconfig_default("CONFIG_NROS_ZENOH_READ_PRIORITY", kc), 200)
        expect("a conditional default is not one fact",
               kconfig_default("CONFIG_NROS_ZENOH_LEASE_PRIORITY", kc), None)

        def image(name, dotconfig, confs, cache=True):
            b = d / name
            (b / "zephyr").mkdir(parents=True)
            (b / "zephyr" / ".config").write_text(dotconfig)
            app = d / f"{name}-app"
            app.mkdir()
            for fn, body in confs.items():
                (app / fn).write_text(body)
            if cache:
                (b / "CMakeCache.txt").write_text(
                    f"APPLICATION_SOURCE_DIR:PATH={app}\n"
                    f"CONF_FILE:STRING={';'.join(confs)}\n")
            return b / "zephyr" / ".config"

        READ16 = "CONFIG_NROS_ZENOH_READ_PRIORITY=16\n"
        expect("a value equal to the default is current",
               stale_band_reasons(image("cur", "CONFIG_NROS_ZENOH_READ_PRIORITY=200\n",
                                        {"prj.conf": ""}), kc), [])
        expect("an old default that no conf sets is STALE",
               len(stale_band_reasons(image("old", READ16,
                                            {"prj.conf": "CONFIG_FOO=y\n"}), kc)), 1)
        expect("a conf that sets it is an OVERRIDE, not staleness",
               stale_band_reasons(image("ovr", READ16, {"prj.conf": READ16}), kc), [])
        expect("no CMakeCache is UNKNOWN, never stale",
               stale_band_reasons(image("unk", READ16, {"prj.conf": ""}, cache=False), kc), [])
        expect("an absent symbol has nothing to compare",
               stale_band_reasons(image("abs", "CONFIG_FOO=y\n", {"prj.conf": ""}), kc), [])
        expect("no single default means no comparison",
               stale_band_reasons(image("cond", "CONFIG_NROS_ZENOH_LEASE_PRIORITY=16\n",
                                        {"prj.conf": ""}), kc), [])

    if fails:
        print("check-tier-priority-plan-image --selftest: FAILED")
        for f in fails:
            print(f"  {f}")
        return False
    print("check-tier-priority-plan-image --selftest: OK "
          "(real zephyr/Kconfig parses; 8 synthetic cases)")
    return True


if __name__ == "__main__":
    sys.exit(main(sys.argv))

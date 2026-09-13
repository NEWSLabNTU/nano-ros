#!/usr/bin/env python3
"""Every platform name the TREE uses must be answered by some `nros-platform.toml`.

WHY THIS EXISTS (issue 1145, the 2026-09-12 regression).

`names` in a platform descriptor is the list of names that platform answers to;
the directory it lives in is just where the file sits. `nros-platform-threadx`
declared `names = ["threadx"]` while `examples/fixtures.toml` only ever spells
`threadx-linux` and `threadx-riscv64`. So every threadx build asked for a name
no descriptor answered.

That is NOT a hard error by design. `or_builtin_rungs` turns an
`UnknownPlatform` into a warning plus the builtin knob defaults, deliberately,
so that a TYPO in `NROS_PLATFORM_NAME` stays visible instead of silently
selecting builtins. Right for a typo — and wrong for a platform that simply
never declared its own name, because the builtins it falls through to are not
that platform's numbers. The builtin executor knobs sit below
`EXECUTOR_BACKING_DEFAULT_U64S`, so the backing guard then refused the
reservation and `threadx-linux` stopped COMPILING, in a nightly, with an error
naming neither the descriptor nor the name.

A `cargo:warning` is exactly the shape nobody reads: it scrolls past a green
build for as long as the fall-through happens to be survivable. This asks the
question once, cheaply, at the one moment it is still cheap to answer.

It is deliberately BUILDLESS — TOML and a manifest read, no cargo — so it runs
on the fast line and does not depend on a build succeeding.
"""

import os
import re
import sys

try:
    import tomllib
except ModuleNotFoundError:  # py<3.11
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(ROOT, "examples", "fixtures.toml")
PLATFORM_DIR = os.path.join(ROOT, "packages", "platform")


def declared_names(platform_dir=PLATFORM_DIR):
    """Every name any descriptor answers to -> the file that declares it."""
    out = {}
    if not os.path.isdir(platform_dir):
        return out
    for entry in sorted(os.listdir(platform_dir)):
        path = os.path.join(platform_dir, entry, "nros-platform.toml")
        if not os.path.isfile(path):
            continue
        with open(path, "rb") as fh:
            doc = tomllib.load(fh)
        for name in doc.get("names", []):
            out[name] = os.path.relpath(path, ROOT)
    return out


def used_names(fixtures=FIXTURES):
    """Every `platform = "X"` a fixture row names."""
    if not os.path.isfile(fixtures):
        return set()
    with open(fixtures, encoding="utf8") as fh:
        return set(re.findall(r'^\s*platform\s*=\s*"([^"]+)"', fh.read(), re.M))


# Names that are NOT platform-descriptor names. `native` is a ROLE (this is the
# host build), not a software stack; CLAUDE.md's Naming section is explicit that
# it is not a synonym for a reach. A row spelling one of these is answered by
# the host path, which has no descriptor to find.
NOT_A_DESCRIPTOR_NAME = {"native", "linux"}

# A RATCHET, not an allowlist: this set may only SHRINK (issue 1362).
#
# These five fall through to the builtin knobs today — MEASURED, three
# `cargo:warning` lines each (executor, params, memory), the same three
# `or_builtin_rungs` call sites. They are baselined rather than fixed in the
# same commit as threadx, because "add the name to a descriptor" is not a
# no-op: it swaps builtin knob values for that descriptor's, and doing that to
# four platforms on the strength of an assumption is how one fixed regression
# becomes four new ones. `esp32` and `baremetal` have no platform package at
# all, so for them it is not even a name — it is a decision.
BASELINE_UNANSWERED = {
    "baremetal",
    "esp32",
    "freertos-posix",
    "nuttx-riscv",
    "zephyr-cortex-m",
}


def findings(used, declared, baseline=frozenset()):
    return sorted(
        n for n in used
        if n not in declared and n not in NOT_A_DESCRIPTOR_NAME and n not in baseline
    )


def stale_baseline(used, declared, baseline):
    """Baselined names that now resolve — the ratchet must tighten."""
    return sorted(n for n in baseline if n in declared or n not in used)


def self_test():
    """A gate nobody has watched fail reads exactly like one that passes."""
    cases = [
        # (used, declared, expected findings)
        ({"threadx-linux"}, {"threadx": "f"}, ["threadx-linux"]),
        ({"threadx-linux"}, {"threadx-linux": "f"}, []),
        ({"freertos", "freertos-lwip"}, {"freertos": "f", "freertos-lwip": "f"}, []),
        ({"native"}, {}, []),           # a ROLE, never a descriptor name
        ({"zephyr", "nuttx"}, {"zephyr": "f"}, ["nuttx"]),
    ]
    # The ratchet: a baselined name is silent, and stops being baselined the
    # moment a descriptor answers it.
    if findings({"esp32"}, {}, {"esp32"}) != []:
        print("  self-test FAIL: a baselined name should not be reported")
        return 1
    if stale_baseline({"esp32"}, {"esp32": "f"}, {"esp32"}) != ["esp32"]:
        print("  self-test FAIL: a baselined name that now resolves must be reported stale")
        return 1
    bad = 0
    for used, declared, want in cases:
        got = findings(used, declared)
        if got != want:
            print(f"  self-test FAIL: used={used} declared={set(declared)} -> {got}, want {want}")
            bad += 1
    print(f"check-platform-name-answered self-test: {'OK' if not bad else 'FAILED'} "
          f"({len(cases)} cases)")
    return bad


def main():
    if "--self-test" in sys.argv:
        return 1 if self_test() else 0
    if self_test():
        return 1

    declared = declared_names()
    used = used_names()
    stale = stale_baseline(used, declared, BASELINE_UNANSWERED)
    if stale:
        print("check-platform-name-answered: FAIL — the baseline is STALE.", file=sys.stderr)
        for name in stale:
            print(f"  {name!r} no longer needs baselining; drop it from "
                  f"BASELINE_UNANSWERED.", file=sys.stderr)
        print("  A ratchet that does not tighten stops being one.", file=sys.stderr)
        return 1

    bad = findings(used, declared, BASELINE_UNANSWERED)
    if bad:
        print("check-platform-name-answered: FAIL — platform name(s) no descriptor answers to:",
              file=sys.stderr)
        for name in bad:
            print(f"  {name!r} is used by a fixture row, and no "
                  f"packages/platform/*/nros-platform.toml lists it in `names`.", file=sys.stderr)
        print("\n  An unanswered name is NOT fatal at build time — it warns and falls", file=sys.stderr)
        print("  through to the BUILTIN knob defaults, which are not that platform's", file=sys.stderr)
        print("  numbers. That is how `threadx-linux` stopped compiling (issue 1145).", file=sys.stderr)
        print("  Add the name to that platform's `names`, the way `freertos` carries", file=sys.stderr)
        print("  `freertos-lwip`. names[0] stays canonical.", file=sys.stderr)
        return 1
    print(f"check-platform-name-answered: OK ({len(used)} platform name(s) in fixtures, "
          f"{len(declared)} answered by a descriptor, "
          f"{len(BASELINE_UNANSWERED)} baselined — issue 1362)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

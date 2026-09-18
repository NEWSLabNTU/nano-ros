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

WHICH NAMES IT READS, and why that changed (issue 1362, corrected 2026-09-18).

This gate first read `platform = "…"` out of `examples/fixtures.toml`. That is
the wrong population, and it put three false entries in the baseline below.

A fixture row's `platform` is a COORDINATE LABEL — it names the lane cell and
feeds `build_subdir` — and it is not what any build looks a descriptor up by.
`NROS_PLATFORM_NAME` is emitted by `nros ws board-facts` from
`descriptor.platform` (`nros-cli-core/src/cmd/board_facts.rs`), i.e. the
`platform = "…"` a BOARD declares. The two disagree on purpose:

    fixtures row              board declares      reaches the lookup
    platform = "freertos-posix"   freertos            freertos
    platform = "nuttx-riscv"      nuttx               nuttx
    platform = "zephyr-cortex-m"  zephyr              zephyr

MEASURED through the live path, not read off the files:

    $ nros ws board-facts …/c/src/demo_bringup --board freertos-posix
    NROS_PLATFORM_NAME=freertos
    $ nros ws board-facts …/realtime-cpp/src/demo_bringup --board rv-virt-nuttx
    NROS_PLATFORM_NAME=nuttx
    $ nros ws board-facts …/features/src/demo_bringup --board zephyr
    NROS_PLATFORM_NAME=zephyr

So none of those three ever fell through, and baselining them recorded work
that did not exist. The original measurement looked convincing and was
circular: it EXPORTED `NROS_PLATFORM_NAME=freertos-posix` by hand and observed
`or_builtin_rungs` warn — which proves the lookup answers an unknown name, not
that any build ever asks it one.

It still catches issue 1145: `nros-board-threadx-linux` declares
`platform = "threadx-linux"`, so that name IS in this population, and reverting
the threadx descriptor fix reports both threadx names again.
"""

import os
import re
import sys

try:
    import tomllib
except ModuleNotFoundError:  # py<3.11
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PLATFORM_DIR = os.path.join(ROOT, "packages", "platform")
BOARDS_DIR = os.path.join(ROOT, "packages", "boards")


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


def looked_up_names(boards_dir=BOARDS_DIR):
    """Every `platform = "X"` a BOARD declares -> the board files that declare it.

    This is the population `NROS_PLATFORM_NAME` is drawn from, so it is the
    population a descriptor has to answer. See the module docstring for why it
    is not `examples/fixtures.toml`.
    """
    out = {}
    if not os.path.isdir(boards_dir):
        return out
    for entry in sorted(os.listdir(boards_dir)):
        path = os.path.join(boards_dir, entry, "nros-board.toml")
        if not os.path.isfile(path):
            continue
        with open(path, "rb") as fh:
            doc = tomllib.load(fh)
        # `[[board]]` is an ARRAY of tables: one file may describe several
        # boards, and they need not share a platform. Reading a top-level
        # `platform` key finds nothing here — every declaration is inside an
        # element.
        for board in doc.get("board", []):
            if not isinstance(board, dict):
                continue
            name = board.get("platform")
            if isinstance(name, str) and name:
                rel = os.path.relpath(path, ROOT)
                if rel not in out.setdefault(name, []):
                    out[name].append(rel)
    return out


# Names that are NOT platform-descriptor names. `native` is a ROLE (this is the
# host build), not a software stack; CLAUDE.md's Naming section is explicit that
# it is not a synonym for a reach.
#
# EMPTY since the population became board-declared (issue 1362). It held
# `{"native", "linux"}` to excuse fixture ROWS that spelled a role in their
# coordinate label. No board declares either — `packages/boards/linux` declares
# `platform = "posix"`, correctly — and a board that did would be making the
# very claim CLAUDE.md forbids, which this gate should report rather than
# excuse. Kept as a named seam, not deleted, because the next reader will
# otherwise re-derive the question.
NOT_A_DESCRIPTOR_NAME = frozenset()

# A RATCHET, not an allowlist: this set may only SHRINK (issue 1362).
#
# TWO entries, down from five. The other three — `freertos-posix`,
# `nuttx-riscv`, `zephyr-cortex-m` — were never fall-throughs at all: they are
# fixture COORDINATE labels, and their boards declare `freertos` / `nuttx` /
# `zephyr`, each already answered. See the module docstring for the measurement
# that retired them.
#
# These two are real. Each is a `platform = "…"` a board declares and no
# descriptor lists, so their builds take the BUILTIN knobs — three
# `cargo:warning` lines each (executor, params, memory), the three
# `or_builtin_rungs` call sites:
#
#   esp32       packages/boards/nros-board-esp32-qemu/nros-board.toml
#   bare-metal  packages/boards/nros-board-mps2-an385/nros-board.toml
#
# Neither has a platform package, so neither is a missing NAME — it is a
# decision: does it get a descriptor, or is falling through to builtins the
# correct answer for a target with no RTOS to describe? `bare-metal` is the
# clearer case for "correct as is"; `esp32` has an RTOS (ESP-IDF's FreeRTOS)
# and so probably wants one. Both need their knobs compared on a built image
# before either is claimed.
#
# NOTE the spelling. The board declares `bare-metal`; `examples/fixtures.toml`
# labels the same rows `baremetal`. The old baseline carried the fixtures
# spelling, so even its one real entry named a string this population never
# produces.
BASELINE_UNANSWERED = {
    "bare-metal",
    "esp32",
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
        # issue 1145, the catch this gate exists for: the threadx descriptor
        # answering only to "threadx" while a board declares "threadx-linux".
        ({"threadx-linux"}, {"threadx": "f"}, ["threadx-linux"]),
        ({"threadx-linux"}, {"threadx-linux": "f"}, []),
        ({"freertos", "freertos-lwip"}, {"freertos": "f", "freertos-lwip": "f"}, []),
        ({"zephyr", "nuttx"}, {"zephyr": "f"}, ["nuttx"]),
        # issue 1362's correction: a fixture COORDINATE label is not in this
        # population at all, so it can never be reported. `freertos-posix` is a
        # fixtures label whose board declares `freertos`; what the gate sees is
        # the board's name, and that one is answered.
        ({"freertos"}, {"freertos": "f"}, []),
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
    used_map = looked_up_names()
    used = set(used_map)
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
            where = ", ".join(used_map.get(name, []))
            print(f"  {name!r} is declared by {where or 'a board'}, and no "
                  f"packages/platform/*/nros-platform.toml lists it in `names`.", file=sys.stderr)
        print("\n  An unanswered name is NOT fatal at build time — it warns and falls", file=sys.stderr)
        print("  through to the BUILTIN knob defaults, which are not that platform's", file=sys.stderr)
        print("  numbers. That is how `threadx-linux` stopped compiling (issue 1145).", file=sys.stderr)
        print("  Add the name to that platform's `names`, the way `freertos` carries", file=sys.stderr)
        print("  `freertos-lwip`. names[0] stays canonical.", file=sys.stderr)
        return 1
    answered = sum(1 for n in used if n in declared)
    print(f"check-platform-name-answered: OK ({len(used)} platform name(s) declared by "
          f"boards: {answered} answered, {len(BASELINE_UNANSWERED)} baselined — issue 1362; "
          f"descriptors answer to {len(declared)} name(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())

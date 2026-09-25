#!/usr/bin/env python3
"""Every platform name the TREE uses must be answered by some `nros-platform.toml`.

WHY THIS EXISTS (issue 1145, the 2026-09-12 regression).

`names` in a platform descriptor is the list of names that platform answers to;
the directory it lives in is just where the file sits. `nros-platform-threadx`
declared `names = ["threadx"]` while `examples/fixtures.toml` only ever spells
`threadx-linux` and `threadx-riscv64`. So every threadx build asked for a name
no descriptor answered.

That USED NOT TO BE a hard error, by design. `or_builtin_rungs` turned an
`UnknownPlatform` into a warning plus the builtin knob defaults, deliberately,
so that a TYPO in `NROS_PLATFORM_NAME` stayed visible instead of silently
selecting builtins. Right for a typo — and wrong for a platform that simply
never declared its own name, because the builtins it falls through to are not
that platform's numbers. The builtin executor knobs sit below
`EXECUTOR_BACKING_DEFAULT_U64S`, so the backing guard then refused the
reservation and `threadx-linux` stopped COMPILING, in a nightly, with an error
naming neither the descriptor nor the name.

A `cargo:warning` is exactly the shape nobody reads: it scrolls past a green
build for as long as the fall-through happens to be survivable. This asks the
question once, cheaply, at the one moment it is still cheap to answer.

IT IS A HARD ERROR NOW (phase-468 W1, 2026-09-25). `BuildRungs::require_rungs`
— what `or_builtin_rungs` became — panics on an `UnknownPlatform`, naming the
platform, every root it searched, the names the tree does answer, and the
remedy. That is only safe because this gate's population is fully answered, so
the two land together: the gate is what keeps the panic unreachable in a green
tree, and the panic is what makes a red gate mean something. A warning that
nobody reads has been replaced by a build that stops.

It is deliberately BUILDLESS — TOML and a manifest read, no cargo — so it runs
on the fast line and does not depend on a build succeeding.

WHICH NAMES IT READS, and why that changed (issue 1362, corrected 2026-09-18).

This gate first read `platform = "…"` out of `examples/fixtures.toml`. That is
the wrong population, and it put three false entries in the baseline this
gate used to carry (see where that baseline lived, further down).

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
import tempfile

try:
    import tomllib
except ModuleNotFoundError:  # py<3.11
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PLATFORM_DIR = os.path.join(ROOT, "packages", "platform")
# The loader searches TWO roots, and this gate used to scan one.
#
# `PlatformsTree::default_search_path` is `$NROS_PLATFORMS_DIR` (if set), then
# `packages/platform`, then `config` — so `config/bare-metal/nros-platform.toml`
# (`names = ["bare-metal"]`, since phase-349 W1) answers that name at runtime.
# Reading only the first root made this gate report it UNANSWERED, and the
# repair was to write it into `BASELINE_UNANSWERED` rather than to widen the
# reach (issue 1486).
#
# The second-order effect is worse than the false entry. `stale_baseline()`
# compares the baseline against `declared`, and `declared` came from the same
# narrow root — so the ratchet could never observe `bare-metal` becoming
# answered, and the one mechanism meant to retire a baseline row could not fire
# for the row that was wrong. A ratchet computed from the same partial view it
# is ratcheting is not a ratchet.
#
# `$NROS_PLATFORMS_DIR` is deliberately NOT read here: it is a per-invocation
# override, and a gate that answered differently depending on the caller's
# environment would be reporting a property of the shell rather than of the
# tree.
CONFIG_DIR = os.path.join(ROOT, "config")
SEARCH_ROOTS = (PLATFORM_DIR, CONFIG_DIR)
BOARDS_DIR = os.path.join(ROOT, "packages", "boards")


def declared_names(search_roots=SEARCH_ROOTS):
    """Every name any descriptor answers to -> the file that declares it.

    Over the loader's whole search path, not just its first root. An earlier
    root wins on a duplicate, matching `PlatformsTree`: "first root defining a
    name wins".
    """
    out = {}
    for root in search_roots:
        if not os.path.isdir(root):
            continue
        for entry in sorted(os.listdir(root)):
            path = os.path.join(root, entry, "nros-platform.toml")
            if not os.path.isfile(path):
                continue
            with open(path, "rb") as fh:
                doc = tomllib.load(fh)
            for name in doc.get("names", []):
                out.setdefault(name, os.path.relpath(path, ROOT))
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

# THE BASELINE IS GONE (phase-468 W1). There is no `BASELINE_UNANSWERED` any
# more, and re-adding one would be the defect rather than the repair.
#
# It existed because the rule could not be met: an unanswered name only warned,
# so a gate that refused one outright would have been refusing a state the tree
# was in. It held five entries, then two, then one:
#
#   freertos-posix / nuttx-riscv / zephyr-cortex-m
#       never fall-throughs at all — fixture COORDINATE labels, whose boards
#       declare `freertos` / `nuttx` / `zephyr`. Retired by issue 1362, which
#       changed the POPULATION this gate reads.
#   bare-metal
#       answered by `config/bare-metal/nros-platform.toml` since phase-349 W1.
#       Only this gate's one-root reach said otherwise (issue 1486); reading
#       both of the loader's `SEARCH_ROOTS` retired it.
#   esp32
#       the last one, and the only one that was ever a real fall-through.
#       phase-468 W1 answered it: `config/bare-metal` now lists `esp32` in its
#       `names`, because the esp32 board's zenoh C build ALREADY resolved that
#       file (`platform-bare-metal` -> `zpico-sys/bare-metal` ->
#       `CARGO_FEATURE_BARE_METAL` -> `nros-zpico-build`'s own `platform_name`),
#       `PlatformKind::Esp32::platform_feature()` already answered
#       `platform-bare-metal`, and `[arch.riscv32imc]` in that file was written
#       for the ESP32-C3. The file's own `names` comment carries the four
#       measurements. It declares no `[knobs.*]`, so the resolved rungs are
#       byte-identical to the builtins esp32 was falling through to — the image
#       does not move.
#
# Three of the five entries were never real, and the one that was got answered
# by reading what the tree already did rather than by inventing numbers. That
# is the case against ever writing another one: a baseline here records a
# question nobody asked, and the ratchet can only retire an entry the tree
# happens to fix by accident.
#
# NOTE the spelling. A board declares `bare-metal`; `examples/fixtures.toml`
# labels the same rows `baremetal`. The old baseline carried the fixtures
# spelling, so even its one real entry named a string this population never
# produces.


def findings(used, declared):
    return sorted(
        n for n in used if n not in declared and n not in NOT_A_DESCRIPTOR_NAME
    )


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
    bad = 0
    for used, declared, want in cases:
        got = findings(used, declared)
        if got != want:
            print(f"  self-test FAIL: used={used} declared={set(declared)} -> {got}, want {want}")
            bad += 1
    bad += removed_descriptor_is_reported()
    print(f"check-platform-name-answered self-test: {'OK' if not bad else 'FAILED'} "
          f"({len(cases)} table cases + the removed-descriptor control)")
    return bad


def removed_descriptor_is_reported():
    """phase-468 W1 box 4 — the gate must FAIL on a deliberately removed descriptor.

    The table cases above exercise `findings()` over hand-written sets, which
    proves the predicate and nothing about the tree. This runs the REAL board
    population against a search path built from the real descriptors MINUS one,
    so it answers the question the work item actually asks: if a descriptor
    disappeared, would this gate say so?

    Mirroring rather than copying the files: the synthetic root holds one
    `nros-platform.toml` per descriptor with only the `names` line, which is the
    only key `declared_names()` reads. That keeps the control independent of
    whatever else those files grow.

    The removed descriptor is chosen from the tree, never named here — a
    hardcoded platform is the shape that rots into a vacuous control the day
    that platform is renamed.
    """
    declared = declared_names()
    used = set(looked_up_names())
    answering = sorted({f for n, f in declared.items() if n in used})
    if not answering:
        print("  self-test FAIL: no descriptor answers any board-declared name, so "
              "the removed-descriptor control cannot run")
        return 1

    victim = answering[0]
    with tempfile.TemporaryDirectory() as tmp:
        root = os.path.join(tmp, "platform")
        by_file = {}
        for name, rel in declared.items():
            by_file.setdefault(rel, []).append(name)
        for rel, names in by_file.items():
            if rel == victim:
                continue
            # The directory name is irrelevant to `declared_names()` — it reads
            # `names` — but keep it recognisable for anyone debugging this.
            d = os.path.join(root, os.path.basename(os.path.dirname(rel)))
            os.makedirs(d, exist_ok=True)
            with open(os.path.join(d, "nros-platform.toml"), "w") as fh:
                fh.write("names = [" + ", ".join(f'"{n}"' for n in sorted(names)) + "]\n")
        maimed = declared_names(search_roots=(root,))
        lost = {n for n in declared if n not in maimed}
        got = set(findings(used, maimed))

    # The assertion is on the DELTA, never on the absolute set. A tree that is
    # ALREADY reporting something — which is exactly the tree an operator is
    # looking at when this control runs — must not make the control fail for a
    # reason that is not about the control; `main()` below is what names those,
    # and it says far more useful things than a self-test can.
    base = set(findings(used, declared))
    want = base | {n for n in lost if n in used and n not in NOT_A_DESCRIPTOR_NAME}

    if want == base:
        print(f"  self-test FAIL: removing {victim} took no board-declared name with it, "
              f"so the control proves nothing")
        return 1
    if got != want:
        print(f"  self-test FAIL: with {victim} removed the gate reports "
              f"{sorted(got)}, expected {sorted(want)}")
        return 1
    return 0


def main():
    if "--self-test" in sys.argv:
        return 1 if self_test() else 0
    if self_test():
        return 1

    declared = declared_names()
    used_map = looked_up_names()
    used = set(used_map)

    bad = findings(used, declared)
    if bad:
        print("check-platform-name-answered: FAIL — platform name(s) no descriptor answers to:",
              file=sys.stderr)
        for name in bad:
            where = ", ".join(used_map.get(name, []))
            print(f"  {name!r} is declared by {where or 'a board'}, and no "
                  f"packages/platform/*/ or config/*/nros-platform.toml lists it in `names`.",
                  file=sys.stderr)
        print("\n  An unanswered name is FATAL at build time (phase-468 W1):", file=sys.stderr)
        print("  `BuildRungs::require_rungs` panics rather than falling through to", file=sys.stderr)
        print("  the BUILTIN knob defaults, which are not that platform's numbers.", file=sys.stderr)
        print("  Falling through is how `threadx-linux` stopped compiling with an", file=sys.stderr)
        print("  error naming neither the descriptor nor the name (issue 1145).", file=sys.stderr)
        print("\n  Two remedies, and which one is right is a MEASUREMENT, not a", file=sys.stderr)
        print("  preference:", file=sys.stderr)
        print("    - the platform already resolves to an existing descriptor on some", file=sys.stderr)
        print("      other road (feature, cmake token, arch table) -> add the name to", file=sys.stderr)
        print("      that descriptor's `names`, the way `config/bare-metal` carries", file=sys.stderr)
        print("      `esp32` and `freertos` carries `freertos-lwip`. names[0] stays", file=sys.stderr)
        print("      canonical, and write the measurement down beside it.", file=sys.stderr)
        print("    - it is a genuinely new platform -> give it its own", file=sys.stderr)
        print("      `nros-platform.toml`. A descriptor declaring only `names` is a", file=sys.stderr)
        print("      legitimate answer ('this platform states no rungs'); an ABSENT", file=sys.stderr)
        print("      file is not an answer at all, which is the whole point.", file=sys.stderr)
        print("\n  Do NOT add a baseline. There used to be one here and phase-468 W1", file=sys.stderr)
        print("  emptied it; see the comment where it lived.", file=sys.stderr)
        return 1
    print(f"check-platform-name-answered: OK ({len(used)} platform name(s) declared by "
          f"boards, all answered — no baseline since phase-468 W1; "
          f"descriptors answer to {len(declared)} name(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())

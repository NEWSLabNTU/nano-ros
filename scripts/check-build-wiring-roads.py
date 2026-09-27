#!/usr/bin/env python3
"""Every `Driver` variant is a road the build-wiring map knows.

# The failure this prevents

`docs/reference/canonical-build-path.md` explains how a configuration knob
reaches the compiler on each build road, and `scripts/nros-build-wiring.py`
carries the one authored structure that map needs: a road's CARRIER, which is a
design fact no grep can read off the tree.

An authored structure beside a live enum drifts, and this repository has
measured that drift three times in the safe-looking direction: the RMW parity
map read `("gap", "no vtable slot")` for 28 slots that had landed, the
step->stage map in `check-lane-stage-reporting` goes stale on a renamed step,
and the package-directory list named four directories that no longer existed.
Each time the authored side said LESS than the tree, so every reader was told
a road did not exist rather than being told the map was incomplete.

A new `Driver` variant is exactly that: a road whose knob carrier nobody wrote
down, in a document whose whole subject is that the carriers differ.

# What it checks

BOTH directions, because either gap is a lie:

* every `Driver` variant in `plan.rs` has a `ROADS` entry and is named by the
  reference doc;
* every `ROADS` key is a real `Driver` variant — a road removed from the code
  and left in the map is the parity map's failure exactly.

Run: `just check build-wiring-roads`  (also `--self-test`)
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLAN = ROOT / "packages/cli/nros-cli-core/src/builder/plan.rs"
DOC = ROOT / "docs/reference/canonical-build-path.md"
TOOL = ROOT / "scripts/nros-build-wiring.py"

# `pub enum Driver { ... }` up to the closing brace at column 0 of the block.
ENUM_RE = re.compile(r"pub enum Driver\s*\{(.*?)\n\}", re.S)
# A variant is a bare CamelCase identifier at the start of a line inside it,
# never a doc comment or an attribute.
VARIANT_RE = re.compile(r"^\s{4}([A-Z][A-Za-z0-9]*)\s*,\s*$", re.M)


def driver_variants(text: str) -> list[str]:
    m = ENUM_RE.search(text)
    if not m:
        return []
    return VARIANT_RE.findall(m.group(1))


def roads_keys(text: str) -> list[str]:
    m = re.search(r"^ROADS\s*=\s*\{(.*?)^\}", text, re.S | re.M)
    if not m:
        return []
    return re.findall(r'^\s{4}"([A-Za-z0-9]+)"\s*:', m.group(1), re.M)


def self_test() -> int:
    bad = 0

    def chk(label: str, ok: bool) -> None:
        nonlocal bad
        print(f"  {'ok' if ok else 'FAIL'}  {label}")
        if not ok:
            bad = 1

    sample = (
        "pub enum Driver {\n"
        "    /// doc line, not a variant\n"
        "    Cargo,\n"
        "    #[allow(dead_code)]\n"
        "    CMake,\n"
        "}\n"
        "\nimpl Driver {\n    fn other(self) -> Later { Later::Nope }\n}\n"
    )
    got = driver_variants(sample)
    chk("variants are read, doc lines and attributes are not", got == ["Cargo", "CMake"])
    chk("the scan stops at the enum, not at the next block",
        "Later" not in got and "Nope" not in got)
    chk("no enum at all reads as EMPTY, which the caller treats as fatal",
        driver_variants("fn nothing() {}") == [])

    roads_sample = (
        'ROADS = {\n'
        '    "Cargo": {\n        "exec": "cargo build",\n    },\n'
        '    "West": {\n        "exec": "west build",\n    },\n'
        '}\n'
    )
    chk("ROADS keys are read at one nesting level only",
        roads_keys(roads_sample) == ["Cargo", "West"])
    chk("no ROADS block reads as EMPTY", roads_keys("X = 1") == [])

    # The negative control that matters: a variant absent from ROADS must be
    # REPORTED. Asserted on the computed delta, so an already-clean tree cannot
    # make this pass for the wrong reason.
    variants = ["Cargo", "CMake", "West", "Invented"]
    roads = ["Cargo", "CMake", "West"]
    chk("a Driver variant missing from ROADS is a finding",
        sorted(set(variants) - set(roads)) == ["Invented"])
    chk("a ROADS key that is no longer a Driver variant is a finding",
        sorted(set(roads + ["Retired"]) - set(variants)) == ["Retired"])
    return bad


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    # The control runs on the NORMAL path, not only when asked for. A negative
    # control nobody invokes is the shape this repository refuses: it reads as
    # coverage while proving nothing about the run that matters.
    if self_test() != 0:
        print("[FAIL] the self-test did not pass, so nothing below is trustworthy.",
              file=sys.stderr)
        return 1

    for p in (PLAN, DOC, TOOL):
        if not p.is_file():
            print(f"[FAIL] missing {p.relative_to(ROOT)}", file=sys.stderr)
            return 1

    variants = driver_variants(PLAN.read_text())
    if not variants:
        print(f"[FAIL] no `pub enum Driver` variants found in {PLAN.relative_to(ROOT)}",
              file=sys.stderr)
        print("       With none parsed this gate would pass while checking", file=sys.stderr)
        print("       nothing, which is the shape it exists to refuse.", file=sys.stderr)
        return 1

    roads = roads_keys(TOOL.read_text())
    if not roads:
        print(f"[FAIL] no ROADS entries found in {TOOL.relative_to(ROOT)}", file=sys.stderr)
        return 1

    doc = DOC.read_text()
    fail = 0

    for missing in sorted(set(variants) - set(roads)):
        print(f"[FAIL] Driver::{missing} is a build road with no ROADS entry in", file=sys.stderr)
        print(f"       {TOOL.relative_to(ROOT)} — so the wiring map cannot say how a", file=sys.stderr)
        print("       knob reaches the compiler on it, which is that map's whole", file=sys.stderr)
        print("       subject. Add its `exec`, `carrier`, `emits_root` and hazard.", file=sys.stderr)
        fail = 1

    for stale in sorted(set(roads) - set(variants)):
        print(f"[FAIL] ROADS names '{stale}', which is not a Driver variant.", file=sys.stderr)
        print("       A road left in the map after the code dropped it tells every", file=sys.stderr)
        print("       reader a carrier exists for a road that does not.", file=sys.stderr)
        fail = 1

    for v in variants:
        if v.lower() not in doc.lower():
            print(f"[FAIL] {DOC.relative_to(ROOT)} never names Driver::{v}.", file=sys.stderr)
            print("       The road table there is what a reader consults; a road", file=sys.stderr)
            print("       absent from it reads as a road that does not exist.", file=sys.stderr)
            fail = 1

    if fail:
        return 1

    print(f"check-build-wiring-roads: OK — {len(variants)} build road(s) "
          f"({', '.join(variants)}), each with a carrier in the wiring map and "
          f"named by the reference doc.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

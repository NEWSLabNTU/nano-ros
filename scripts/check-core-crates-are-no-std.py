#!/usr/bin/env python3
"""Every core crate declares `#![no_std]` UNCONDITIONALLY.

ARCHITECTURE section 2 / phase-359: the terminal state of the core crates is
`core` and `core+alloc`. `std` there is a second implementation of the platform
layer, not a convenience over it.

WHY THE SPELLING MATTERS, and why this is not a style gate.

`#![cfg_attr(not(feature = "std"), no_std)]` compiles a DIFFERENT CRATE
depending on a feature. Both configurations are then real, both need testing,
and the one an embedded image uses is the one CI is least likely to build --
every merge-gating lane in this repo builds with `std`, because `std` is what a
host test needs. So the conditional form quietly creates the configuration that
nothing checks.

`#![no_std]` unconditional removes the choice. A core crate that cannot name
`std` cannot grow a dependency on it, and `check-no-std-stdio` (the sibling
gate) then has a fixed target rather than a moving one.

This does NOT stop a crate using `alloc` -- that is a separate axis and a
legitimate one (`extern crate alloc` behind `feature = "alloc"`). Issue 1177 is
about the alloc axis; this gate is about the std one.

WHICH CRATES -- and why that is no longer decided here (issue 1212).

This gate used to carry its own six-name list, and argued for it: "a new crate
landing there should have to be added here on purpose". The decision was right;
the list was the wrong place to keep it. `packages/core` held ELEVEN Rust crates
against the six named, and the four target-side ones outside the list
(`nros-serdes`, `nros-serdes-packed`, `nros-diagnostics`, `nros-executor-layout`)
read as considered-and-excluded when they had simply never been enumerated --
`nros-executor-layout` appeared in no "core crates" list anywhere in the repo.

The scope now comes from `scripts/lib/core_crates.py`, which DERIVES it from
`packages/core` and lets a crate classify itself out with the `host-only` marker
issue 0287 already established. The decision survives with its polarity fixed:
core is the default, and opting OUT is what costs a person a written reason.

Run: python3 scripts/check-core-crates-are-no-std.py [--self-test]
"""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from core_crates import (  # noqa: E402
    core_crate_paths,
    layout,
    self_test as core_crates_self_test,
)

REPO = Path(__file__).resolve().parents[1]

UNCONDITIONAL = re.compile(r"^\s*#!\[no_std\]", re.M)
# `.*` and not `[^)]*`: the form this gate exists to name is
# `#![cfg_attr(not(feature = "std"), no_std)]`, and a character class excluding
# `)` cannot cross the one that closes `not(...)`. So the CONDITIONAL branch
# never fired for the exact spelling the docstring above is about -- a crate
# with it was reported as "no `#![no_std]` at all", advice that is wrong in a
# way that sends the reader to add a line that is already there. Found by
# mutation-testing this gate while widening its scope (issue 1212); the verdict
# was always red, only the reason was misattributed. `re.M` keeps `.` off
# newlines, so this still cannot reach past the attribute's own line.
CONDITIONAL = re.compile(r"^\s*#!\[cfg_attr\(.*\bno_std\b", re.M)


def offenders(roots=None, repo=None):
    base = Path(repo) if repo else REPO
    out = []
    for rel in roots if roots is not None else core_crate_paths(base):
        lib = base / rel / "src" / "lib.rs"
        if not lib.is_file():
            out.append((rel, "no src/lib.rs"))
            continue
        text = lib.read_text(errors="replace")
        if UNCONDITIONAL.search(text):
            continue
        if CONDITIONAL.search(text):
            out.append((rel, "conditional `cfg_attr(..., no_std)` -- make it unconditional"))
        else:
            out.append((rel, "no `#![no_std]` at all"))
    return out


def self_test():
    """A crate with each spelling, so the gate is known to be able to fail."""
    import tempfile

    # The third element is a substring the REASON must contain, or None when the
    # case is expected to pass. Asserting only the count let the two failing
    # cases be indistinguishable, and they were: the nested-`not(...)` case was
    # caught by the wrong branch for two phases and this self-test read green,
    # because "1 offender" was all it ever asked for.
    cases = [
        ("#![no_std]\n", 0, None, "unconditional passes"),
        (
            '#![cfg_attr(not(feature = "std"), no_std)]\n',
            1,
            "conditional",
            "conditional with a nested `not(...)` is caught AS conditional",
        ),
        (
            '#![cfg_attr(feature = "embedded", no_std)]\n',
            1,
            "conditional",
            "conditional without nesting is caught as conditional",
        ),
        ("// nothing\n", 1, "no `#![no_std]` at all", "missing is caught"),
        ("//! docs\n#![no_std]\n", 0, None, "unconditional after a doc comment passes"),
    ]
    failures = 0
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        for i, (body, want, want_why, name) in enumerate(cases):
            rel = f"crate{i}"
            (root / rel / "src").mkdir(parents=True)
            (root / rel / "src" / "lib.rs").write_text(body)
            found = offenders(roots=[rel], repo=root)
            if len(found) != want:
                print(f"  self-test FAIL: {name} -- got {len(found)}, want {want}", file=sys.stderr)
                failures += 1
            elif want_why is not None and want_why not in found[0][1]:
                print(
                    f"  self-test FAIL: {name} -- reason {found[0][1]!r}"
                    f" does not mention {want_why!r}",
                    file=sys.stderr,
                )
                failures += 1
    # The DERIVATION's own negative control, owned by the module that does the
    # deriving rather than copied here -- one self-test, exercised by every
    # consumer of the scope.
    failures += core_crates_self_test()
    if failures:
        print(f"check-core-crates-are-no-std self-test: FAILED ({failures})", file=sys.stderr)
        return 1
    print(f"check-core-crates-are-no-std self-test: OK ({len(cases)} + 1 cases)")
    return 0


def main(argv):
    if len(argv) == 2 and argv[1] == "--self-test":
        return self_test()
    if self_test():
        return 1
    lay = layout()
    if lay.bad:
        print(
            "check-core-crates-are-no-std: a crate under packages/core opts out of core"
            " without saying why:",
            file=sys.stderr,
        )
        for rel, why in lay.bad:
            print(f"  {rel}: {why}", file=sys.stderr)
        print("", file=sys.stderr)
        print("  `host-only = true` is how a crate leaves the core; `host-only-reason`", file=sys.stderr)
        print("  is what stops the next reader mistaking an oversight for a decision.", file=sys.stderr)
        return 1

    bad = offenders()
    if not bad:
        print(
            f"check-core-crates-are-no-std: OK -- {len(lay.target)} core crate(s) are"
            f" unconditionally no_std"
            f" ({len(lay.host)} host-classified, {len(lay.non_rust)} non-Rust under"
            f" packages/core)."
        )
        return 0
    print("check-core-crates-are-no-std: a core crate does not declare `#![no_std]`:", file=sys.stderr)
    for rel, why in bad:
        print(f"  {rel}: {why}", file=sys.stderr)
    print("", file=sys.stderr)
    print("  A core crate runs where there is no operating system. The conditional", file=sys.stderr)
    print("  form compiles a different crate per feature, and the configuration an", file=sys.stderr)
    print("  embedded image uses is the one no merge-gating lane builds.", file=sys.stderr)
    print("", file=sys.stderr)
    print("  The scope is DERIVED from packages/core (scripts/lib/core_crates.py).", file=sys.stderr)
    print("  If this crate genuinely runs only on the build host, declare", file=sys.stderr)
    print("  `[package.metadata.nros] host-only = true` with a `host-only-reason`", file=sys.stderr)
    print("  rather than widening a list.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))

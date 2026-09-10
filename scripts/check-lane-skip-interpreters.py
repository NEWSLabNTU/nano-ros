#!/usr/bin/env python3
"""Every `nros_lane_skip` producer sits under a caller that INTERPRETS rc 78.

WHY THIS EXISTS

`scripts/build/lane-skip.sh` gives a lane a third verdict: a missing
precondition is neither success nor failure, so `nros_lane_skip` prints a marker
and exits 78 (sysexits' EX_CONFIG). Its own doc names the one consumer:

    The driver in `justfile`'s `build-test-fixtures-leaves` treats 78 as
    SKIPPED, prints the reason, and does NOT fail the build.

That is the whole interpreter. A producer reached from anywhere else emits a
skip into a context that cannot read it, and 78 lands as a plain failure —
which is the OPPOSITE of what the protocol exists to say.

WHAT IT COST

`just ci provision-zenohd` was called from `native.just`'s `setup` under
`set -e`. On the bare-host runner CI was root, the install path was taken, and
the skip branch never ran. The containerised self-hosted runner is NOT root, so
the recipe took its "workstation" branch, skipped honestly, exited 78 — and
failed `just setup tier2`, failed L3, and stalled the merge queue for every PR
in the repo for an hour. Nothing about the recipe was wrong; the caller could
not hear it.

WHY A DECLARED LIST AND NOT A CALL-GRAPH WALK

`just` recipes reach each other through `just <module> <recipe>` strings, shell
`$()`, and workflow YAML, so "who calls this" is not statically decidable here.
Guessing it would give a checker that is confidently wrong. Instead every
producing recipe is DECLARED with the interpreter that covers it, and a new one
is a FAILURE until somebody writes down which caller reads its 78 — the same
"a new entry forces a decision" ratchet as `.config/gate-selftest-baseline.txt`.

Usage:  check-lane-skip-interpreters.py [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# recipe name -> what interprets its rc 78. Adding a row is the decision; the
# reason column is the evidence somebody checked, not decoration.
INTERPRETED = {
    # `justfile`'s `build-test-fixtures-leaves` fan-out — the driver named in
    # lane-skip.sh's own header. It runs `just <plat> build-fixtures` and maps
    # 78 -> SKIPPED.
    "build-fixtures": "justfile build-test-fixtures-leaves driver",
    "build-fixtures-arm": "justfile build-test-fixtures-leaves driver",
    "build-fixtures-arm-smp": "justfile build-test-fixtures-leaves driver",
    "build-fixtures-posix": "justfile build-test-fixtures-leaves driver",
    "build-fixture-extras": "reached from build-fixtures in the same module",
    "build-examples": "reached from build-fixtures in the same module",
    "build-riscv-c": "reached from build-fixtures in the same module",
    "build-riscv-c-workspaces": "reached from build-fixtures in the same module",
    "build-riscv-rust": "reached from build-fixtures in the same module",
    "build-rtic-main-e2e": "reached from build-fixtures in the same module",
    # `native.just`'s `setup` interprets 78 explicitly (this gate's motivating
    # failure); the CI workflows that call it do the same.
    "provision-zenohd": "native.just setup interprets 78; see this gate's header",
    # A gate, not a fixture lane: `check-submodule-commits-reachable` skips only
    # when the network is unreachable, which no merge-gating lane is. LATENT
    # instance of this same class — it works because of the environment, not
    # because a caller reads the rc. Left declared rather than silently passing.
    "submodule-commits-reachable": "LATENT: only skips with no network; no lane it runs in lacks one",
    # The FVP pair. `build-fvp-*` is chained by `verify-fvp-runtime`, which
    # interprets 78 — it did NOT until this gate found it, while its own comment
    # promised it "skips cleanly (never false-fails) when the model is absent".
    "build-fvp-ws-entry": "verify-fvp-runtime interprets 78",
    "build-fvp-board-import": "verify-fvp-runtime interprets 78",
    # `run-fvp-*` are invoked directly (`just zephyr run-fvp-ws-entry`), where
    # the marker is read by a person and 78 is a legible exit code. Declared
    # rather than assumed: if either is ever chained under `set -e`, that caller
    # owes an interpreter, and this row is where the next person looks.
    "run-fvp-ws-entry": "invoked directly; no chaining caller",
    "run-fvp-board-import": "invoked directly; no chaining caller",
    # Becomes a producer by PROPAGATING `build-fvp-ws-entry`'s skip rather than
    # swallowing it — `check-lane-skip-protocol` refused the `exit 0` form, and
    # was right: a recipe that ran no runtime check must not report success.
    "verify-fvp-runtime": "invoked directly; propagates the build's 78 rather than reporting OK",
}

RECIPE = re.compile(r"^([a-z][a-z0-9_-]*):")


def producers(text):
    """Recipe name -> True for each recipe whose body calls `nros_lane_skip`."""
    out, current = set(), None
    for line in text.splitlines():
        m = RECIPE.match(line)
        if m:
            current = m.group(1)
        elif "nros_lane_skip" in line and not line.lstrip().startswith("#"):
            if current:
                out.add(current)
    return out


def tracked_just_files():
    """Ask git, never walk the tree (the repo's rule for enumerating)."""
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "just/*.just", "just/**/*.just", "justfile"],
        capture_output=True, text=True, check=True,
    ).stdout
    return [p for p in out.split("\n") if p]


def undeclared():
    found = {}
    for rel in tracked_just_files():
        with open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace") as f:
            for name in producers(f.read()):
                found.setdefault(name, []).append(rel)
    return {n: fs for n, fs in found.items() if n not in INTERPRETED}


def self_test():
    """Both directions, on the normal path.

    The negative case is the point: this gate's whole job is to notice a
    producer nobody declared, so a version that cannot see one is worthless —
    and a gate that only ever runs against a tree satisfying it would never
    find out. Both cases are synthetic text, so this costs nothing.
    """
    declared = "prep:\n    source scripts/build/lane-skip.sh\n    nros_lane_skip \"x\"\n"
    assert producers(declared) == {"prep"}, "a producer must be seen"

    commented = "prep:\n    # nros_lane_skip \"x\"\n    echo hi\n"
    assert producers(commented) == set(), "a COMMENT naming it is not a producer"

    two = "a:\n    nros_lane_skip \"x\"\nb:\n    echo hi\n"
    assert producers(two) == {"a"}, "attribution must follow the recipe header"
    return 0


def main():
    self_test()
    bad = undeclared()
    if bad:
        print(
            "check-lane-skip-interpreters: recipe(s) exit 78 with no declared "
            "interpreter:", file=sys.stderr
        )
        for name, files in sorted(bad.items()):
            print(f"  - {name}  ({', '.join(sorted(set(files)))})", file=sys.stderr)
        print(
            "\n  `nros_lane_skip` exits 78, and only a caller that MAPS 78 to\n"
            "  'skipped' can hear it. Everywhere else it is a plain failure —\n"
            "  which is the opposite of what the protocol says.\n"
            "  Add a row to INTERPRETED naming what reads this recipe's rc, or\n"
            "  make the caller interpret it. See this file's header for the\n"
            "  hour of blocked merge queue that motivated the rule.",
            file=sys.stderr,
        )
        return 1
    print(
        f"check-lane-skip-interpreters: OK — {len(INTERPRETED)} producing "
        f"recipe(s), each with a declared interpreter."
    )
    return 0


if __name__ == "__main__":
    if "--self-test" in sys.argv[1:]:
        raise SystemExit(self_test())
    raise SystemExit(main())

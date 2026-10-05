#!/usr/bin/env python3
"""A lane must not name a gate that one of its own steps already runs (issue 1500).

`just` runs a DEPENDENCY once per invocation, and that is where the intuition
"listing it twice is harmless" comes from. It does not reach inside the gate
runner: `check::fast` and `check::build` start each gate as its own
`just check <gate>` PROCESS, so a gate that is derived into one of those lanes
AND named beside it runs twice.

That is how `api-parity` ran twice in every `just check`, `just ci gate`, tier 1
and tier 2 after issue 1066 moved it onto the fast lane: once inside the fan-out
and once as `default`'s trailing dependency (and as a `steps=(...)` entry of
`ci gate`). Measured on `host-tests` run 37176625915: 163 s of a 4-vCPU runner's
time, plus ~4 000 lines of identical diff output in a log that already had no
timestamps — while the job was being killed by its 150-minute ceiling.

What this checks, over the two places a lane names its steps:

  * `default:`'s dependency line in `just/check.just`;
  * every `steps=(...)` array in `just/ci.just`.

A name there that a SIBLING step already derives (`check::fast` -> the fast
list, `check::build` -> the build list, `check::default` -> both plus default's
own dependencies) is a duplicate, unless it is in POSITIONAL below with the
reason it is worth running twice.

Buildless: it reads two justfiles and asks `check-gate-lists.py` for the lists.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CHECK_JUST = os.path.join(ROOT, "just", "check.just")
CI_JUST = os.path.join(ROOT, "just", "ci.just")
GATE_LISTS = os.path.join(ROOT, "scripts", "check", "check-gate-lists.py")

# Named twice ON PURPOSE. The second run is the price of running FIRST.
POSITIONAL = {
    "cli-fresh": (
        "0.02 s, and its whole contribution is position (issues 0363, 1320): a "
        "stale CLI fails here before a 13-minute fan-out reports it as a CMake "
        "error in an unrelated gate."
    ),
}

DEFAULT_DEPS = re.compile(r"^default:\s*([^#\n]*)$", re.M)
STEPS = re.compile(r"^\s*steps=\(\s*([^)]*?)\s*\)\s*$", re.M)


def derived(lane: str) -> set[str]:
    r = subprocess.run([sys.executable, GATE_LISTS, "--list", lane],
                       capture_output=True, text=True, check=True)
    return {n.strip() for n in r.stdout.splitlines() if n.strip()}


def lane_runs(step: str, fast: set[str], build: set[str], default: list[str]) -> set[str]:
    """The gate names a step reaches THROUGH the runner (not its own name)."""
    if step == "check::fast":
        return set(fast)
    if step == "check::build":
        return set(build)
    if step == "check::default":
        return set(fast) | set(build) | set(default)
    return set()


def duplicates(names: list[str], fast: set[str], build: set[str], default: list[str]):
    """[(name, sibling)] — a `check::<gate>` (or bare default dep) a sibling runs."""
    out = []
    for name in names:
        gate = name.split("::", 1)[1] if name.startswith("check::") else name
        for sib in names:
            if sib == name:
                continue
            if gate in lane_runs(sib, fast, build, default) and gate not in POSITIONAL:
                out.append((name, sib))
    return out


def self_test() -> None:
    fast, build = {"api-parity", "cli-fresh", "fmt"}, {"c", "cpp"}
    dup = duplicates(["check::cli-fresh", "check::fast", "check::api-parity"], fast, build, [])
    assert dup == [("check::api-parity", "check::fast")], dup
    # check default's deps are written bare and resolve inside the module.
    dup = duplicates(["check::fast", "check::build", "check::c"], fast, build, [])
    assert dup == [("check::c", "check::build")], dup
    assert duplicates(["check::fast", "check::build", "test-unit"], fast, build, []) == []
    assert duplicates(["check::default", "check::fmt"], fast, build, []) == \
        [("check::fmt", "check::default")]


def main() -> int:
    self_test()
    fast, build = derived("fast-serial"), derived("build-serial")
    check_text = open(CHECK_JUST, encoding="utf-8").read()
    m = DEFAULT_DEPS.search(check_text)
    if not m:
        sys.stderr.write("check-lane-step-duplicates: no `default:` line in just/check.just\n")
        return 1
    default = m.group(1).split()

    problems = []
    for name, sib in duplicates([f"check::{d}" for d in default], fast, build, default):
        problems.append(f"just/check.just `default:` names `{name[7:]}`, which `{sib[7:]}` "
                        f"already runs as its own process")
    ci_text = open(CI_JUST, encoding="utf-8").read()
    arrays = STEPS.findall(ci_text)
    if not arrays:
        sys.stderr.write("check-lane-step-duplicates: found no `steps=(...)` in just/ci.just — "
                         "the lanes changed shape and this gate checks nothing\n")
        return 1
    for arr in arrays:
        for name, sib in duplicates(arr.split(), fast, build, default):
            problems.append(f"just/ci.just `steps=({arr})` names `{name}`, which `{sib}` "
                            f"already runs")

    if problems:
        sys.stderr.write("check-lane-step-duplicates: FAILED — a gate runs twice per lane\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        sys.stderr.write(
            "  The gate runner starts each gate as a separate `just` process, so just's\n"
            "  once-per-invocation dependency rule does not apply. Drop the name, or add\n"
            "  it to POSITIONAL in scripts/check-lane-step-duplicates.py with the reason\n"
            "  running it first is worth running it twice (issue 1500).\n")
        return 1
    print(f"check-lane-step-duplicates: OK ({len(arrays)} lane step list(s) + check default; "
          f"{len(POSITIONAL)} positional duplicate(s) allowed)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

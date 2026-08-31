#!/usr/bin/env python3
"""issue 0650 — a fixture lane may not skip its way to a green exit.

Issue 0599 gave lanes a third verdict (`nros_lane_skip`, exit 78 = SKIPPED) and
converted three lanes to it. Twenty-one sites in five other lanes kept
`echo "… skip: …"; exit 0`, so a lane with no toolchain built nothing, printed
`<platform> test fixtures built.`, and exited 0 — and the driver recorded OK.

That is not a cosmetic verdict. It is how phase-366 W5.c's six diverged riscv64
examples reached main: the lane that compiles them reported OK on every host
that lacked the toolchain, so nothing contradicted them until a source-level
gate in another lane failed one run later.

# The rule

Inside `just/*.just` and `justfile`, a skip in a lane recipe must go through the
protocol in `scripts/build/lane-skip.sh`:

  * `nros_lane_skip "<reason>"`              — the whole recipe cannot run
  * `nros_lane_skip_note <lane> "<reason>"`  — this STEP cannot run; the lane
                                               continues and flushes at the end

What is banned is a bare `exit 0` reached from a line that announces a skip, in
either spelling — same line, or an `echo` followed by `exit 0`. Both were
present, which is why the first sweep found only some of them.

# Deliberately not checked

Whether a lane calls `nros_lane_skip_flush`. That is a property of the recipe
graph rather than of a line, and asserting it from a regex would either miss
lanes or forbid recipes that legitimately have no success claim. The artifact
side already covers it: a lane whose steps skipped and which still reports OK
shows up as missing fixtures at the gate.

Run: python3 scripts/check-lane-skip-protocol.py
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

SCAN = [os.path.join(ROOT, "justfile")]
SCAN_DIR = os.path.join(ROOT, "just")

# A line that ANNOUNCES a skip. `echo`/`printf` only: a comment mentioning the
# word is prose, and `nros_lane_skip*` is the protocol itself.
ANNOUNCES = re.compile(r"^\s*(echo|printf)\b[^\n]*\bskip", re.IGNORECASE)
SAME_LINE_EXIT = re.compile(r";\s*exit\s+0\s*$")
BARE_EXIT_0 = re.compile(r"^\s*exit\s+0\s*$")
PROTOCOL = re.compile(
    r"\bnros_(lane|check)_(skip(_note|_flush|_reset|_report)?|scope|scope_note)\b"
)

# Sites that are NOT a lane precondition, with the reason. A skip line here is
# about a case inside an already-running step, not about a lane that cannot run.
EXEMPT_SUBSTRINGS = (
    # `just workspace`: sccache is already on PATH — nothing was skipped, the
    # setup step simply had nothing to do.
    "already on PATH",
    # The rerun helper reports that a junit file holds no real failures.
    "nothing to rerun",
    # Coordinate narrowing inside a lane that IS running: the lane's own
    # `fixtures.toml` rows say this id is out of the run's coordinates. That is
    # phase-340's lane scoping, and it is reported by the manifest, not here.
    "no nuttx-riscv coordinate",
    "not in this lane's coordinates",
)

# Previously an exemption list. The six sites it held — `check-abi-bindings`
# without bindgen, `dep-chain` without ROS 2, `check-board-projections` without
# the in-tree CLI, `colcon-parity`, and the two doxygen recipes — now go through
# `nros_check_skip`, the CHECK-side ledger (scripts/build/check-skip.sh). They
# keep their exit code, because `check-fast` is documented to run green on a
# pristine worktree; what changed is that the lane's closing sentence names them
# instead of letting "All checks passed!" stand for gates that never ran.


def offenders(text):
    """[(lineno, line, why)] for every raw skip-to-zero in one file body."""
    out = []
    lines = text.splitlines()
    for i, line in enumerate(lines, start=1):
        if line.lstrip().startswith("#"):
            continue
        if not ANNOUNCES.search(line):
            continue
        if PROTOCOL.search(line):
            continue
        if any(frag in line for frag in EXEMPT_SUBSTRINGS):
            continue
        if SAME_LINE_EXIT.search(line):
            out.append((i, line.strip(), "announces a skip and exits 0 on the same line"))
        elif i < len(lines) and BARE_EXIT_0.match(lines[i]):
            out.append((i, line.strip(), "announces a skip, then `exit 0` on the next line"))
    return out


def self_test():
    """Both directions: a classifier that stopped classifying looks like a pass."""
    bad = []
    must_flag = [
        ('    echo "FreeRTOS skip: arm-none-eabi-gcc not found"; exit 0', "same line"),
        ('    echo "Zephyr skip: toolchain missing"\n    exit 0', "next line"),
    ]
    must_pass = [
        ('    nros_lane_skip "arm-none-eabi-gcc not found"', "whole-recipe protocol"),
        ('    nros_lane_skip_note nuttx "no riscv gcc"; exit 0', "step protocol"),
        ('    # echo "skip: prose in a comment"; exit 0', "a comment"),
        ('    echo "[sccache] already on PATH — skipping"\n    exit 0', "exempt site"),
        ('    echo "building"; exit 0', "no skip announced"),
    ]
    for body, label in must_flag:
        if not offenders(body):
            bad.append(f"self-test: expected a violation for {label!r}")
    for body, label in must_pass:
        got = offenders(body)
        if got:
            bad.append(f"self-test: unexpected violation for {label!r}: {got}")
    if bad:
        for b in bad:
            sys.stderr.write(b + "\n")
        sys.exit(2)


def main():
    self_test()
    files = list(SCAN)
    if os.path.isdir(SCAN_DIR):
        files += [
            os.path.join(SCAN_DIR, f)
            for f in sorted(os.listdir(SCAN_DIR))
            if f.endswith(".just")
        ]

    failures = []
    for path in files:
        try:
            text = open(path, encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        rel = os.path.relpath(path, ROOT)
        for lineno, line, why in offenders(text):
            failures.append((rel, lineno, line, why))

    if failures:
        sys.stderr.write("check-lane-skip-protocol: FAILED — skip(s) that exit 0:\n\n")
        for rel, lineno, line, why in failures:
            sys.stderr.write(f"  {rel}:{lineno}: {why}\n      {line}\n")
        sys.stderr.write(
            "\n  A lane that skips must not report success (issue 0650): it built\n"
            "  nothing, and `build-test-fixtures` records OK. Use the protocol in\n"
            "  scripts/build/lane-skip.sh —\n"
            "    nros_lane_skip \"<reason>\"             the whole recipe cannot run\n"
            "    nros_lane_skip_note <lane> \"<reason>\"  this STEP cannot; the lane\n"
            "                                          continues and flushes at the end\n"
            "  A genuinely non-precondition case goes in EXEMPT_SUBSTRINGS with its\n"
            "  reason, not into a bare `exit 0`.\n"
        )
        return 1

    print(f"lane-skip protocol: OK ({len(files)} justfile(s), no skip exits 0)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

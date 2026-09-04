#!/usr/bin/env python3
"""Every gate in `just check`'s DEFAULT list must run in some workflow.

issue 1040 — `check-api-parity` is in that list and ran in NO workflow at all.
Not on `pull_request`, not on `merge_group`, not nightly, not on dispatch:

    grep -rl api-parity .github/workflows/    ->  (nothing)

It existed only as a local recipe, so an unclassified ledger row was invisible
until somebody happened to run the full tier. Three landed on main on
2026-09-04 alone, alongside two compile-tier reds and a NuttX break — seven in
one day, every one found by a person running `just ci gate` rather than by CI.

This does NOT require them to be merge-gating. `check-build` is deliberately
`schedule`/`workflow_dispatch` only: it resolves artifacts no CI job builds, and
making it required once turned every PR red for a day (`check-lane-contracts`
now forbids that shape). A DAILY signal is the bar here, not a blocking one.

Scope is deliberately the default list and nothing wider. Asking it of all ~200
individual gates would flag most of them, because they run collectively via
`just check fast` -- and a gate that fires on almost everything teaches people
to ignore it.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHECK_JUST = ROOT / "just/check.just"
WORKFLOWS = ROOT / ".github/workflows"


def default_list():
    """The recipes `just check` runs with no argument."""
    for line in CHECK_JUST.read_text().splitlines():
        m = re.match(r"^default:\s*(.+?)\s*$", line)
        if m:
            return m.group(1).split()
    return []


def workflow_mentions():
    """Every `just check <name>` invoked by any workflow."""
    found = set()
    for wf in sorted(WORKFLOWS.glob("*.yml")):
        for m in re.finditer(r"just check ([a-z0-9-]+)", wf.read_text()):
            found.add(m.group(1))
    return found


def self_test():
    """On the NORMAL path — a control nobody runs decays into a comment."""
    ok = True
    names = default_list()
    if not names:
        print("self-test FAILED: could not parse `default:` from check.just", file=sys.stderr)
        ok = False
    # the parse must find the real list, not an empty one that passes vacuously
    if names and not all(re.fullmatch(r"[a-z0-9-]+", n) for n in names):
        print(f"self-test FAILED: default list looks wrong: {names}", file=sys.stderr)
        ok = False
    mentions = workflow_mentions()
    if not mentions:
        print("self-test FAILED: no `just check` found in any workflow", file=sys.stderr)
        ok = False
    if ok:
        print(
            f"check-default-gates-run-somewhere self-test: OK "
            f"({len(names)} default gate(s), {len(mentions)} mentioned in workflows)"
        )
    return ok


def main() -> int:
    if not self_test():
        return 1
    names = default_list()
    mentions = workflow_mentions()
    missing = [n for n in names if n not in mentions]
    if missing:
        print(
            f"check-default-gates-run-somewhere: {len(missing)} default gate(s) run in NO workflow",
            file=sys.stderr,
        )
        for n in missing:
            print(f"  just check {n}", file=sys.stderr)
        print("", file=sys.stderr)
        print("  A gate in `just check`'s default list that no workflow runs is", file=sys.stderr)
        print("  invisible between local full-tier runs, and its reds accumulate", file=sys.stderr)
        print("  until someone finds several at once (issue 1040).", file=sys.stderr)
        print("", file=sys.stderr)
        print("  It need not be merge-gating — a nightly signal is the bar. Add it", file=sys.stderr)
        print("  to gate.yml's `schedule`/`workflow_dispatch` step beside", file=sys.stderr)
        print("  `just check build`.", file=sys.stderr)
        return 1
    print(
        f"check-default-gates-run-somewhere: OK ({len(names)} default gate(s), "
        f"all run in at least one workflow)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

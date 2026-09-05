#!/usr/bin/env python3
"""A gate no merge-gating job runs is a gate that rots. Issue 0993.

`just check build` is invoked in exactly one place, `gate.yml`, behind a
`schedule`/`workflow_dispatch` guard. So no pull request and no merge group runs
anything in that lane, and a red there is invisible until somebody reads a
nightly.

Issue 0981 measured the cost: `codegen_golden` sat red on `main` for a day while
the required `CI` context stayed green, and two separate changes reached a green
pull request while failing `just ci gate` locally.

Some gates genuinely belong there — they cannot run without something built.
`borrowed-e2e` compiles `nros-c` and reads its generated config header; that is
a real reason. "Nobody got round to moving it" is not, and the two are
indistinguishable once the list is long.

So the ungated set is written down and may only SHRINK, the same ratchet shape
as `.config/gate-selftest-baseline.txt`. Adding a gate to a non-merge-gating
lane now requires adding a line here, which is the moment to ask whether it
needs to be there.

The membership test is a PRISTINE worktree: `git worktree add --detach`, then
`just check <gate>`. A gate that passes with no build artifacts does not belong
in the build tier, whatever its history.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHECK_JUST = ROOT / "just" / "check.just"
WORKFLOWS = ROOT / ".github" / "workflows"
BASELINE = ROOT / ".config" / "ungated-gates.txt"

GATING_EVENTS = ("pull_request", "merge_group")


def lane_members(text: str, lane: str) -> list[str]:
    m = re.search(rf"^{lane}: \\\n((?:    [a-z0-9-]+ \\\n)+)", text, re.M)
    if not m:
        return []
    return re.findall(r"^    ([a-z0-9-]+) \\", m.group(1), re.M)


def lane_gates(text: str, lane: str) -> list[str]:
    """A lane's members, however that lane records them.

    `build-serial` is still an authored dependency list, so `lane_members`
    reads it. `fast-serial` has none: issue 1072 deleted the 218-name registry
    because two authors adding alphabetically adjacent gates insert at the same
    base line and conflict, and no `.gitattributes` driver runs on GitHub's
    server-side rebase to fix it. Its membership is DERIVED by
    `check-gate-lists.py`, and it is asked rather than parsed here — a regex
    over a dependency line that no longer exists returns [], which would make
    this gate silently vacuous the moment `just check fast` stopped being run
    by a gating job. That is the exact shape this file exists to prevent.
    """
    if lane.removesuffix("-serial") == "fast":
        out = subprocess.run(
            [sys.executable, str(ROOT / "scripts/check/check-gate-lists.py"),
             "--list", "fast"],
            capture_output=True, text=True, check=True,
        )
        return [ln for ln in out.stdout.split("\n") if ln.strip()]
    return lane_members(text, lane)


def lanes_run_by_gating_jobs() -> set[str]:
    """Which `just check <lane>` invocations a PR or merge_group can reach.

    Text-scanned, and deliberately OVER-approximating: a lane wrongly counted as
    gated merely means this file does not require its members to be listed,
    while under-counting would demand baseline lines for lanes that do gate.
    Fail toward calling a lane gated.
    """
    gated: set[str] = set()
    if not WORKFLOWS.is_dir():
        return gated
    for path in sorted(WORKFLOWS.glob("*.y*ml")):
        text = path.read_text(encoding="utf-8", errors="replace")
        wf_events = set(re.findall(r"^\s{2}(pull_request|merge_group):", text, re.M))
        for block in re.split(r"\n      - name:", text):
            guard = re.search(r"if:\s*(.+)", block)
            g = guard.group(1) if guard else ""
            # An explicit event allow-list that names neither gating event
            # cannot be reached by one.
            # Both quote styles: the guard that matters is written
            # `contains(fromJSON('["schedule","workflow_dispatch"]'), ...)`, so
            # the event names are DOUBLE-quoted inside a single-quoted string.
            # Matching only `'schedule'` saw no events, concluded the step was
            # unguarded, and reported zero ungated gates — this gate's own first
            # version was vacuous on the one step it exists to read.
            named = set(re.findall(r"['\"](pull_request|merge_group|schedule|"
                                   r"workflow_dispatch|push)['\"]", g))
            if named and not (named & set(GATING_EVENTS)):
                continue
            if not wf_events & set(GATING_EVENTS):
                continue
            for lane in re.findall(r"just\s+check\s+([a-z0-9-]+)", block):
                gated.add(lane)
    return gated


def read_baseline() -> list[str]:
    if not BASELINE.is_file():
        return []
    return [ln.strip() for ln in BASELINE.read_text(encoding="utf-8").split("\n")
            if ln.strip() and not ln.strip().startswith("#")]


def self_test() -> None:
    """Runs on the NORMAL path — `check-gate-selftests`."""
    txt = "fast-serial: \\\n    a \\\n    b \\\n\nbuild: \\\n    c \\\n"
    assert lane_members(txt, "fast-serial") == ["a", "b"]
    assert lane_members(txt, "build") == ["c"]
    assert lane_members(txt, "nope") == []
    # The guard shape that actually appears in gate.yml — double-quoted event
    # names inside a single-quoted fromJSON list. The first version of this
    # gate missed it and reported everything as gated.
    guard = "${{ contains(fromJSON('[\"schedule\",\"workflow_dispatch\"]'), github.event_name) }}"
    named = set(re.findall(r"['\"](pull_request|merge_group|schedule|"
                           r"workflow_dispatch|push)['\"]", guard))
    assert named == {"schedule", "workflow_dispatch"}, named


def main() -> int:
    self_test()

    text = CHECK_JUST.read_text(encoding="utf-8")
    gated_lanes = lanes_run_by_gating_jobs()

    ungated: list[str] = []
    for lane in ("fast-serial", "build-serial"):
        # A lane's LIST and its VERB are different names: `fast-serial` holds
        # the gates and `just check fast` runs them, and since issue 0993 the
        # same is true of `build-serial` / `build`. The workflows invoke the
        # verb, so check both spellings or a gated lane reads as ungated.
        verb = lane.removesuffix("-serial")
        if lane in gated_lanes or verb in gated_lanes:
            continue
        ungated.extend(lane_gates(text, lane))

    listed = set(read_baseline())
    actual = set(ungated)

    stale = sorted(listed - actual)
    fresh = sorted(actual - listed)
    problems = []
    for g in fresh:
        problems.append(
            f"  {g}: in a lane no pull_request or merge_group job runs, and not "
            f"in {BASELINE.relative_to(ROOT)}.")
    for g in stale:
        problems.append(
            f"  {g}: listed as ungated but is now reachable from a gating lane "
            f"— delete its line (this file may only shrink).")

    if not problems:
        print(f"check-gate-visibility: OK — {len(actual)} gate(s) run by no "
              f"merge-gating job, all acknowledged; may only decrease.")
        return 0

    print("check-gate-visibility: the ungated set drifted.", file=sys.stderr)
    for p in problems:
        print(p, file=sys.stderr)
    print("", file=sys.stderr)
    print("  A gate no pull request runs is a gate that rots (issue 0981: a red "
          "sat on", file=sys.stderr)
    print("  `main` for a day behind a green required check). Before adding a "
          "line, test", file=sys.stderr)
    print("  the gate in a PRISTINE worktree — if it passes with no build "
          "artifacts it does", file=sys.stderr)
    print("  not belong in the build tier at all.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())

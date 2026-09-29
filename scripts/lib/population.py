"""An empty population is not a pass — phase-472 W4.

A gate that examined NOTHING and printed OK is not narrow, it is OFF, and it
reads exactly like a gate that looked and found the tree clean. The 2026-09-28
audit found ten of them, each off for its own reason:

* the population's SPELLING moved (`check-cargo-custom-command-depfile` —
  issue 1304 respelled cargo as `"${_ffi_cargo}"`, the program regex stopped
  matching, and deleting DEPFILE at all three sites still printed OK);
* the population's LOCATION moved (`check-no-std-entry-emission`,
  `check-nested-workspace-excludes`, `check-profile-board-mirror`);
* the population's SOURCE went missing and read as empty
  (`check-interop-verdicts` — a missing tracked ledger erased 25 verdicts);
* the TOOL that reads it went missing and the gate exited 0
  (`check-workflow-runner-isolation`, `check-required-contexts-reportable`
  without PyYAML; `check-host-triple-literals` without rustc).

So the rule, for every gate that scans a population:

    PRINT the count examined, and FAIL on zero — unless the empty population
    is DECLARED, with a reason, at the call site.

A declared empty population is `check-vendor-fetch-pinned`'s "NOTHING TO CHECK
… not a pass": a measured fact about this tree, stated as such, never an OK.
A population the gate cannot even COMPUTE here (a missing tool) is neither —
it is a skip, and belongs in the `nros_check_skip` ledger
(`scripts/build/check-skip.sh`) via exit 78, or it is a failure. Never rc=0
with a success line.

The shell spelling is `scripts/lib/population.sh` (`nros_require_population`);
the two share their wording so a reader sees one rule.
"""

from __future__ import annotations

import sys

# Exit status for "this gate cannot run here" — the just recipe turns it into
# an `nros_check_skip` entry (see `check-zenoh-feature-off-compile`).
EXIT_CANNOT_RUN = 78


def require_population(n, what, *, gate, declared_empty=None, out=None, err=None):
    """Report the population a gate examined; return True iff it may proceed.

    `n` is the number of items examined (an int, or any sized collection).
    `what` names them in the plural ("cargo custom command(s)").
    `declared_empty` is the reason an empty population is legitimate HERE —
    a stated, reviewable claim at the call site. Without one, zero fails.

    Returns False (and prints why, on `err`) when the caller must fail.
    """
    out = out or sys.stdout
    err = err or sys.stderr
    count = n if isinstance(n, int) else len(n)
    if count > 0:
        print(f"{gate}: examined {count} {what}", file=out)
        return True
    if declared_empty:
        print(
            f"{gate}: NOTHING TO CHECK — 0 {what}. Declared: {declared_empty}\n"
            f"  That is a measured fact about this tree, not a pass.",
            file=out,
        )
        return True
    print(
        f"{gate}: FAILED — examined 0 {what}.\n"
        f"  An empty population is not a pass: the gate is OFF, not green\n"
        f"  (phase-472 W4). Either the population moved — its spelling, its\n"
        f"  location, or the tool that reads it — and the gate must be\n"
        f"  re-pointed at where it went, or the tree genuinely has none and the\n"
        f"  call site must DECLARE that, with a reason.",
        file=err,
    )
    return False


def self_test():
    """The helper's own negative control: zero must fail, declared must not."""
    import io

    def run(n, declared=None):
        o, e = io.StringIO(), io.StringIO()
        ok = require_population(n, "things", gate="probe", declared_empty=declared, out=o, err=e)
        return ok, o.getvalue() + e.getvalue()

    ok, text = run(0)
    assert not ok and "examined 0 things" in text, "zero undeclared must FAIL"
    ok, text = run(3)
    assert ok and "examined 3 things" in text, "a count must be PRINTED"
    ok, text = run([])
    assert not ok, "an empty collection is zero"
    ok, text = run(0, declared="none exist yet")
    assert ok and "NOTHING TO CHECK" in text and "not a pass" in text, \
        "a declared empty population is stated, never an OK"
    return 0


if __name__ == "__main__":
    sys.exit(self_test())

#!/usr/bin/env python3
"""CI may not tolerate a missing fixture — phase-411 W4.

`NROS_FIXTURES_OPTIONAL=1` converts an absent fixture binary into a `skip!`
instead of a failure. That is a legitimate LOCAL opt-in: a developer working on
one platform has not provisioned the others, and a run demanding all of them is
a run nobody performs.

It is never legitimate in CI. A lane names its scope, and under phase-411 naming
IS the specification — so a gap in what CI asked for is a failure, and nothing
else was expected in the first place. `host-tests.yml` set it unconditionally,
which made the lane's correctness depend on remembering to unset a variable, and
its own comment admitted as much ("the FULL `test-all` tier leaves the var
unset and still hard-fails").

This is why the reader in `nros-tests` SURVIVES while CI's use of it does not:
deleting it would remove a real local affordance to fix a CI-only defect. The
rule is about who sets it, so the gate is about where it appears.

Buildless: greps the workflow files.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import workflow_commands  # noqa: E402  phase-472 W1 — workflows AND composite actions
FLAG = "NROS_FIXTURES_OPTIONAL"
# A line that merely EXPLAINS the ban (like the one this gate exists for) is
# not a violation. A `#` comment is prose; an env assignment is policy.
# Anywhere on a non-comment line — an `env:` key, `export FLAG=1`, or an inline
# `run: FLAG=1 just …` (which the line-start form missed) — but never a READ
# (`${FLAG:-}`).
SETS = re.compile(rf"^(?!\s*#).*?(?<![{{$\w]){FLAG}\s*[:=]")


def analyze(lines):
    return [(i, l.rstrip()) for i, l in enumerate(lines, 1) if SETS.search(l)]


def selftest():
    """Both verdicts, on the normal path — phase-395."""
    assert analyze([f'          {FLAG}: "1"\n']), "an env assignment must be caught"
    assert analyze([f'    {FLAG}=1 just test-all\n']), "a shell assignment must be caught"
    assert not analyze([f'          # {FLAG} is GONE — see phase-411 W4\n']), \
        "a comment explaining the ban must not be a violation"
    assert not analyze(["          CARGO_BUILD_JOBS: \"2\"\n"]), "unrelated env is fine"
    # phase-472 W1
    assert analyze([f"      run: {FLAG}=1 just test-all\n"]), "an inline run: assignment"
    assert not analyze([f'      run: echo "${{{FLAG}:-}}"\n']), "a READ is not a set"
    workflow_commands.ci_files_self_test()


def main():
    selftest()
    problems = []
    # phase-472 W1 — workflows AND composite actions: an action's `run:` is a
    # CI step, and this read `.github/workflows/` only.
    files = workflow_commands.ci_files()
    for path in files:
        fn = os.path.relpath(path, ROOT)
        with open(path, encoding="utf-8") as fh:
            for lineno, line in analyze(fh.readlines()):
                problems.append(
                    f"{fn}:{lineno}: sets {FLAG} — CI names its scope, so a "
                    f"missing fixture inside that scope is a FAILURE, not a "
                    f"skip. Narrow the scope instead (`just test <scope>`).\n"
                    f"      {line.strip()}")
    if problems:
        sys.stderr.write("check-ci-no-fixture-tolerance: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1
    print(f"ci fixture tolerance: OK (no workflow or action sets {FLAG}; {len(files)} file(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())

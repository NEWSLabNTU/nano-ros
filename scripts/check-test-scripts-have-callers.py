#!/usr/bin/env python3
"""A test script with no caller is a test that runs nowhere.

WHY THIS EXISTS. On 2026-09-03, `e569d9a55` -- a commit about which Zephyr SDK
the CI image bakes -- deleted the `declared-qos-header` recipe and its fast-lane
entry from `just/check.just`. Its message says nothing about either; it was an
accidental loss in a rebase. `tests/cmake-declared-qos-header-tests.sh` then sat
TRACKED, passing, and reachable by nobody, guarding the delivery half of
phase-403 step 2 which had merged hours earlier.

Nothing caught it. Every gate we have asks whether some rule holds in the code;
none asks whether a test still has a way to run. A stranded script is invisible
in the worst possible way -- it looks exactly like a test that passes, because
running it by hand still passes.

This is the repo's own recurring class one level up: a mechanism that is correct
and unreachable. The instances so far -- `rx_buffer_hint` sizing nothing (0896),
the bound inventory with no reader (0963), the knob-delivery selftest behind a
flag, the NuttX `printk` arm, `just xrce setup`'s short-circuit -- were each
found by a human noticing, which is not a control.

WHAT IT CHECKS. Every `tests/*.sh` must be INVOKED -- named on a non-comment
line -- by a justfile, a CI workflow, or another script. Comments do not count;
see `callers_of` for why that is the whole gate rather than a detail.

Still deliberately weak in one way, and worth knowing: being invoked from a
recipe is not the same as that recipe being REACHED, so a script called only by
a recipe nobody runs still passes here. `check-lane-contracts` and
`check-gate-selftests` cover other parts of the same question. A weak check that
runs beats a strong one nobody wrote.

Exit 0 when every script has a caller, 1 otherwise.
"""

import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Where a caller may live. A script named ONLY inside `tests/` does not count:
# tests calling each other is not a route from CI or from a developer's `just`.
CALLER_DIRS = ["just", ".github", "scripts"]
CALLER_FILES = ["justfile"]


def _files_under(root):
    """TRACKED files in the caller roots.

    `git ls-files`, not `os.walk`, and the reason is measured: `scripts/`
    carries 7.8 GB across 60,533 files of build output on a working tree, and
    walking it read-line-by-line hangs this gate for minutes. The first version
    used `grep -rl`, which was fast only because it is C and short-circuits.

    Tracked-only is also the correct SEMANTIC rather than just the fast one: a
    caller that is not committed cannot run the script for anybody else.
    """
    r = subprocess.run(
        ["git", "-C", root, "ls-files", "--"] + CALLER_DIRS + CALLER_FILES,
        capture_output=True, text=True, check=False,
    )
    if r.returncode == 0:
        for rel in r.stdout.splitlines():
            if rel.strip():
                yield os.path.join(root, rel)
        return
    # Not a git tree (the self-test's tmpdir is one such): fall back to a walk,
    # which is safe there because the fixture is three files.
    targets = [os.path.join(root, d) for d in CALLER_DIRS]
    targets += [os.path.join(root, f) for f in CALLER_FILES]
    for t in targets:
        if os.path.isfile(t):
            yield t
        elif os.path.isdir(t):
            for dirpath, _dirs, files in os.walk(t):
                for fn in files:
                    yield os.path.join(dirpath, fn)


def callers_of(name, root):
    """Files that name `name` on a line that is NOT a comment.

    COMMENTS DO NOT COUNT, and that distinction is the whole gate. The first
    version of this file matched any mention, and its own mutation test proved
    it useless: the restored recipe's comment names the script, and so does THIS
    file's docstring -- so `cmake-declared-qos-header-tests.sh` would have had a
    permanent "caller" made entirely of prose about it being uncalled. A gate
    that its own explanation satisfies is not a gate.

    Line-based and deliberately crude: a `#`-leading line is a comment in every
    language in the caller roots (just, bash, YAML, Python). A trailing comment
    on a real command line still counts as a caller, which is correct -- the
    command is there.
    """
    hits = []
    for path in _files_under(root):
        try:
            with open(path, encoding="utf8", errors="replace") as fh:
                for line in fh:
                    if name not in line:
                        continue
                    if line.lstrip().startswith("#"):
                        continue
                    hits.append(path)
                    break
        except OSError:
            continue
    return hits


def scan(root):
    """(stranded, checked) for `tests/*.sh` under `root`."""
    tests_dir = os.path.join(root, "tests")
    if not os.path.isdir(tests_dir):
        return [], 0
    stranded, checked = [], 0
    for fn in sorted(os.listdir(tests_dir)):
        if not fn.endswith(".sh"):
            continue
        checked += 1
        if not callers_of(fn, root):
            stranded.append(fn)
    return stranded, checked


def self_test(quiet=False):
    """Negative control: the rule must FIRE on a stranded script.

    Runs on the NORMAL path, not behind a flag -- a control nobody runs decays
    into a comment, and `check-gate-selftests` holds this file to that.
    """
    with tempfile.TemporaryDirectory() as tmp:
        os.makedirs(os.path.join(tmp, "tests"))
        os.makedirs(os.path.join(tmp, "just"))
        called = os.path.join(tmp, "tests", "called-tests.sh")
        orphan = os.path.join(tmp, "tests", "orphan-tests.sh")
        open(called, "w").write("#!/bin/sh\n")
        open(orphan, "w").write("#!/bin/sh\n")
        open(os.path.join(tmp, "just", "check.just"), "w").write(
            "a-gate:\n    ./tests/called-tests.sh\n"
        )

        stranded, checked = scan(tmp)
        assert checked == 2, f"expected to check 2 scripts, checked {checked}"
        assert stranded == ["orphan-tests.sh"], \
            f"the rule must name the uncalled script and only it; got {stranded}"

        # A COMMENT naming it must NOT silence the rule. This case is the one
        # that matters: the first version of this gate failed it, and failed it
        # against its own docstring.
        open(os.path.join(tmp, "just", "check.just"), "a").write(
            "# orphan-tests.sh is mentioned here in prose only\n"
        )
        stranded, _ = scan(tmp)
        assert stranded == ["orphan-tests.sh"], \
            f"a comment mentioning a script must not count as a caller; got {stranded}"

        # And the intended escape: actually INVOKING it silences the rule.
        open(os.path.join(tmp, "just", "check.just"), "a").write(
            "another:\n    ./tests/orphan-tests.sh\n"
        )
        stranded, _ = scan(tmp)
        assert stranded == [], f"an invoked script must not be reported; got {stranded}"

    if not quiet:
        print("check-test-scripts-have-callers self-test: OK")
    return 0


def main(argv):
    if "--self-test" in argv:
        return self_test()
    # Always, not only behind the flag. See `scripts/check-board-tiers.py`.
    rc = self_test(quiet=True)
    if rc:
        return rc

    stranded, checked = scan(ROOT)
    if stranded:
        print("check-test-scripts-have-callers: test script(s) nothing can run:")
        for fn in stranded:
            print(f"  - tests/{fn}")
        print(
            "\n  Nothing in just/, justfile, .github/ or scripts/ names these.\n"
            "  A tracked test with no caller reads exactly like a passing test,\n"
            "  because running it by hand still passes -- which is how\n"
            "  `cmake-declared-qos-header-tests.sh` sat unreachable for a day\n"
            "  after a rebase dropped its recipe (2026-09-04).\n"
            "\n  Give it a recipe, or delete it if it is genuinely obsolete."
        )
        return 1
    print(f"check-test-scripts-have-callers: OK ({checked} script(s), all reachable)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

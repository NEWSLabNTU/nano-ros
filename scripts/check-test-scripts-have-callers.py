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

    `git ls-files`, not `os.walk`, and there is NO walk fallback -- the repo
    has a gate against exactly that (`check-no-tracked-file-find`), which
    measured 7m36s -> 0.8s for the same 232 paths and notes that pruning does
    not help because find still stats every directory it considers pruning. The
    first version of this file walked `scripts/`, which carries 7.8 GB across
    60,533 files of build output on a working tree, and hung for minutes.

    Tracked-only is also the correct SEMANTIC rather than merely the fast one:
    a caller that is not committed cannot run the script for anybody else.
    """
    r = subprocess.run(
        ["git", "-C", root, "ls-files", "--"] + CALLER_DIRS + CALLER_FILES,
        capture_output=True, text=True, check=False,
    )
    if r.returncode != 0:
        raise SystemExit(
            f"check-test-scripts-have-callers: `git ls-files` failed in {root}.\n"
            f"  {r.stderr.strip()}\n"
            "  This gate reads the INDEX on purpose and has no filesystem-walk\n"
            "  fallback -- see `check-no-tracked-file-find`, and the 7m36s -> 0.8s\n"
            "  it measured. The self-test builds a real git tree for the same\n"
            "  reason."
        )
    me = os.path.relpath(os.path.abspath(__file__), ROOT)
    for rel in r.stdout.splitlines():
        rel = rel.strip()
        if not rel:
            continue
        # THIS FILE IS NEVER A CALLER. It names the scripts it reports on -- in
        # the failure message below, on a non-comment line -- so without this it
        # is its own caller and can never report anything. That is the second
        # time this gate defeated itself with its own prose: the first was
        # counting comments, caught by the mutation test; this one was caught by
        # the same test after the comment fix, which is the argument for keeping
        # a mutation test rather than trusting a green.
        if rel == me:
            continue
        yield os.path.join(root, rel)


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
    def git(tmp, *args):
        subprocess.run(["git", "-C", tmp, *args], check=True,
                       capture_output=True, text=True)

    def stage(tmp):
        """`git ls-files` reads the INDEX, so the fixture has to be staged."""
        git(tmp, "add", "-A")

    with tempfile.TemporaryDirectory() as tmp:
        # A real git tree, because the gate reads the index and deliberately has
        # no walk fallback. Cheap: three files.
        git(tmp, "init", "-q")
        os.makedirs(os.path.join(tmp, "tests"))
        os.makedirs(os.path.join(tmp, "just"))
        called = os.path.join(tmp, "tests", "called-tests.sh")
        orphan = os.path.join(tmp, "tests", "orphan-tests.sh")
        open(called, "w").write("#!/bin/sh\n")
        open(orphan, "w").write("#!/bin/sh\n")
        open(os.path.join(tmp, "just", "check.just"), "w").write(
            "a-gate:\n    ./tests/called-tests.sh\n"
        )
        stage(tmp)

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
        stage(tmp)
        stranded, _ = scan(tmp)
        assert stranded == ["orphan-tests.sh"], \
            f"a comment mentioning a script must not count as a caller; got {stranded}"

        # And the intended escape: actually INVOKING it silences the rule.
        open(os.path.join(tmp, "just", "check.just"), "a").write(
            "another:\n    ./tests/orphan-tests.sh\n"
        )
        stage(tmp)
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

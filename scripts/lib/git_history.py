"""What a git ancestry answer is worth when the checkout's history is cut.

Issue 1476. `check-roadmap-commit-refs` failed tier 2 and the tier-2 nightly
with three citations it called "not an ancestor of HEAD" — all three of which
ARE on main, and were on main when the run started. The gate already had a
shallow guard and the guard did not fire, because it covered the wrong half of
the question.

# The two negatives, and why only one of them was guarded

A gate asks git two things about a commit id:

    git cat-file -e <sha>^{commit}          is the object HERE?
    git merge-base --is-ancestor <sha> TIP  is it in TIP's history?

Both can answer NO for a reason that is about the CHECKOUT rather than about
the commit, and a shallow clone produces each of them:

  * the object was never fetched                 -> `cat-file` says no
  * the object IS here but `.git/shallow` grafts the tip parentless, so no
    path from TIP reaches it                     -> `merge-base` says no

The gate guarded the first and not the second, so it stayed exposed to the
one that CI actually produces. Measured on the tier-2 runner: it is
self-hosted, so `/home/runner/_work/nano-ros/nano-ros` is a PERSISTENT
workspace. `actions/checkout` runs `git clean -ffdx`, which removes files and
never objects, then fetches the new tip at `--depth=1`. Every earlier run's
tip therefore stays in the object store while the history between them does
not — so `cat-file` succeeds on exactly the commits a recent doc cites, the
absent-object guard is bypassed, and `merge-base` answers a confident false.
A `--depth 1` clone that is FRESH would have failed the first test and been
reported as unverifiable; it is the workspace's own memory that turns a skip
into a wrong verdict.

# The rule

Truncation can only manufacture a FALSE NEGATIVE, never a false positive: if
git finds a path, the path exists. So a `True` from any clone counts, and a
`False` from a truncated one becomes "cannot tell here" — which is a verdict
the caller must REPORT (issue 1043's NOT VERIFIED, issue 0650's ledger), never
one it may quietly read as OK.

The predicate is `--is-shallow-repository` and that is deliberate: a PARTIAL
clone (`--filter=blob:none`) holds every commit and fetches blobs lazily, so
its ancestry answers are sound, and a single-branch clone holds the full
ancestry of the branch it has. Only a shallow clone grafts commits parentless,
and only that turns a true ancestor into "not an ancestor".

This file is the ONE spelling of that rule for Python; `scripts/lib/
git-history.sh` is its shell twin, same names. `check-ancestry-truncation`
keeps a second one from appearing.

Self-test:  python3 scripts/lib/git_history.py --self-test
"""

from __future__ import annotations

import subprocess
import sys

# The three outcomes, as a value: True / False / None. `None` is not "no" —
# it is "this checkout cannot answer", which is a different thing to print.
UNKNOWN = None


def _git(cwd, *args) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", *args], cwd=str(cwd), capture_output=True, text=True
    )


def is_truncated(cwd) -> bool:
    """True when this checkout's COMMIT HISTORY is cut, i.e. a shallow clone.

    Not "is this clone complete" — see the module docstring for why a partial
    or single-branch clone is not this and must keep its teeth.
    """
    return _git(cwd, "rev-parse", "--is-shallow-repository").stdout.strip() == "true"


def has_commit(cwd, sha: str) -> bool:
    """Whether the object store here holds `sha` as a commit.

    Presence, not reachability — `cat-file` answers for a commit left behind by
    a rebase just as readily as for one on main.
    """
    return _git(cwd, "cat-file", "-e", f"{sha}^{{commit}}").returncode == 0


def interpret_ancestry(is_ancestor: bool, truncated: bool):
    """What a `merge-base --is-ancestor` answer is worth. ASYMMETRIC.

    True stays True in any clone. False becomes UNKNOWN when the history is
    truncated, because truncation manufactures false negatives.
    """
    if is_ancestor:
        return True
    return UNKNOWN if truncated else False


def ancestry(cwd, sha: str, tip: str = "HEAD", truncated=None):
    """True / False / UNKNOWN — is `sha` in `tip`'s history, as far as we can tell.

    UNKNOWN covers both negatives the module docstring names: the object is
    absent, or it is present with the path to it cut. A caller that wants to
    tell those apart asks `has_commit` first; a caller that only wants a
    verdict does not need to.
    """
    if truncated is None:
        truncated = is_truncated(cwd)
    if not has_commit(cwd, sha):
        # An object this checkout does not have is a REAL failure in a full
        # clone — a citation of a commit that exists nowhere is the worst form
        # of a dangling reference — and says nothing at all in a truncated one.
        return UNKNOWN if truncated else False
    rc = _git(cwd, "merge-base", "--is-ancestor", sha, tip).returncode
    if rc not in (0, 1):
        # A malformed revision or a git that could not run: not a measurement.
        return UNKNOWN
    return interpret_ancestry(rc == 0, truncated)


# The truth table, as data, so both the self-test and any gate that wants to
# assert the rule read the same rows.
TRUTH_TABLE = [
    # (is_ancestor, truncated, expected)
    (True, False, True),
    (True, True, True),  # truncation never weakens a positive
    (False, False, False),
    (False, True, UNKNOWN),  # issue 1476: the arm that was missing
]


def self_test() -> int:
    bad = 0
    for is_ancestor, truncated, want in TRUTH_TABLE:
        got = interpret_ancestry(is_ancestor, truncated)
        if got is not want:
            print(
                f"git_history self-test: interpret_ancestry({is_ancestor}, "
                f"{truncated}) = {got!r}, wanted {want!r}",
                file=sys.stderr,
            )
            bad += 1
    if bad:
        return 1
    print(f"git_history self-test: OK ({len(TRUTH_TABLE)} row(s))")
    return 0


if __name__ == "__main__":
    sys.exit(self_test())

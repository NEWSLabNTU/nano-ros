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

Reading old TREES or BLOBS is a different question, with a different
predicate (a partial clone fails it): see "History OBJECT reads" below.

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


# ---------------------------------------------------------------------------
# History OBJECT reads: trees and blobs of old commits (issue 2442).
#
# Everything above is about COMMITS. A path walk (`git log -- <path>`,
# `--name-status`, `-p`), an old tree (`git ls-tree <rev>`, `git show
# <rev>:<path>`, `git diff <old> ...`) or an old blob needs more: the TREES of
# those commits. A shallow clone does not have them either, and the predicate
# above already says so. A PARTIAL clone is the case it lets through on
# purpose: CI provisions history as `git fetch --filter=tree:0 --unshallow`
# (scripts/ci/provision-gate-lane.sh), so every commit is whole and not
# shallow, while every tree and blob below the fetched tips lives on the
# promisor remote. Git fetches a missing one LAZILY, one object per round
# trip, from inside whatever command asked: `check-toolchain-paths-persist`'s
# `git log --diff-filter=DR --name-status -- cmake/toolchain/` ran into the
# 40-minute nextest cap that way, and a reproduced clone was still walking when
# `timeout 90` killed it.
#
# So a gate that reads history OBJECTS asks two things first, and runs the
# read bounded:
#
#   history_objects_unreadable(cwd) -> None, or why a whole-history object
#                                      read cannot be answered here
#   tree_present(cwd, rev)          -> is <rev>'s whole tree HERE, asked
#                                      without fetching anything
#   history_git(cwd, *args)         -> run one read bounded: no prompt, no
#                                      lazy fetch where git honours it, stdin
#                                      closed, a timeout (HistoryUnknown)
#
# An unreadable answer is UNKNOWN, reported as NOT VERIFIED through the skip
# ledger (scripts/lib/check_skip.py), never a pass. `is_partial` carries the
# load on a git that ignores GIT_NO_LAZY_FETCH (it arrived in 2.44, though
# Ubuntu backports it to 2.34): the walk is never STARTED in a partial clone.
# `check-history-object-reads` keeps a raw read of this kind from appearing
# outside this file and its shell twin.

import os  # noqa: E402

# The environment every bounded read runs under. GIT_TERMINAL_PROMPT=0: a
# lazy fetch that needs credentials fails instead of waiting on a tty.
BOUNDED_ENV = {"GIT_TERMINAL_PROMPT": "0", "GIT_NO_LAZY_FETCH": "1"}
HISTORY_TIMEOUT_S = 60


class HistoryUnknown(Exception):
    """A bounded history read did not finish or could not be answered here."""


def _bounded(cwd, args, timeout, fetch=False):
    env = {**os.environ, **BOUNDED_ENV}
    if fetch:
        env.pop("GIT_NO_LAZY_FETCH")
    return subprocess.run(
        ["git", "-C", str(cwd), *args], capture_output=True, text=True,
        env=env, stdin=subprocess.DEVNULL, timeout=timeout,
    )


def is_partial(cwd) -> bool:
    """True for a PARTIAL clone: some objects live only on a promisor remote.

    Its commits can be whole (so `is_truncated` is False and ancestry answers
    stand), but a tree or blob below a fetched tip is fetched lazily, one
    object at a time, by whatever command needs it.
    """
    r = _bounded(cwd, ["config", "--get-regexp",
                       r"^(extensions\.partialclone|remote\..*\.promisor)$"],
                 HISTORY_TIMEOUT_S)
    return r.returncode == 0 and r.stdout.strip() != ""


def history_objects_unreadable(cwd):
    """None when a read of OLD trees/blobs across history can be answered here.

    Otherwise the reason, for a NOT VERIFIED line: the clone is shallow (the
    commits are cut) or partial (their trees are on the network).
    """
    if is_truncated(cwd):
        return "this clone is shallow, so history before its tip is absent"
    if is_partial(cwd):
        return ("this clone is partial (trees or blobs live on a promisor "
                "remote), and reading old trees would fetch them one object "
                "at a time")
    return None


def tree_present(cwd, rev: str) -> bool:
    """Whether <rev>'s whole tree (every subtree; blobs not asked) is HERE.

    Asked with `rev-list --missing=print`, which reports a missing object
    instead of fetching it, on every git this repo meets (2.34 included).
    `cat-file -e <rev>^{tree}` is NOT this: in a partial clone it fetches.
    """
    try:
        r = _bounded(cwd, ["rev-list", "--objects", "--no-object-names",
                           "--missing=print", "--filter=blob:none",
                           "--no-walk", rev], HISTORY_TIMEOUT_S)
    except subprocess.TimeoutExpired:
        return False
    if r.returncode != 0:
        return False
    return not any(ln.startswith("?") for ln in r.stdout.splitlines())


def history_git(cwd, *args, timeout: int = HISTORY_TIMEOUT_S,
                check: bool = True, fetch: bool = False) -> subprocess.CompletedProcess:
    """Run ONE git read of history objects, bounded.

    Raises HistoryUnknown when it does not finish in `timeout` s, or (with
    `check`) when git fails: neither is a measurement. Callers ask
    `history_objects_unreadable` or `tree_present` FIRST, so a partial clone
    never reaches the read on a git that would fetch anyway.

    `fetch=True` allows a LAZY fetch, for a read that is a handful of objects
    by construction (one path at one commit: its trees and its blob). It is
    still bounded by the timeout. Never pass it for a walk over commits.
    """
    try:
        r = _bounded(cwd, list(args), timeout, fetch)
    except subprocess.TimeoutExpired as e:
        raise HistoryUnknown(
            f"`git {' '.join(args)}` did not finish in {timeout} s") from e
    if check and r.returncode != 0:
        raise HistoryUnknown(
            f"`git {' '.join(args)}` exited {r.returncode}: {r.stderr.strip()}")
    return r


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
    bad = _object_reads_self_test()
    if bad:
        return 1
    print(f"git_history self-test: OK ({len(TRUTH_TABLE)} row(s), object reads "
          "on a full, a shallow and a partial clone)")
    return 0


def _object_reads_self_test() -> int:
    """Negative control: build a full, a shallow and a PARTIAL clone and ask.

    The partial clone is the CI shape (depth 1, then `--filter=tree:0
    --unshallow`) from a throwaway source repo under $TMPDIR that allows
    filters; nothing touches the repository this runs in.
    """
    import tempfile
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from git_hook_env import nros_clear_inherited_git_env

    saved = dict(os.environ)
    nros_clear_inherited_git_env()
    os.environ.update({"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t",
                       "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t",
                       "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull})
    bad = 0
    try:
        with tempfile.TemporaryDirectory(prefix="git-history-selftest-") as tmp:
            def run(*a, cwd=tmp):
                subprocess.run(["git", *a], cwd=cwd, check=True,
                               capture_output=True, stdin=subprocess.DEVNULL)
            src = os.path.join(tmp, "src")
            run("init", "-q", src)
            for i in range(4):
                d = os.path.join(src, "d", str(i))
                os.makedirs(d)
                with open(os.path.join(d, "f"), "w") as f:
                    f.write(f"{i}\n")
                run("add", "-A", cwd=src)
                run("commit", "-qm", f"c{i}", cwd=src)
            run("config", "uploadpack.allowFilter", "true", cwd=src)
            url = "file://" + src
            shallow = os.path.join(tmp, "shallow")
            partial = os.path.join(tmp, "partial")
            run("clone", "-q", "--depth", "1", url, shallow)
            run("clone", "-q", "--depth", "1", url, partial)
            run("fetch", "-q", "--no-tags", "--filter=tree:0", "--unshallow",
                "origin", cwd=partial)
            old = "HEAD~2"
            checks = [
                ("full: readable", history_objects_unreadable(src) is None),
                ("full: old tree here", tree_present(src, old)),
                ("shallow: unreadable", history_objects_unreadable(shallow) is not None),
                ("partial: not truncated", not is_truncated(partial)),
                ("partial: is_partial", is_partial(partial)),
                ("partial: unreadable", history_objects_unreadable(partial) is not None),
                ("partial: tip tree here", tree_present(partial, "HEAD")),
                ("partial: old tree absent", not tree_present(partial, old)),
                # The probe must not have fetched what it reported absent.
                ("partial: probe fetched nothing", not tree_present(partial, old)),
                ("bounded read of HEAD's tree",
                 "d/" in history_git(partial, "ls-tree", "-r", "--name-only", "HEAD").stdout),
            ]
            try:
                history_git(partial, "log", "--name-only", "--format=", timeout=0.001)
                checks.append(("a timeout is HistoryUnknown", False))
            except HistoryUnknown:
                checks.append(("a timeout is HistoryUnknown", True))
            for name, ok in checks:
                if not ok:
                    print(f"git_history self-test: {name}: FAILED", file=sys.stderr)
                    bad += 1
    finally:
        os.environ.clear()
        os.environ.update(saved)
    return bad


if __name__ == "__main__":
    sys.exit(self_test())

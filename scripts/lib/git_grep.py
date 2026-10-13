"""`git grep` for a checker: "no match" is a finding, a failure is not.

Issue 1816. `git grep` exits 0 for "matched", 1 for "matched nothing", and 128
when it could not search at all — not a repository, a pattern git's regex
engine rejects, a pathspec it cannot parse. The natural Python spelling,

    out = subprocess.run(["git", "grep", ...], capture_output=True).stdout

reads all three through `stdout`, and the third prints nothing there. So a
gate that FORBIDS a pattern turns "git did not run" into "the pattern is
absent" and passes. `check-deprecated-cmake-names` (now `check-retired-cmake-names`) reported OK over a `git
archive` export, where git searched nothing, and 13 of the 14 call sites
already on main behaved the same way — a gate that cannot fail outside a
checkout, or on a regex git cannot parse, looks exactly like a gate that
measured a clean tree.

It is the same defect `scripts/lib/grep-q.sh` answers for plain `grep` (issue
0726): one exit code means "absent", a different one means "I did not look",
and only a caller who asks for the status can tell them apart. Here the
caller does not ask — the helper refuses to return a verdict from a search
that did not run, and raises instead. Every caller is a checker or a
generator, so there is nothing useful a caller could do with a failed search
except stop.

`check-git-grep-status` keeps a raw `git grep` subprocess from reappearing in
`scripts/`. This file is its only exemption.

Self-test:  python3 scripts/lib/git_grep.py --self-test
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from os import PathLike
from pathlib import Path


class GitGrepFailed(RuntimeError):
    """`git grep` exited with neither "matched" nor "matched nothing"."""


def run(args: list[str], cwd: str | PathLike[str]) -> str:
    """Run `git -C <cwd> grep <args…>` and return its stdout.

    An empty string means git SEARCHED and matched nothing. Anything other
    than exit 0/1 raises `GitGrepFailed`, carrying git's own stderr."""
    r = subprocess.run(["git", "-C", str(cwd), "grep", *args],
                       capture_output=True, text=True)
    if r.returncode not in (0, 1):
        raise GitGrepFailed(
            f"`git grep {' '.join(args)}` in {cwd} exited {r.returncode} — it did not "
            f"search, so it has no answer: {r.stderr.strip() or '(no stderr)'}"
        )
    return r.stdout


def lines(args: list[str], cwd: str | PathLike[str]) -> list[str]:
    return run(args, cwd).splitlines()


def any_match(args: list[str], cwd: str | PathLike[str]) -> bool:
    """`git grep -q`, with a failed search raising rather than reading False."""
    return bool(run(["-l", *args], cwd))


def self_test() -> None:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from git_hook_env import nros_clear_inherited_git_env

    # The victim repositories are built HERE, so an inherited `GIT_DIR` must
    # not aim them at the caller's (issues 0986/1465).
    saved = dict(os.environ)
    nros_clear_inherited_git_env()
    try:
        with tempfile.TemporaryDirectory() as d:
            # Outside any repository: the exact defect.
            try:
                run(["-n", "x"], d)
            except GitGrepFailed:
                pass
            else:
                raise AssertionError("a search outside a repository returned a verdict")
            subprocess.run(["git", "init", "-q", d], check=True)
            (Path(d) / "f").write_text("needle\n")
            subprocess.run(["git", "-C", d, "add", "f"], check=True)
            assert lines(["-n", "needle"], d) == ["f:1:needle"]
            assert run(["-n", "absent"], d) == ""
            assert any_match(["needle"], d) and not any_match(["absent"], d)
            # A pattern git's regex engine rejects is the other way to not search.
            try:
                run(["-E", "a{"], d)
            except GitGrepFailed:
                pass
            else:
                raise AssertionError("an unparseable pattern returned a verdict")
    finally:
        os.environ.clear()
        os.environ.update(saved)


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        print("git_grep: self-test OK")
        sys.exit(0)
    sys.exit("usage: git_grep.py --self-test")

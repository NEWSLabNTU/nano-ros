"""A filesystem walk that stays inside THIS repository — issue 1565.

`scripts/lib/tracked.py` answers "which files does this tree TRACK" through the
index, and that is the right tool whenever the question is about tracked
content. This module is for the other case, where a walk is genuinely needed —
untracked build output, a generated header, a sidecar `nros sync` wrote — and
answers the question a walk is easy to get wrong: WHICH REPOSITORY is it
counting?

A plain `os.walk` / `find` from the checkout root answers "what is on this disk
under this path", and in this checkout that is a strictly larger set than the
repository:

* `.claude/worktrees/*` — every parallel agent session's linked worktree, each
  holding its own build output. Measured 2026-09-29: `find . -name
  nros_declared_qos_generated.h` returned 19 where this tree held 1.
* submodules — a walk goes straight through a gitlink into another repository.
  Same day: 44 `*.contract.yaml` against 17 tracked, the 27-file gap all in
  `play_launch`.

Both are the same shape: a directory that holds a `.git` entry (a FILE for a
linked worktree and a submodule, a directory for a nested clone) is the root of
ANOTHER repository. So the rule here is structural rather than a list of names
— a walk stops at a nested repository — plus `.claude/` at the top, which is
session state rather than tree content even where it holds no worktree.

    from repo_walk import walk
    for dirpath, dirnames, filenames in walk(REPO):
        ...

or, for a walk that already prunes for other reasons, call `prune(dirpath,
dirnames, root)` first thing in the loop body.

`python3 scripts/lib/repo_walk.py [--root DIR] PATTERN…` prints the files whose
NAME matches any fnmatch PATTERN, scoped this way — the tool to reach for in
ad-hoc analysis, which is where both of 1565's miscounts were made.
"""

from __future__ import annotations

import fnmatch
import os
import sys

# Session state at the checkout root, never tree content.
TOP_LEVEL_FOREIGN = (".claude",)


def is_nested_repo(path: str) -> bool:
    """`path` is the root of another repository (a submodule, a linked
    worktree, a nested clone). `exists`, not `isdir`: for the first two the
    `.git` entry is a FILE (issue 1336)."""
    return os.path.lexists(os.path.join(path, ".git"))


def prune(dirpath: str, dirnames: list, root: str) -> None:
    """Drop, in place, every child of `dirpath` that is not this repository."""
    top = os.path.abspath(dirpath) == os.path.abspath(root)
    dirnames[:] = [
        d for d in dirnames
        if d != ".git"
        and not (top and d in TOP_LEVEL_FOREIGN)
        and not is_nested_repo(os.path.join(dirpath, d))
    ]


def walk(root: str):
    """`os.walk(root)` that never leaves the repository rooted at `root`."""
    # walk-ok: this IS the scoped walk, for files git cannot see (build
    # output, generated sidecars); tracked content goes through tracked.py.
    for dirpath, dirnames, filenames in os.walk(root):
        prune(dirpath, dirnames, root)
        yield dirpath, dirnames, filenames


def _self_test() -> int:
    import tempfile

    with tempfile.TemporaryDirectory() as t:
        def mk(rel, git=None):
            os.makedirs(os.path.join(t, rel), exist_ok=True)
            if git == "file":
                with open(os.path.join(t, rel, ".git"), "w") as fh:
                    fh.write("gitdir: elsewhere\n")
            elif git == "dir":
                os.makedirs(os.path.join(t, rel, ".git"), exist_ok=True)
            open(os.path.join(t, rel, "hit.h"), "w").close()

        mk("packages/a")
        mk(".claude/worktrees/agent-x", git="file")
        mk(".claude/notes")
        mk("third-party/sub", git="file")
        mk("examples/clone", git="dir")
        mk("examples/leaf/.claude")  # only the TOP-level `.claude` is foreign
        os.makedirs(os.path.join(t, ".git", "objects"))
        got = sorted(os.path.relpath(dp, t) for dp, _, fs in walk(t) if "hit.h" in fs)
        want = ["examples/leaf/.claude", "packages/a"]
        if got != want:
            print(f"repo_walk self-test FAIL: {got} != {want}", file=sys.stderr)
            return 1
    return 0


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return _self_test()
    root = "."
    if "--root" in argv:
        i = argv.index("--root")
        root = argv[i + 1]
        argv = argv[:i] + argv[i + 2:]
    if not argv:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: repo_walk.py [--root DIR] PATTERN…", file=sys.stderr)
        return 2
    for dirpath, _dirnames, filenames in walk(root):
        for f in sorted(filenames):
            if any(fnmatch.fnmatch(f, p) for p in argv):
                print(os.path.join(dirpath, f))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

#!/usr/bin/env python3
"""issue 0718 — a build directory written into an example leaf must be ignored.

# What this protects

An example leaf is a standalone copy-out project (RFC-0026), so its build
output lands *inside* the tree rather than in a shared `build/`. Every such
directory has to be named in the leaf's own `.gitignore`: there is no repo-root
pattern for `examples/**/build*`, because `build` is also a legitimate tracked
name elsewhere and a blanket rule would hide real files.

# The defect it was written for

`just threadx_riscv64 build-fixtures` builds the six `rust/` leaves for BOTH
RMWs, via `build_threadx_cmake_rmw <leaf> cyclonedds build-cyclonedds` and
`build_threadx_cmake_rmw <leaf> zenoh build-zenoh`. Their `.gitignore` files
listed `/build-cyclonedds/` and not `/build-zenoh/`, so a fixture build left
six untracked directories of object files sitting in `git status` — which is
the state in which `git add -A` scoops build output into a commit, the hazard
CLAUDE.md already bans the blanket add for. The c/ and cpp/ leaves of the same
platform listed both, so the asymmetry was invisible from any one leaf.

# The rule

No directory under `examples/` whose basename begins with `build` may be
untracked. It is either ignored by the leaf that owns it, or (rarely) tracked
on purpose.

# Detection

This asks git, not the filesystem: `git status --porcelain` reports exactly the
paths that are neither tracked nor ignored. That makes the check free of its
own path logic and correct for every platform's naming, at the cost of being a
POST-BUILD check — on a tree that has never been built there is nothing to see.
That is the honest shape for this property: which build directories a leaf
gets is decided by shell variables inside the `just` recipes, so a static parse
would have to guess, and a guess here is either noise or a false green.

Run: python3 scripts/check-example-leaf-build-dirs.py
"""

import subprocess
import sys


def main() -> int:
    out = subprocess.run(
        ["git", "status", "--porcelain", "--untracked-files=normal", "--", "examples/"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout

    offenders = []
    for line in out.splitlines():
        if not line.startswith("?? "):
            continue
        path = line[3:].strip().strip('"')
        # `git status` reports an untracked DIRECTORY with a trailing slash and
        # does not descend into it, so the leaf's build dir arrives whole.
        if not path.endswith("/"):
            continue
        basename = path.rstrip("/").rsplit("/", 1)[-1]
        if basename.startswith("build"):
            offenders.append(path)

    if offenders:
        print("example leaf build directories are untracked and unignored:", file=sys.stderr)
        for path in offenders:
            leaf = path.rstrip("/").rsplit("/", 1)[0]
            name = path.rstrip("/").rsplit("/", 1)[-1]
            print(f"  {path}", file=sys.stderr)
            print(f"      add `/{name}/` to {leaf}/.gitignore", file=sys.stderr)
        print(
            "\nA leaf builds in-tree (RFC-0026), so each build directory it is given\n"
            "must be named in its own .gitignore — see issue 0718.",
            file=sys.stderr,
        )
        return 1

    print("example leaf build dirs: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())

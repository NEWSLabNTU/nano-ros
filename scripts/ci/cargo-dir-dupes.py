#!/usr/bin/env python3
"""cargo-dir-dupes.py <root> — how much of a tree of cargo target dirs is the
SAME compiled unit stored more than once.

phase-559 W5b / issue 2318. `build/corrosion-cargo` holds one cargo directory
per CMake leaf (issue 0616: a target dir serves one workspace root). Issue 2318
measured that most of each directory is HOST tooling (build scripts,
proc-macros) whose units are byte-identical across leaves of one key. Before
anyone proposes sharing them, this says how many bytes sharing would return
on a given tree:

- per cargo directory: total, host half (`<profile>/`), target half
  (`<triple>/<profile>/`);
- across the tree: every unit file (`deps/lib<name>-<hash>.{rlib,rmeta,so,a}`
  and `build/<pkg>-<hash>/`) present in more than one cargo directory, and the
  bytes the extra copies hold.

A unit's metadata hash already encodes its features, dependencies, profile and
path spelling, so two files with one name ARE one unit. Read-only; prints a
report and exits 0. A missing root prints that and exits 0 (the summary it is
called from must not fail on it).
"""

from __future__ import annotations

import os
import re
import sys
from collections import defaultdict

UNIT = re.compile(r"^lib.+-[0-9a-f]{16}\.(rlib|rmeta|so|a|dylib)$")
BUILD_UNIT = re.compile(r"^.+-[0-9a-f]{16}$")


def size_of(path: str) -> int:
    total = 0
    for dirpath, _dirs, files in os.walk(path, followlinks=False):  # walk-ok: a build dir of untracked cargo output
        for f in files:
            try:
                total += os.lstat(os.path.join(dirpath, f)).st_size
            except OSError:
                pass
    return total


def cargo_dirs(root: str):
    """Directories holding a cargo `CACHEDIR.TAG` (cargo writes one per target dir)."""
    for dirpath, dirs, files in os.walk(root, followlinks=False):  # walk-ok: a build dir of untracked cargo output
        if "CACHEDIR.TAG" in files:
            yield dirpath
            dirs[:] = []


def units(cargo_dir: str):
    """(key, path, bytes) for every unit under a cargo dir's `deps/` and `build/`."""
    for dirpath, dirs, files in os.walk(cargo_dir, followlinks=False):  # walk-ok: a build dir of untracked cargo output
        base = os.path.basename(dirpath)
        if base == "deps":
            for f in files:
                if UNIT.match(f):
                    p = os.path.join(dirpath, f)
                    rel = os.path.relpath(p, cargo_dir)
                    yield rel, p, os.lstat(p).st_size
            dirs[:] = []
        elif base == "build":
            for d in list(dirs):
                if BUILD_UNIT.match(d):
                    p = os.path.join(dirpath, d)
                    rel = os.path.relpath(p, cargo_dir)
                    yield rel, p, size_of(p)
            dirs[:] = []


def halves(cargo_dir: str):
    """(host, target) bytes: a profile dir at depth 1 is host; deeper is a triple's."""
    host = target = 0
    for entry in os.scandir(cargo_dir):
        if not entry.is_dir(follow_symlinks=False):
            continue
        if os.path.isdir(os.path.join(entry.path, "deps")):
            host += size_of(entry.path)
        else:
            target += size_of(entry.path)
    return host, target


def gb(n: int) -> str:
    return f"{n / 1e9:.1f} G"


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print((__doc__ or "").splitlines()[0], file=sys.stderr)
        return 2
    root = argv[1]
    if not os.path.isdir(root):
        print(f"cargo-dir-dupes: {root}: no such directory")
        return 0
    seen: dict[str, list[int]] = defaultdict(list)
    rows = []
    for cd in sorted(cargo_dirs(root)):
        host, target = halves(cd)
        rows.append((host + target, host, target, os.path.relpath(cd, root)))
        for key, _p, n in units(cd):
            seen[key].append(n)
    rows.sort(reverse=True)
    total = sum(r[0] for r in rows)
    print(f"cargo-dir-dupes: {root}: {len(rows)} cargo dir(s), {gb(total)}")
    print(f"  host half {gb(sum(r[1] for r in rows))}, target half {gb(sum(r[2] for r in rows))}")
    for t, h, tg, rel in rows[:25]:
        print(f"  {gb(t):>8}  host {gb(h):>8}  target {gb(tg):>8}  {rel}")
    dup = {k: v for k, v in seen.items() if len(v) > 1}
    extra = sum(sum(v) - max(v) for v in dup.values())
    print(f"  units stored in more than one cargo dir: {len(dup)}; "
          f"bytes in the extra copies: {gb(extra)}")
    worst = sorted(dup.items(), key=lambda kv: sum(kv[1]) - max(kv[1]), reverse=True)[:10]
    for k, v in worst:
        print(f"    {gb(sum(v) - max(v)):>8} in {len(v) - 1} extra copies  {k}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

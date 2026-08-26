#!/usr/bin/env python3
"""Remove shared Corrosion cargo directories nothing points at. Issue 0805.

`nros_share_corrosion_cargo_dir()` names its directory by a HASH of the leaf's
resolved configuration, and replaces `<leaf-build>/cargo` with a symlink to it.
So when any key input changes — a profile, a capability, a normalisation fix —
the old directory stops being referenced and simply stays on disk. One such
change in this repo stranded 8 directories holding 8.6 GB.

That is the issue-0500 SDK-store class one directory over: a store that only
grows. This is its GC.

## Reachability, not age

A key directory is LIVE if and only if some leaf's `cargo` symlink resolves to
it. That is exact — no heuristic about mtimes, no "probably unused". Everything
else is unreachable by construction: nothing can find it, because the only way
in is through a symlink a configure wrote.

Reporting is the default. `--prune` deletes.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent


def shared_root() -> Path:
    return ROOT / "build" / "corrosion-cargo"


def live_targets() -> set[Path]:
    """Every directory some leaf's `cargo` symlink currently resolves to."""
    live: set[Path] = set()
    for base in ("examples", "packages"):
        start = ROOT / base
        if not start.is_dir():
            continue
        # walk-ok: hunts symlinks inside UNTRACKED build dirs — the git index
        # cannot see build output, which is the whole point of this sweep.
        for dirpath, dirnames, _ in os.walk(start):
            # `cargo` is always directly inside a build dir; never descend into
            # one, or this walks every object file in the tree.
            if "cargo" in dirnames:
                p = Path(dirpath) / "cargo"
                if p.is_symlink():
                    live.add(Path(os.path.realpath(p)))
                    dirnames.remove("cargo")
            # Do not descend into the shared store itself.
            dirnames[:] = [d for d in dirnames if d != ".git"]
    return live


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--prune", action="store_true", help="delete unreachable dirs")
    args = ap.parse_args()

    root = shared_root()
    if not root.is_dir():
        print("gc-shared-cargo: no shared store — nothing to do.")
        return 0

    live = live_targets()
    total_freed = 0
    unreachable = []
    for platform_dir in sorted(root.iterdir()):
        if not platform_dir.is_dir():
            continue
        for entry in sorted(platform_dir.iterdir()):
            if entry.suffix == ".key" or not entry.is_dir():
                continue
            if entry.resolve() in live:
                continue
            key_file = entry.with_suffix(".key")
            key = key_file.read_text().strip() if key_file.exists() else "(no key file)"
            size = int(subprocess.run(
                ["du", "-sb", str(entry)], capture_output=True, text=True
            ).stdout.split()[0] or 0)
            unreachable.append((entry, key_file, key, size))
            total_freed += size

    if not unreachable:
        print(f"gc-shared-cargo: OK — every key dir under {root.relative_to(ROOT)} "
              f"is referenced by a leaf ({len(live)} live target(s)).")
        return 0

    verb = "removing" if args.prune else "unreachable (re-run with --prune to delete)"
    print(f"gc-shared-cargo: {len(unreachable)} {verb}, "
          f"{total_freed / 1e9:.1f} GB")
    for entry, key_file, key, size in unreachable:
        print(f"  {size / 1e9:6.2f} GB  {entry.relative_to(root)}")
        print(f"            key: {key}")
        if args.prune:
            shutil.rmtree(entry, ignore_errors=True)
            key_file.unlink(missing_ok=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())

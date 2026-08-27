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

A key directory is LIVE if some leaf still refers to it. No heuristic about
mtimes, no "probably unused".

There are TWO ways a leaf refers to one, and counting only the first is a bug
this tool shipped with:

* **A `cargo` symlink** — the Corrosion consumers. Corrosion computes its own
  `--target-dir` from `CMAKE_BINARY_DIR`, so the only way to redirect it is to
  replace that path with a symlink.
* **A path baked into the generated build files** — the NuttX FFI driver, which
  sets `CARGO_TARGET_DIR` itself and needs no symlink. Its shared directory
  appears in the leaf's `build.ninja` and nowhere else.

The symlink-only version reported the live NuttX directory as unreachable, so
`--prune` would have deleted 502 MB that twelve leaves were actively building
against — not corruption, but a full rebuild for everyone and a tool that lies
about what it is deleting.

Reporting is the default. `--prune` deletes.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent


def shared_root() -> Path:
    return ROOT / "build" / "corrosion-cargo"


def live_targets(root: Path) -> set[Path]:
    """Directories still referenced by a leaf, by EITHER mechanism."""
    live: set[Path] = set()
    root_s = str(root)
    for base in ("examples", "packages"):
        start = ROOT / base
        if not start.is_dir():
            continue
        # walk-ok: hunts references inside UNTRACKED build dirs — the git index
        # cannot see build output, which is the whole point of this sweep.
        for dirpath, dirnames, filenames in os.walk(start):
            # 1. the Corrosion consumers: a `cargo` symlink.
            if "cargo" in dirnames:
                p = Path(dirpath) / "cargo"
                if p.is_symlink():
                    live.add(Path(os.path.realpath(p)))
                    dirnames.remove("cargo")
            # 2. the NuttX consumer: the path is baked into build.ninja. Read
            #    only the generated build files, never the whole tree.
            for name in ("build.ninja", "CMakeCache.txt"):
                if name not in filenames:
                    continue
                try:
                    text = (Path(dirpath) / name).read_text(errors="replace")
                except OSError:
                    continue
                if root_s not in text:
                    continue
                for m in re.finditer(re.escape(root_s) + r"/([^/\s\"']+)/([0-9a-f]{12})", text):
                    live.add(Path(root_s) / m.group(1) / m.group(2))
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

    live = live_targets(root)
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

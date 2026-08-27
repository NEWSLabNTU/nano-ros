#!/usr/bin/env python3
"""Reclaim stale cargo PROFILE trees in zephyr west build dirs. Issue 0805.

Zephyr's C/C++ Rust side is built by `nros_cargo_build()`, which pins its own
`CARGO_TARGET_DIR=<build>/nros-rust` (`zephyr/cmake/nros_cargo_build.cmake`).
Inside that directory cargo keeps ONE SUBDIRECTORY PER PROFILE, and nothing ever
removes the ones a later configure stopped using. Measured on this host: 141
build dirs totalling **358 GB**, and in an 11-dir sample **18.8 GB of stale
profile output against 10.9 GB live** — roughly 63% of it is profiles the build
no longer builds with.

## Why this and not a shared target dir

The obvious fix — point every leaf at one shared `--target-dir`, as issue 0805
did for the Corrosion lanes and for NuttX — is WRONG here, and the evidence is
specific:

* The per-image generated headers live INSIDE that directory
  (`<target>/nros-c-generated/nros/nros_config_generated.h`) and differ by
  image: a zenoh leaf carries `NROS_EXECUTOR_STORAGE_SIZE 308976`, a cyclonedds
  leaf `89512`. Sharing hands one image the other's sizes — the mirror class
  this repo has been burned by six times (0088, 0114, 0122, 0123, 0245, 0268).
* 199 `libnros_c*.a` across the workspace are **147 distinct**, so there is
  little to reuse and much to thrash.
* Kconfig reaches deep crates' build scripts through `$DOTCONFIG` and does NOT
  reliably reach cargo's fingerprint (issue 0460), so a key cannot be shown
  complete — and an incomplete key is the failure mode, not the fallback.
* `nros_cargo_build.cmake` already records a MEASURED decision that sharing this
  directory "bought nothing" and "produced only the collision".

Stale profile trees have none of those problems: a profile the current configure
does not name cannot be feeding the current build.

## What counts as stale

The live profile is `NROS_CARGO_PROFILE_DIR` from that build dir's own
`CMakeCache.txt`. Any other profile subdirectory of `nros-rust/` is stale.
Target-triple directories and the `*-generated` header dirs are never touched.

A build dir with no CMakeCache, or no readable profile, is SKIPPED rather than
guessed at.

Reporting is the default. `--prune` deletes.
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
DEFAULT_WS = REPO.parent / "nano-ros-workspace"

def profile_dirs(rust_dir: Path):
    """Every cargo PROFILE directory under `nros-rust/`, at either depth.

    Structural, not a name guess: cargo lays out `<profile>/` for host units and
    `<triple>/<profile>/` for cross ones, and a profile directory is exactly one
    that holds `deps/`. An earlier version guessed by counting dashes — it read
    `nros-fast-release` as a target triple and skipped the single largest stale
    tree on the host (3.8 GB in one build dir), reporting 3.9 GB where the real
    figure was several times that. Name shape is not evidence; layout is.
    """
    for entry in sorted(rust_dir.iterdir()):
        if not entry.is_dir() or entry.name.endswith("-generated"):
            continue
        if (entry / "deps").is_dir():
            yield entry                      # host profile: nros-rust/<profile>
            continue
        for nested in sorted(entry.iterdir()):   # nros-rust/<triple>/<profile>
            if nested.is_dir() and (nested / "deps").is_dir():
                yield nested


def live_profile(build_dir: Path) -> str | None:
    cache = build_dir / "CMakeCache.txt"
    if not cache.is_file():
        return None
    for line in cache.read_text(errors="replace").splitlines():
        if line.startswith("NROS_CARGO_PROFILE_DIR"):
            return line.split("=", 1)[1].strip() or None
    return None


def du_mb(p: Path) -> int:
    out = subprocess.run(["du", "-sm", str(p)], capture_output=True, text=True).stdout
    try:
        return int(out.split()[0])
    except (ValueError, IndexError):
        return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--workspace", default=str(DEFAULT_WS))
    ap.add_argument("--prune", action="store_true")
    args = ap.parse_args()

    ws = Path(args.workspace)
    if not ws.is_dir():
        print(f"gc-zephyr-builds: no workspace at {ws} — nothing to do.")
        return 0

    total = 0
    rows = []
    skipped = 0
    for build_dir in sorted(ws.glob("build-*")):
        rust = build_dir / "nros-rust"
        if not rust.is_dir():
            continue
        cur = live_profile(build_dir)
        if cur is None:
            skipped += 1
            continue
        for entry in profile_dirs(rust):
            if entry.name == cur:
                continue
            mb = du_mb(entry)
            rows.append((entry, cur, mb))
            total += mb

    if skipped:
        print(f"gc-zephyr-builds: skipped {skipped} build dir(s) with no readable "
              f"NROS_CARGO_PROFILE_DIR — not guessing which profile is live.")
    if not rows:
        print("gc-zephyr-builds: OK — no stale profile trees.")
        return 0

    verb = "removing" if args.prune else "stale (re-run with --prune to delete)"
    print(f"gc-zephyr-builds: {len(rows)} {verb}, {total / 1024:.1f} GB")
    for entry, cur, mb in rows:
        print(f"  {mb / 1024:6.2f} GB  {entry.relative_to(ws)}   (live profile: {cur})")
        if args.prune:
            shutil.rmtree(entry, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""A zenoh image's POSIX mutex/cond pools are DERIVED, never hand-set.

zenoh-pico takes one pthread mutex and one condition variable per declared
subscriber and queryable, and Zephyr's pools are static. The counts used to be
hand-set in every zenoh image's conf (`CONFIG_MAX_PTHREAD_MUTEX_COUNT=64`,
`CONFIG_MAX_PTHREAD_COND_COUNT=16`), one constant copied into ~40 files
against a floor the build computes. Every new entity moved the floor and left
the copies behind: when the stock parameter services went on by default, two
images stopped at configure on `CONFIG_MAX_PTHREAD_COND_COUNT=16 is too small`.

Now the module derives both pools' Kconfig DEFAULTS before Kconfig runs, from
the subscriber and queryable counts `nros configure` resolved
(`zephyr/cmake/nros_zpico_pthread_pools.cmake`, `Kconfig.pthread-pools`), so
the count grows with the image. A conf line is an ASSIGNMENT, which beats the
default, so a hand-set count re-creates the stale copy.

WHAT IT REFUSES: `CONFIG_MAX_PTHREAD_COND_COUNT=` / `CONFIG_MAX_PTHREAD_MUTEX_COUNT=`
in a tracked `*.conf` that an image with a `system.toml` merges, which is
  * any conf at or below a directory holding a `system.toml` (a leaf, a
    template, a workspace bringup and its `boards/` files), and
  * a board package's conf (`packages/boards/**`), which reaches only images
    whose `system.toml` names that board.
Cyclone DDS fragments (`*cyclonedds*`) are exempt: Cyclone sizes its own pools
(issues 0371/0496), and the derivation is under `if NROS_RMW_ZENOH`. A guest
application (no `system.toml`) has no resolve to derive from and keeps its
own line.

Absolute, no baseline: the sweep emptied it.

Usage::

    check-zenoh-pthread-pools-derived.py [--selftest]
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSIGN = re.compile(r"^\s*CONFIG_MAX_PTHREAD_(COND|MUTEX)_COUNT\s*=")


def owned_by_image(path: str, has_system_toml) -> bool:
    """Whether an image with a `system.toml` merges the conf at `path`."""
    if "cyclonedds" in os.path.basename(path):
        return False
    if path.startswith("packages/boards/"):
        return True
    d = os.path.dirname(path)
    while d:
        if has_system_toml(d):
            return True
        d = os.path.dirname(d)
    return False


def offences(path: str, text: str, has_system_toml) -> list[str]:
    if not owned_by_image(path, has_system_toml):
        return []
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        if ASSIGN.match(line):
            out.append(f"{path}:{n}: {line.strip()} -- delete it: the module derives this "
                       "pool's default from the image's subscribers and queryables "
                       "(zephyr/cmake/nros_zpico_pthread_pools.cmake), and a conf line "
                       "would pin it below the next entity")
    return out


def tracked(pattern: str) -> list[str]:
    out = subprocess.run(["git", "-C", ROOT, "ls-files", "-z", "--", pattern],
                         check=True, capture_output=True).stdout
    return [p for p in out.decode().split("\0") if p]


def selftest() -> int:
    tomls = {"examples/zephyr/c/talker", "examples/workspaces/c/src/demo_bringup"}
    has = tomls.__contains__
    cases = [
        ("examples/zephyr/c/talker/prj-zenoh.conf", "CONFIG_MAX_PTHREAD_COND_COUNT=16\n", 1),
        ("examples/workspaces/c/src/demo_bringup/boards/native_sim_native_64/prj-zenoh.conf",
         "CONFIG_MAX_PTHREAD_MUTEX_COUNT = 64\n", 1),
        ("packages/boards/nros-board-x-zephyr/prj.conf", "CONFIG_MAX_PTHREAD_MUTEX_COUNT=64\n", 1),
        ("examples/zephyr/c/talker/prj-cyclonedds.conf", "CONFIG_MAX_PTHREAD_MUTEX_COUNT=2048\n", 0),
        ("packages/testing/guest/prj.conf", "CONFIG_MAX_PTHREAD_MUTEX_COUNT=64\n", 0),
        ("examples/zephyr/c/talker/prj.conf", "# CONFIG_MAX_PTHREAD_COND_COUNT=16\n", 0),
        ("examples/zephyr/c/talker/prj.conf", "CONFIG_POSIX_THREAD_THREADS_MAX=16\n", 0),
    ]
    fail = 0
    for path, text, want in cases:
        got = len(offences(path, text, has))
        if got != want:
            print(f"check-zenoh-pthread-pools-derived selftest: {path} {text.strip()!r}: "
                  f"{got} offence(s), want {want}", file=sys.stderr)
            fail += 1
    return fail


def main() -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--selftest", action="store_true", help="run only the negative controls")
    args = ap.parse_args()
    fail = selftest()
    if fail or args.selftest:
        if not fail:
            print("check-zenoh-pthread-pools-derived selftest: OK")
        return 1 if fail else 0
    dirs = {os.path.dirname(p) for p in tracked("*system.toml")
            if os.path.basename(p) == "system.toml"}
    found: list[str] = []
    for p in tracked("*.conf"):
        try:
            with open(os.path.join(ROOT, p), encoding="utf-8", errors="replace") as f:
                found += offences(p, f.read(), dirs.__contains__)
        except FileNotFoundError:
            continue  # deleted in the worktree, not yet staged
    if found:
        print("check-zenoh-pthread-pools-derived: a zenoh image's POSIX pool is hand-set:",
              file=sys.stderr)
        for line in found:
            print(f"  {line}", file=sys.stderr)
        return 1
    print("check-zenoh-pthread-pools-derived: OK -- no system.toml image hand-sets a "
          "zenoh POSIX pool")
    return 0


if __name__ == "__main__":
    sys.exit(main())

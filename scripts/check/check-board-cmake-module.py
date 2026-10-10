#!/usr/bin/env python3
"""check-board-cmake-module — phase-484 W4d (RFC-0103 D1).

A cmake board module (`cmake/board/nano-ros-board-<stem>.cmake`) is the cmake
road's spelling of a board, and the descriptor's `cmake_module = "<stem>"` is
the one place that says which board it belongs to. `nros build` emits the
descriptor's module to cmake rather than the alias an image authored — so the
mapping has to be total and exact, both directions:

  1. every module file is claimed by EXACTLY ONE `[[board]]` (none = a module
     no descriptor builds with; two = an ambiguous board);
  2. every claim names a module file that exists.

Descriptors under `tests/` fixtures are not the catalog and are not read.
Buildless, ~0.1 s; self-tests its parser on each run.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODULE_DIR = ROOT / "cmake" / "board"
PREFIX, SUFFIX = "nano-ros-board-", ".cmake"

CLAIM = re.compile(r'^cmake_module\s*=\s*"([^"]+)"', re.M)


def claims_in(text: str) -> list[str]:
    return CLAIM.findall(text)


def self_test() -> int:
    cases = [
        ('[[board]]\nnames = ["a"]\ncmake_module = "x"\n', ["x"]),
        ('[[board]]\ncmake_module = "x"\n[[board]]\ncmake_module = "y"\n', ["x", "y"]),
        ('# cmake_module = "x"\n', []),
        ('[[board]]\nnames = ["cmake_module"]\n', []),
    ]
    bad = 0
    for text, want in cases:
        got = claims_in(text)
        if got != want:
            bad += 1
            print(f"  self-test FAILED: {text!r} -> {got}, want {want}")
    return bad


def main() -> int:
    if self_test():
        print("check-board-cmake-module --self-test: FAILED")
        return 1
    files = subprocess.run(
        ["git", "ls-files", "*nros-board.toml"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    files = [f for f in files if "/tests/" not in f and "/fixtures/" not in f]
    owners: dict[str, list[str]] = {}
    for f in files:
        for m in claims_in((ROOT / f).read_text()):
            owners.setdefault(m, []).append(f)
    modules = sorted(
        p.name[len(PREFIX):-len(SUFFIX)]
        for p in MODULE_DIR.glob(f"{PREFIX}*{SUFFIX}")
    )
    problems = []
    for m in modules:
        who = owners.get(m, [])
        if not who:
            problems.append(f"cmake/board/{PREFIX}{m}{SUFFIX} is claimed by no board "
                            f"(add `cmake_module = \"{m}\"` to its [[board]])")
        elif len(who) > 1:
            problems.append(f"module `{m}` is claimed by {len(who)} boards: {', '.join(who)}")
    for m, who in sorted(owners.items()):
        if m not in modules:
            problems.append(f"{', '.join(who)} claims cmake_module = \"{m}\", but "
                            f"cmake/board/{PREFIX}{m}{SUFFIX} does not exist")
    if problems:
        print("check-board-cmake-module: the board <-> cmake module map is not exact:")
        for p in problems:
            print(f"  {p}")
        return 1
    print(f"check-board-cmake-module: OK ({len(modules)} modules, each claimed once)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

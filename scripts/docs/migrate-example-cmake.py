#!/usr/bin/env python3
"""Migrate example CMakeLists to the phase-287 bootstrap shape (#171 D5).

Surgical, not a rewrite. Two edits per leaf, everything else preserved:

1. The opening boilerplate — the `if(NOT DEFINED NANO_ROS_ROOT) … endif()`
   root-resolve guard, the `if(NOT COMMAND …) include(NanoRosWorkspace.cmake)
   endif()` + `nano_ros_workspace_pkg_guard()` pair, and the trailing
   `if(NROS_RMW STREQUAL "cyclonedds") enable_language(CXX) endif()`
   micro-option — collapse to the uniform bootstrap prelude +
   `nano_ros_bootstrap()`.

2. The link pair — `target_link_libraries(<t> PRIVATE <pkg>__nano_ros_<lang> …)`
   immediately followed by `nros_platform_link_app(<t>)` — collapse to
   `nano_ros_link(<t>)`. ONLY when every library linked is a generated
   `*__nano_ros_*` msg lib; a leaf that links anything else (custom-platform,
   custom-transport-loopback) keeps its explicit link block untouched.

Cyclone descriptor-TU hooks, `nano_ros_use_board`, extra comments, and any
per-example lines below the prelude are left exactly where they are. Idempotent:
a file already carrying `nano_ros_bootstrap(` is skipped.

Usage:
    scripts/docs/migrate-example-cmake.py --dry-run   # report, no writes
    scripts/docs/migrate-example-cmake.py             # apply
    scripts/docs/migrate-example-cmake.py <path>...   # restrict to these
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

PRELUDE = """# --- nano-ros bootstrap (uniform across every example — do not hand-edit) ---
# Resolve the nano-ros checkout: -DNANO_ROS_ROOT → $NROS_REPO_DIR → in-tree
# walk-up. Everything after (helper include, per-pkg guard, RMW/CXX) is handled
# by nano_ros_bootstrap().
if(NOT DEFINED NANO_ROS_ROOT)
    if(NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        set(_nros_d "${CMAKE_CURRENT_SOURCE_DIR}")
        while(_nros_d AND NOT EXISTS "${_nros_d}/nros-sdk-index.toml")
            get_filename_component(_nros_d "${_nros_d}" DIRECTORY)
        endwhile()
        set(NANO_ROS_ROOT "${_nros_d}")
    endif()
endif()
include("${NANO_ROS_ROOT}/cmake/NanoRosBootstrap.cmake")
nano_ros_bootstrap()
# --- end nano-ros bootstrap ---"""

# The old boilerplate: from the first `if(NOT DEFINED NANO_ROS_ROOT)` up to and
# including the `nano_ros_workspace_pkg_guard(...)` call, then an OPTIONAL
# trailing cyclonedds CXX block. Comments/blank lines between are swept.
_GUARD_START = re.compile(r"^[ \t]*if\s*\(\s*NOT\s+DEFINED\s+NANO_ROS_ROOT", re.M)
_PKG_GUARD = re.compile(r"nano_ros_workspace_pkg_guard\s*\([^)]*\)")
_CXX_BLOCK = re.compile(
    r"\s*if\s*\(\s*NROS_RMW\s+STREQUAL\s+\"cyclonedds\"\s*\)\s*"
    r"enable_language\s*\(\s*CXX\s*\)\s*endif\s*\(\s*\)",
    re.S,
)


def _replace_prelude(text: str) -> tuple[str, bool]:
    m = _GUARD_START.search(text)
    if not m:
        return text, False
    g = _PKG_GUARD.search(text, m.start())
    if not g:
        return text, False
    # Sweep the comment block that documents the guard: contiguous `#` / blank
    # lines immediately above `if(NOT DEFINED NANO_ROS_ROOT)`, stopping at the
    # first real code line above (e.g. `set(CMAKE_C_STANDARD …)`). Otherwise the
    # old guard's prose is orphaned above the new prelude.
    start = m.start()
    lines = text[:start].splitlines(keepends=True)
    while lines:
        stripped = lines[-1].strip()
        if stripped == "" or stripped.startswith("#"):
            start -= len(lines[-1])
            lines.pop()
        else:
            break
    # keep one trailing blank line before the prelude for readability
    m_start = start
    end = g.end()
    # Optional trailing cyclonedds CXX block, allowing only blank/comment lines
    # between it and the pkg_guard call.
    cxx = _CXX_BLOCK.search(text, end)
    if cxx and re.fullmatch(r"(?:[ \t]*\n|[ \t]*#[^\n]*\n)*[ \t]*", text[end : cxx.start()]):
        end = cxx.end()
    return text[:m_start] + PRELUDE + text[end:], True


_LINK_PAIR = re.compile(
    r"""target_link_libraries\s*\(\s*(?P<t>[A-Za-z0-9_]+)\s+PRIVATE\s+
        (?P<libs>[^)]*?)\s*\)      # the linked libs
        \s*
        nros_platform_link_app\s*\(\s*(?P=t)\s*\)""",
    re.S | re.X,
)


def _replace_link(text: str) -> tuple[str, bool, str]:
    m = _LINK_PAIR.search(text)
    if not m:
        return text, False, ""
    libs = m.group("libs").split()
    # only collapse when every linked lib is a generated msg lib
    if not libs or not all("__nano_ros_" in l for l in libs):
        return text, False, "extra-libs"
    repl = f"nano_ros_link({m.group('t')})"
    return text[: m.start()] + repl + text[m.end() :], True, ""


def leaves() -> list[Path]:
    out = subprocess.run(
        ["git", "ls-files", "examples/**/CMakeLists.txt"],
        cwd=REPO, capture_output=True, text=True, check=True,
    ).stdout.split()
    return [REPO / p for p in out if "/build" not in p]


def main() -> int:
    args = sys.argv[1:]
    dry = "--dry-run" in args
    targets = [REPO / a for a in args if not a.startswith("--")]
    files = [f for f in leaves() if not targets or f in targets]

    changed = skipped_done = no_guard = link_kept = 0
    for f in files:
        s = f.read_text()
        if "nano_ros_bootstrap(" in s:
            skipped_done += 1
            continue
        s2, pre_ok = _replace_prelude(s)
        if not pre_ok:
            no_guard += 1
            print(f"  SKIP (no guard block): {f.relative_to(REPO)}")
            continue
        s3, link_ok, why = _replace_link(s2)
        if not link_ok and why == "extra-libs":
            link_kept += 1
            print(f"  prelude-only (extra libs kept): {f.relative_to(REPO)}")
        if not dry:
            f.write_text(s3)
        changed += 1

    print(f"\n{'DRY-RUN ' if dry else ''}{changed} migrated "
          f"({link_kept} prelude-only), {skipped_done} already done, "
          f"{no_guard} had no guard block (left untouched).")
    return 0


if __name__ == "__main__":
    sys.exit(main())

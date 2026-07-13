#!/usr/bin/env python3
"""Phase-287 W6 (workspace slice 3) — migrate workspace ENTRY packages and
workspace ROOTS to the ament shape.

Entry pkgs:
  * the `nano_ros_workspace_pkg_guard()` preamble → `find_package(nano_ros REQUIRED)`
  * `nano_ros_entry(NAME n [SOURCES s...] [BOARD b] [LAUNCH "x"] [LANG l]
     [TYPED] [HOST h] [LOCATOR loc] [ARGS a...] DEPLOY d...)`
    → `nano_ros_add_executable(n [SOURCES s...] [BOARD b] [LAUNCH "x"]
       [TYPED] [HOST h] [LOCATOR loc] [ARGS a...] DEPLOY d...)`
    (NAME positional; LANG dropped — the verb infers from sources, and a
    LAUNCH-only entry falls through to nano_ros_entry's cpp default)

Workspace roots:
  * `include(".../cmake/NanoRosWorkspace.cmake")` →
    `find_package(nano_ros REQUIRED COMPONENTS workspace)`
  * `NANO_ROS_ROOT <path>` kv dropped from the `nano_ros_workspace(...)` call
    (the config sets NANO_ROS_ROOT in the including scope)

Usage: pass CMakeLists.txt paths as argv (entries and roots may be mixed;
the transform keys on which markers each file contains).
"""

import re
import sys
from pathlib import Path

GUARD_RE = re.compile(
    r"if\(NOT COMMAND nano_ros_workspace_pkg_guard\).*?"
    r"nano_ros_workspace_pkg_guard\(\)\n",
    re.S,
)
ENTRY_RE = re.compile(r"^nano_ros_entry\(([^)]*)\)", re.S | re.M)
INCLUDE_WS_RE = re.compile(
    r'include\("[^"]*cmake/NanoRosWorkspace\.cmake"\)'
)
ROOT_KV_RE = re.compile(r"[ \t]*NANO_ROS_ROOT[ \t]+\"?[^\n\"]+\"?\n")


def tokenize(body: str) -> list[str]:
    # Keep quoted strings (LAUNCH values) as single tokens, quotes preserved.
    return re.findall(r'"[^"]*"|\S+', body)


def parse_entry(body: str) -> dict | None:
    toks = tokenize(body)
    out = {
        "name": None,
        "sources": [],
        "board": None,
        "launch": None,
        "typed": False,
        "host": None,
        "locator": None,
        "args": [],
        "deploy": [],
    }
    one = {"NAME": "name", "BOARD": "board", "LAUNCH": "launch",
           "HOST": "host", "LOCATOR": "locator"}
    multi = {"SOURCES": "sources", "DEPLOY": "deploy", "ARGS": "args"}
    cur = None
    for t in toks:
        if t in one:
            cur = ("one", one[t])
        elif t in multi:
            cur = ("multi", multi[t])
        elif t == "LANG":
            cur = ("drop", None)
        elif t == "TYPED":
            out["typed"] = True
            cur = None
        elif cur is None:
            return None  # unexpected positional token
        elif cur[0] == "one":
            out[cur[1]] = t
            cur = None
        elif cur[0] == "multi":
            out[cur[1]].append(t)
        # ("drop", None): swallow the LANG value
        elif cur[0] == "drop":
            cur = None
    if not out["name"] or not out["deploy"]:
        return None
    return out


def render_exe(e: dict) -> str:
    lines = [f"nano_ros_add_executable({e['name']}"]
    if e["sources"]:
        lines.append("    SOURCES " + " ".join(e["sources"]))
    if e["board"]:
        lines.append(f"    BOARD   {e['board']}")
    if e["launch"]:
        lines.append(f"    LAUNCH  {e['launch']}")
    if e["typed"]:
        lines.append("    TYPED")
    if e["host"]:
        lines.append(f"    HOST    {e['host']}")
    if e["locator"]:
        lines.append(f"    LOCATOR {e['locator']}")
    if e["args"]:
        lines.append("    ARGS    " + " ".join(e["args"]))
    lines.append("    DEPLOY  " + " ".join(e["deploy"]) + ")")
    return "\n".join(lines)


def migrate_entry(path: Path, s: str) -> str | None:
    m = ENTRY_RE.search(s)
    if not m:
        return None
    e = parse_entry(m.group(1))
    if e is None:
        print(f"SKIP (unparsed nano_ros_entry): {path}")
        return None
    if not GUARD_RE.search(s):
        print(f"SKIP (no guard preamble): {path}")
        return None
    s = GUARD_RE.sub("find_package(nano_ros REQUIRED)\n", s, count=1)
    s = ENTRY_RE.sub(lambda _: render_exe(e), s, count=1)
    return s


def migrate_root(path: Path, s: str) -> str | None:
    if not INCLUDE_WS_RE.search(s):
        return None
    s = INCLUDE_WS_RE.sub(
        "find_package(nano_ros REQUIRED COMPONENTS workspace)", s, count=1
    )
    # Drop the NANO_ROS_ROOT kv INSIDE the nano_ros_workspace(...) call only.
    def strip_kv(m: re.Match) -> str:
        return "nano_ros_workspace(" + ROOT_KV_RE.sub("", m.group(1)) + ")"
    s = re.sub(
        r"nano_ros_workspace\(([^)]*)\)",
        strip_kv,
        s,
        count=1,
        flags=re.S,
    )
    return s


def main() -> None:
    n = 0
    for arg in sys.argv[1:]:
        p = Path(arg)
        s = p.read_text()
        if "nano_ros_entry(" in s and "nano_ros_workspace(" not in s:
            out = migrate_entry(p, s)
        elif "nano_ros_workspace(" in s:
            out = migrate_root(p, s)
        else:
            out = None
        if out is not None:
            p.write_text(out)
            n += 1
            print(f"migrated {p}")
    print(f"{n} files migrated")


if __name__ == "__main__":
    main()

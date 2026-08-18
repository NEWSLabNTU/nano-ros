#!/usr/bin/env python3
"""Issue 0654 — no tree site may invoke the router with command-line flags.

phase-362 retired the vendored `zenohd`. The router is ROS 2's
`rmw_zenoh_cpp/rmw_zenohd`, and it takes NO command-line configuration: it does
not parse argv. So `zenohd --listen tcp/127.0.0.1:7447` is wrong in two
independent ways, and the second is the nasty one:

  * the NAME is gone — nothing installs a `zenohd`;
  * the FLAGS are not rejected, they are UNREAD. A reader who has an
    `rmw_zenohd` and follows such a line gets a router on the DEFAULT
    configuration — not the port they asked for, scouting not disabled — with no
    diagnostic. A wrong port then surfaces as a silent hang in `Executor::open`,
    which the troubleshooting pages blame on other causes.

Configuration travels in the environment:

    ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \\
        /opt/ros/$ROS_DISTRO/lib/rmw_zenoh_cpp/rmw_zenohd

Shell callers should use `nros_router_exec` (scripts/dev/zenohd.sh), which is the
ONE spelling of both halves. Prose sites cannot call a shell function, so they
carry the literal form above.

WHY A GATE: this class regenerates every time someone copies a neighbouring
example's header comment, which is exactly how it reached ~95 files. The gate is
the edge, not the cleanup.

`docs/**/archived/**` is exempt: a historical record describing what was true at
the time is correct, and rewriting it would be a lie about the past.
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# `zenohd --flag` / `zenohd -l …`. Not a bare `zenohd` mention: prose legitimately
# names the router, and issue 0653 (a ROS-less host has none) is a separate axis.
PAT = re.compile(r"\bzenohd\s+(?:--|-l\s)")

# issue 0654 (second half) — the HARDCODED router path.
#
# The first pass replaced 92 `zenohd --listen …` lines with a canonical string
# that spelled `/opt/ros/$ROS_DISTRO/lib/rmw_zenoh_cpp/rmw_zenohd`. That trades a
# command nobody can run for one that runs only on a host whose ROS lives under
# `/opt/ros` — and issue 0653 had already established that prefix is the THIRD of
# three resolution steps, after `NROS_RMW_ZENOHD` and `AMENT_PREFIX_PATH`. So the
# canonical form was wrong on exactly the hosts (source builds, colcon overlays)
# that step 2 exists for, and it was wrong in 92 places at once.
#
# Executables ask `nros_zenohd_bin`; anything that TELLS a human asks
# `nros_router_hint`, which prints `ros2 run rmw_zenoh_cpp rmw_zenohd` — ROS's
# own path-independent spelling. This pattern keeps a fourth copy from appearing.
HARDCODED_PATH = re.compile(r"/opt/ros/[^\s\"']*/lib/rmw_zenoh_cpp/rmw_zenohd")

EXEMPT_PREFIXES = (
    "docs/",          # narrowed below to archived/ only
    "third-party/",
    "tmp/",
)


def is_exempt(path: str) -> bool:
    if path.startswith("third-party/") or path.startswith("tmp/"):
        return True
    # historical records stay as written
    if "/archived/" in path:
        return True
    # this gate's own prose, and the issue that specifies it
    if path == "scripts/check-zenohd-flag-invocations.py":
        return True
    if path.startswith("docs/issues/") and "0654" in path:
        return True
    return False


def is_path_exempt(path: str) -> bool:
    """Files allowed to spell the router's install path.

    The two RESOLVERS, which exist to turn the environment into that path, and
    the machinery that tests them. Everything else asks one of them.
    """
    if is_exempt(path):
        return True
    return path in {
        "scripts/dev/zenohd.sh",                              # the shell resolver
        "packages/testing/nros-tests/src/process.rs",          # the Rust resolver
        "scripts/check-zenohd-resolution-parity.sh",           # drives both
        "scripts/dev/zenohd-resolution-cases.tsv",             # their shared table
        "docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md",
    } or path.startswith("docs/issues/") and "0653" in path


def main() -> int:
    listing = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files"],
        capture_output=True, text=True, check=True,
    ).stdout.split()

    hits: list[tuple[str, int, str]] = []
    path_hits: list[tuple[str, int, str]] = []
    for rel in listing:
        p = ROOT / rel
        try:
            text = p.read_text(encoding="utf-8")
        except (UnicodeDecodeError, FileNotFoundError, IsADirectoryError):
            continue
        lines = text.splitlines()
        if not is_exempt(rel):
            for n, line in enumerate(lines, 1):
                if PAT.search(line):
                    hits.append((rel, n, line.strip()[:110]))
        if not is_path_exempt(rel):
            for n, line in enumerate(lines, 1):
                if HARDCODED_PATH.search(line):
                    path_hits.append((rel, n, line.strip()[:110]))

    if path_hits:
        print("[FAIL] hardcoded router path (issue 0654):")
        for rel, n, line in path_hits:
            print(f"         {rel}:{n}")
            print(f"           {line}")
        print()
        print("       `/opt/ros/<distro>/lib/rmw_zenoh_cpp/rmw_zenohd` is the")
        print("       THIRD of the resolver's three steps (issue 0653), so this")
        print("       line is wrong on a host whose ROS was built from source or")
        print("       installed as a colcon overlay — the hosts step 2")
        print("       (AMENT_PREFIX_PATH) exists for.")
        print()
        print("       To START it:  `nros_router_exec <locator>`  (or `just zenohd`)")
        print("       To RESOLVE it: `nros_zenohd_bin`")
        print("       To TELL a human: `nros_router_hint <locator>`, which prints")
        print("       `ros2 run rmw_zenoh_cpp rmw_zenohd` — path-independent.")
        print()

    if hits:
        print("[FAIL] router invoked with command-line flags (issue 0654):")
        for rel, n, line in hits:
            print(f"         {rel}:{n}")
            print(f"           {line}")
        print()
        print("       `rmw_zenohd` does not parse argv. The flags are not")
        print("       rejected, they are UNREAD — the router silently comes up")
        print("       on its default configuration, and a wrong port reads as a")
        print("       hang in `Executor::open`.")
        print()
        print("       Shell: use `nros_router_exec <locator>` (scripts/dev/zenohd.sh).")
        print("       Prose: `nros_router_hint` prints the line; it is")
        print("              ZENOH_CONFIG_OVERRIDE='listen/endpoints=[\"<loc>\"];"
              "scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd")
        return 1

    if path_hits:
        return 1

    print(f"check-zenohd-flag-invocations: OK ({len(listing)} tracked file(s); "
          "no flag invocation and no hardcoded router path outside archived history)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

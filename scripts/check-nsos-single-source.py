#!/usr/bin/env python3
"""native_sim's network (NSOS) is stated ONCE — issue 2157.

NSOS (Native Sim Offloaded Sockets: host BSD sockets instead of the root-only
`zeth` TAP device) is a fact about the BOARD. It lives in one place, the
nano-ros module's `nros-nsos` snippet (`zephyr/snippets/nros-nsos/`), which a
board package names (`[board.zephyr] snippets = ["nros-nsos"]`) and the module
hook adds to `SNIPPET` on every road.

Before issue 2157 it was 56 copies: three shared overlays (two of them per
Zephyr line), 32 leaf `boards/native_sim_native_64.conf`, and the NSOS tail of
21 `prj-cyclonedds.conf` RMW fragments. Every leaf copy named 3.7's TAP symbol,
`ETH_NATIVE_POSIX`, which 4.4 renamed `ETH_NATIVE_TAP` — so no copy outside the
shared overlays could build on 4.4 — and a leaf file is merged only when
`CONF_FILE` is unset, so the copies were live on some roads and dead on others.

WHAT IT REFUSES, in a tracked `*.conf` outside the snippet:

  * ENABLING NSOS — `CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y` or
    `CONFIG_NET_SOCKETS_OFFLOAD=y`. Name the snippet instead (`-S nros-nsos`,
    or the board's `snippets`). Turning them OFF (a real board's
    `CONFIG_NET_SOCKETS_OFFLOAD=n`) is that board's own fact and passes.
  * NAMING THE TAP DRIVER — `CONFIG_ETH_NATIVE_POSIX` / `CONFIG_ETH_NATIVE_TAP`
    with any value. Each symbol exists on one Zephyr line only, and Zephyr
    refuses an assignment to an undefined symbol, so the line pins the file to
    a line. The snippet turns the driver class off (`CONFIG_ETH_DRIVER=n`).

AND, in the snippet itself (issue 2182): no snippet-wide `append:`. The
generic `zephyr` board names the snippet and is built for non-native boards
too (`-b mps2/an385`), where NSOS cannot be enabled and an unscoped snippet
makes Kconfig refuse the build. The conf must sit under a `boards:` regex that
only native_sim matches.

Absolute, no baseline: the migration emptied it.

Usage::

    check-nsos-single-source.py [--selftest]
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SNIPPET_DIR = "zephyr/snippets/nros-nsos/"

ENABLE = re.compile(r"^\s*CONFIG_(NET_NATIVE_OFFLOADED_SOCKETS|NET_SOCKETS_OFFLOAD)\s*=\s*y\s*$")
TAP = re.compile(r"^\s*CONFIG_(ETH_NATIVE_POSIX|ETH_NATIVE_TAP)\s*=")


def offences(path: str, text: str) -> list[str]:
    """Every refused line of one conf file, as `path:line: text — reason`."""
    if path.startswith(SNIPPET_DIR):
        return []
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        if ENABLE.match(line):
            out.append(f"{path}:{n}: {line.strip()} — enables NSOS; name the `nros-nsos` "
                       "snippet instead (the board's `[board.zephyr] snippets`, or `-S nros-nsos`)")
        elif TAP.match(line):
            out.append(f"{path}:{n}: {line.strip()} — names a TAP-driver symbol one Zephyr "
                       "line defines and the other refuses; `nros-nsos` turns the driver "
                       "class off (`CONFIG_ETH_DRIVER=n`) on every line")
    return out


def snippet_scope_offences(text: str) -> list[str]:
    """The snippet must append only under `boards:` -- issue 2182."""
    out = []
    if re.search(r"^append\s*:", text, re.M):
        out.append(f"{SNIPPET_DIR}snippet.yml: a snippet-wide `append:` applies NSOS to every "
                   "board, and the generic `zephyr` board is built for mps2/an385 too, where "
                   "Kconfig refuses it (issue 2182); scope it under `boards: /native_sim(/.*)?/:`")
    if not re.search(r"^boards\s*:\s*\n\s+/native_sim", text, re.M):
        out.append(f"{SNIPPET_DIR}snippet.yml: no `boards:` entry for native_sim -- the snippet "
                   "would apply NSOS nowhere")
    return out


def tracked_confs() -> list[str]:
    out = subprocess.run(["git", "-C", ROOT, "ls-files", "-z", "--", "*.conf"],
                         check=True, capture_output=True).stdout
    return [p for p in out.decode().split("\0") if p]


def selftest() -> int:
    cases = [
        ("examples/zephyr/c/talker/boards/native_sim_native_64.conf",
         "CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y\n", 1),
        ("examples/zephyr/c/talker/prj-cyclonedds.conf", "CONFIG_NET_SOCKETS_OFFLOAD=y\n", 1),
        ("cmake/zephyr/whatever.conf", "CONFIG_ETH_NATIVE_POSIX=n\n", 1),
        ("cmake/zephyr/whatever.conf", "CONFIG_ETH_NATIVE_TAP=n\n", 1),
        ("packages/boards/nros-board-mps2-an385-zephyr/prj.conf",
         "CONFIG_NET_SOCKETS_OFFLOAD=n\n", 0),
        ("examples/x/prj.conf", "# CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y\n", 0),
        (SNIPPET_DIR + "nsos.conf", "CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y\n", 0),
    ]
    fail = 0
    for path, text, want in cases:
        got = len(offences(path, text))
        if got != want:
            print(f"check-nsos-single-source selftest: {path} {text.strip()!r}: "
                  f"{got} offence(s), want {want}", file=sys.stderr)
            fail += 1
    scope_cases = [
        ("name: nros-nsos\nappend:\n  EXTRA_CONF_FILE: nsos.conf\n", 2),
        ("name: nros-nsos\nboards:\n  /native_sim(/.*)?/:\n    append:\n"
         "      EXTRA_CONF_FILE: nsos.conf\n", 0),
    ]
    for text, want in scope_cases:
        got = len(snippet_scope_offences(text))
        if got != want:
            print(f"check-nsos-single-source selftest: snippet scope {text!r}: "
                  f"{got} offence(s), want {want}", file=sys.stderr)
            fail += 1
    if not os.path.isfile(os.path.join(ROOT, SNIPPET_DIR, "nsos.conf")):
        print(f"check-nsos-single-source selftest: {SNIPPET_DIR}nsos.conf is missing — "
              "the one source this gate protects", file=sys.stderr)
        fail += 1
    return fail


def main() -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or "").splitlines()[0])
    ap.add_argument("--selftest", action="store_true", help="run only the negative controls")
    args = ap.parse_args()
    fail = selftest()
    if fail or args.selftest:
        if not fail:
            print("check-nsos-single-source selftest: OK")
        return 1 if fail else 0
    with open(os.path.join(ROOT, SNIPPET_DIR, "snippet.yml"), encoding="utf-8") as f:
        found = snippet_scope_offences(f.read())
    for p in tracked_confs():
        try:
            with open(os.path.join(ROOT, p), encoding="utf-8", errors="replace") as f:
                found += offences(p, f.read())
        except FileNotFoundError:
            continue  # deleted in the worktree, not yet staged
    if found:
        print("check-nsos-single-source: native_sim's network is stated outside "
              f"{SNIPPET_DIR} (issue 2157):", file=sys.stderr)
        for line in found:
            print(f"  {line}", file=sys.stderr)
        return 1
    print(f"check-nsos-single-source: OK — NSOS is enabled only by {SNIPPET_DIR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

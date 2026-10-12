#!/usr/bin/env python3
"""check-store-source-readers — a store-first tree is LOCATED, never spelled.

RFC-0103 D4/D5 / phase-484 W3c. A `[source.*]` row with `location = "store"`
and a `submodule` is store-first: every reader asks the one ladder (`nros
locate`, `nros_build_paths::locate`, `nros_locate_var`, `nros_locate_source`),
which answers env override > local edit > store > checkout. A reader that
writes the row's checkout path (`third-party/threadx/kernel`) builds the
checkout's copy even when the store holds the pin — or finds nothing in an
agent worktree whose submodule was never initialised, which is the state W3
exists to make buildable.

So: outside the files listed in ALLOWED, each with its reason, no tracked
code line names a store-first row's `dest`. Comment lines do not count (prose
about where a tree used to be is not a read). A listed file that no longer
names any such path is reported too, so the list cannot rot into a blanket
exemption.

What it CANNOT see: a path assembled from pieces (`xrce_sys.join("micro-cdr")`,
`"${CMAKE_CURRENT_SOURCE_DIR}/../xrce-sys/micro-cdr"`), because it matches the
row's whole `dest`. Those were swept by hand when each row flipped; a fallback
of that shape is legitimate when it follows a locate call (the ladder's last
rung), which is why this is not widened to a per-segment match.

A second rule (issue 1811): the four trees phase-484 W3c stopped exporting
(`FREERTOS_DIR`, `LWIP_DIR`, `THREADX_DIR`, `NETX_DIR`) are still the names
their tools take. A shell or `just` reader of one must SET it first, through
`nros_use_sources` or `nros_locate_source`: in a `just` file, earlier in the
same recipe; in a script, earlier in the file. Reading it straight from the
caller's environment is what left four nightly ports and the live-peer board
jobs with an unbound variable or an empty in-tree path after the move.

Buildless, tracked files only, ~0.2 s. Self-tests its comment filter and the
recipe rule.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

try:
    import tomllib
except ImportError:  # Python < 3.11
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import git_grep  # noqa: E402  issue 1816 — a failed search is not "no match"

# file -> why it may name a store-first row's checkout path.
ALLOWED = {
    # The checkout rung itself, for configures that run with NO `nros` CLI
    # (`nros_locate_var` found nothing to ask): the ladder's last rung, spelled
    # where cmake has no ladder to call.
    "cmake/board/nano-ros-board-mps3-an536-freertos.cmake": "no-CLI checkout fallback",
    "cmake/board/nano-ros-board-s32z270-freertos.cmake": "no-CLI checkout fallback",
    "cmake/board/nano-ros-board-rv-virt-threadx.cmake": "no-CLI checkout fallback",
    "cmake/board/nano-ros-board-threadx-linux.cmake": "no-CLI checkout fallback",
    "cmake/platform/nano-ros-freertos.cmake": "no-CLI checkout fallback",
    "cmake/platform/nano-ros-threadx.cmake": "no-CLI checkout fallback",
    "zephyr/cmake/nros_rmw_zenoh.cmake": "no-CLI checkout fallback",
    "tests/freertos-c-smoke/CMakeLists.txt": "default when the recipe passes no -DFREERTOS_DIR",
    "tests/threadx-c-smoke/CMakeLists.txt": "default when the recipe passes no -DTHREADX_DIR",
    # The ladder's own checkout rung and its tests.
    "packages/tooling/nros-build-paths/src/lib.rs": "located_or_checkout + re-root tests",
    "packages/cli/nros-cli-core/src/orchestration/sdk_index.rs": "index parser test fixtures",
    # Messages that TELL a user where the submodule is; nothing reads them.
    "packages/boards/nros-board-freertos/build.rs": "error text naming the submodule",
    "packages/boards/nros-board-threadx/build.rs": "error text naming the submodule",
    "packages/testing/nros-tests/tests/freertos_posix.rs": "skip text naming the submodule",
    "packages/drivers/net/lan9118-lwip/CMakeLists.txt": "error text naming the submodule",
    "packages/drivers/net/virtio-net-netx/CMakeLists.txt": "error text naming the submodule",
    "packages/rmw/zenoh/nros-zpico-build/src/runner.rs": "error text naming the submodule",
    "just/check/docs.just": "skip text naming the submodule",
    "just/check/rmw.just": "skip text naming the submodule",
    "just/check/lanes.just": "skip text naming the submodule",
    "packages/rmw/xrce/nros-rmw-xrce-cffi/build.rs": "error text naming the submodule",
    "packages/cli/nros-cli-core/src/orchestration/metadata_build.rs": "test fixture: a build-script panic text",
    "scripts/check-c-array-guard-probe.py": "skip text naming the submodule",
    "scripts/check-zenoh-feature-off-compile.py": "skip text naming the submodule",
    "packages/testing/nros-tests/src/process.rs": "diagnostic version label, `unknown` when absent",
    "scripts/check-zenoh-lane-ownership.py": "classifies RELATIVE paths, reads nothing",
    # Provisioning lists: they INITIALISE the checkout copy (the ladder's
    # last rung), they do not read a tree.
    ".github/workflows/live-peer.yml": "submodule init list",
    ".github/workflows/gate.yml": "submodule init list (the check job's source gates)",
    ".config/worktree-provisioning.txt": "submodule init list",
    # Developer tools that read the checkout on purpose (they audit the
    # vendored sources a contributor edits, not what a build consumes).
    "scripts/dev/sweep-vendored-c-decls.sh": "audits the checkout's vendored C",
    "scripts/cyclonedds/ddsrt-port-inventory.sh": "audits the checkout's vendored C",
    # Developer tools that BUILD zenoh-pico in place (they write `build/` INTO
    # the tree, which the read-only store copy must never take).
    "justfile": "dev: builds zenoh-pico in the checkout",
    "scripts/debug/capture-ros2-keyexpr.sh": "dev: runs the in-checkout zenoh-pico build",
    "scripts/debug/compare-keyexprs.sh": "dev: runs the in-checkout zenoh-pico build",
    "scripts/debug/debug-keyexpr.sh": "dev: runs the in-checkout zenoh-pico build",
    "scripts/debug/debug-liveliness.sh": "dev: runs the in-checkout zenoh-pico build",
    "docker/can-demo/run.sh": "dev: mounts the checkout into a container",
}

COMMENT = re.compile(r"^\s*(#|//|/\*|\*|--|;)")


def store_dests() -> dict[str, str]:
    index = tomllib.loads((ROOT / "nros-sdk-index.toml").read_text())
    return {
        row["dest"]: name
        for name, row in index.get("source", {}).items()
        if row.get("location") == "store" and row.get("submodule") and row.get("dest")
    }


def hits(dest: str) -> list[tuple[str, int, str]]:
    out = git_grep.run(
        [
            "-n", "-F", dest, "--",
            ":!docs", ":!*.md", ":!nros-sdk-index.toml", ":!.gitmodules",
            ":!scripts/check/check-store-source-readers.py",
        ],
        ROOT,
    )
    found = []
    for line in out.splitlines():
        path, lineno, text = line.split(":", 2)
        if COMMENT.match(text):
            continue
        found.append((path, int(lineno), text.strip()))
    return found


def self_test() -> None:
    for c in ["# x", "  // x", " * x", "/* x", "//! x"]:
        assert COMMENT.match(c), c
    for c in ['set(X "${R}/third-party/x")', 'let p = root.join("third-party/x");']:
        assert not COMMENT.match(c), c


TREE_VARS = re.compile(r"\$\{?(FREERTOS_DIR|LWIP_DIR|THREADX_DIR|NETX_DIR)\b")
LOCATES = re.compile(r"\bnros_use_sources\b|\bnros_locate_source\b")
RECIPE_HEAD = re.compile(r"^[A-Za-z_@][A-Za-z0-9_-]*( [^:=]*)?:(?!=)")


def unlocated_reads(path: str, text: str) -> list[tuple[int, str]]:
    """Lines reading a tree variable with no locate call ahead of them in the
    same recipe (`just`) or file (script)."""
    is_just = path.endswith(".just") or path == "justfile"
    out: list[tuple[int, str]] = []
    located = False
    for n, line in enumerate(text.split("\n"), 1):
        if is_just and RECIPE_HEAD.match(line):
            located = False
        if COMMENT.match(line):
            continue
        if LOCATES.search(line):
            located = True
            continue
        if TREE_VARS.search(line) and not located:
            out.append((n, line.strip()))
    return out


def tree_var_readers() -> list[str]:
    out = git_grep.run(
        ["-l", "-E", r"\$\{?(FREERTOS_DIR|LWIP_DIR|THREADX_DIR|NETX_DIR)\b", "--",
         "just/*.just", "justfile", "scripts/*.sh", "scripts/**/*.sh"],
        ROOT,
    )
    bad = []
    for path in out.splitlines():
        text = (ROOT / path).read_text()
        for n, line in unlocated_reads(path, text):
            bad.append(f"  {path}:{n}: reads a store-first tree's variable unlocated: {line[:110]}")
    return bad


def self_test_recipes() -> None:
    ok = "a:\n    source x\n    nros_use_sources lwip=LWIP_DIR || true\n    echo $LWIP_DIR\n"
    assert unlocated_reads("x.just", ok) == [], ok
    # A second recipe does not inherit the first one's locate.
    two = ok + "b:\n    echo $LWIP_DIR\n"
    assert [n for n, _ in unlocated_reads("x.just", two)] == [6], two
    assert unlocated_reads("x.sh", 'echo "${THREADX_DIR}/x"\n'), "script read"
    assert not unlocated_reads("x.sh", "# $THREADX_DIR in prose\n"), "comment"


def main() -> int:
    self_test()
    self_test_recipes()
    dests = store_dests()
    bad: list[str] = []
    used: set[str] = set()
    for dest, name in sorted(dests.items()):
        for path, lineno, text in hits(dest):
            if path in ALLOWED:
                used.add(path)
                continue
            bad.append(f"  {path}:{lineno}: [source.{name}] spelled as {dest}: {text[:120]}")
    stale = sorted(set(ALLOWED) - used)
    var_bad = tree_var_readers()
    if var_bad:
        print(
            "check-store-source-readers: FAIL — a recipe or script reads a tree "
            "variable phase-484 W3c no longer exports (issue 1811):",
            file=sys.stderr,
        )
        print("\n".join(var_bad), file=sys.stderr)
        print(
            "  Set it first: `source scripts/build/cargo.sh; nros_use_sources "
            "<source>=<VAR> ...`.",
            file=sys.stderr,
        )
        if not (bad or stale):
            return 1
    if bad or stale:
        if bad:
            print(
                "check-store-source-readers: FAIL — a store-first tree is read by its "
                "checkout path (RFC-0103 D5):",
                file=sys.stderr,
            )
            print("\n".join(bad), file=sys.stderr)
            print(
                "  Locate it instead: `nros_build_paths::locate::source(\"<name>\")` (Rust),\n"
                "  `nros_locate_var(<VAR>)` (cmake), `nros_locate_source <name>` (shell,\n"
                "  scripts/build/cargo.sh), `nros locate <name>` (anything else).",
                file=sys.stderr,
            )
        for p in stale:
            print(
                f"check-store-source-readers: {p} is ALLOWED but names no store-first "
                "path any more — remove it from ALLOWED",
                file=sys.stderr,
            )
        return 1
    print(
        f"check-store-source-readers: OK — {len(dests)} store-first source(s), "
        f"{len(used)} listed reader file(s), no other spelling"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

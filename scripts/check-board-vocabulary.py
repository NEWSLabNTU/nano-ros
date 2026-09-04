#!/usr/bin/env python3
"""Every `deploy=` and `board=` a package.xml exports must resolve somewhere.

phase-422 W7. `<export><nano_ros deploy=".." board=".."/></export>` is how a
workspace states where it deploys, and `nros setup --workspace` turns that into
a provisioning command. That only works if the values mean something.

WHAT THE VALUES ACTUALLY MEAN, measured rather than assumed. `board=` is the
CMAKE BOARD vocabulary: all five values across `examples/` resolve to a
`cmake/board/nano-ros-board-<name>.cmake` file. They are NOT `[board.*]` index
keys — only `threadx-linux` is one, so `nros setup <board>` would fail for four
of five. That mismatch is real and is why `nros setup --workspace` validates
before printing a command.

Five namespaces exist for closely related concepts, overlapping partially:

  1. `cmake/board/nano-ros-board-*.cmake` — what `board=` names (canonical here)
  2. `[board.*]` in nros-sdk-index.toml   — what `nros setup <board>` accepts
  3. `packages/boards/nros-board-*`       — the board crate
  4. `examples/fixtures.toml` NANO_ROS_BOARD — the test coordinate
  5. `_NROS_SCOPE_PLATFORMS`              — what `just setup <scope>` accepts

This gate does NOT unify them. Which becomes canonical across all five is a
decision phase-422 W7 leaves to a human, and renaming boards through 90+
package.xml files is not something to do as a side effect. What it stops is a
value resolving in NONE of them — the case where a printed remedy fails and a
user pays for it.

Three reader bugs were found by RUNNING this gate rather than reasoning about
it, each one reporting well-defined boards as undefined: fixtures.toml spells
the board `NANO_ROS_BOARD = ".."` inside `cmake_defs`, not `board = ".."`; and
the cmake board files were missing from the namespace list entirely. A gate
that is wrong about where names live sends someone renaming working examples.

Run:  python3 scripts/check-board-vocabulary.py [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def exports(text):
    """[(deploy, board)] from `<nano_ros deploy=".." board=".."/>` tags."""
    out = []
    for m in re.finditer(r"<nano_ros\s([^>]*)>", text):
        tag = m.group(1)
        d = re.search(r'deploy="([^"]*)"', tag)
        if not d:
            continue
        b = re.search(r'board="([^"]*)"', tag)
        out.append((d.group(1), b.group(1) if b else None))
    return out


def index_boards(root):
    try:
        with open(os.path.join(root, "nros-sdk-index.toml"), encoding="utf8") as fh:
            return set(re.findall(r"^\[board\.([a-z0-9._-]+)\]", fh.read(), re.M))
    except OSError:
        return set()


def board_crates(root):
    d = os.path.join(root, "packages", "boards")
    try:
        return {n[len("nros-board-") :] for n in os.listdir(d) if n.startswith("nros-board-")}
    except OSError:
        return set()


def fixture_boards(root):
    """Boards named by the fixture matrix.

    NOT a bare `board = "..."`: fixtures.toml carries the board inside a cmake
    definition table, `cmake_defs = { NANO_ROS_BOARD = "nuttx-qemu-arm", .. }`.
    Reading the wrong spelling made this gate report three real, well-defined
    boards as resolving nowhere — a false positive that would have sent someone
    renaming working examples.
    """
    boards = set()
    for rel in [("examples", "fixtures.toml")]:
        try:
            with open(os.path.join(root, *rel), encoding="utf8") as fh:
                t = fh.read()
        except OSError:
            continue
        boards |= set(re.findall(r'NANO_ROS_BOARD\s*=\s*"([^"]+)"', t))
        boards |= set(re.findall(r'^\s*board\s*=\s*"([^"]+)"', t, re.M))
    return boards


def cmake_boards(root):
    """Boards defined by `cmake/board/nano-ros-board-<name>.cmake`.

    This is what `board=` in a package.xml export actually names — all five
    values in this repo resolve here, and only one of them is an index key.
    """
    d = os.path.join(root, "cmake", "board")
    try:
        names = os.listdir(d)
    except OSError:
        return set()
    out = set()
    for n in names:
        m = re.fullmatch(r"nano-ros-board-(.+)\.cmake", n)
        if m:
            out.add(m.group(1))
    return out


def scopes(root):
    try:
        with open(os.path.join(root, "scripts", "build", "scope.sh"), encoding="utf8") as fh:
            t = fh.read()
    except OSError:
        return set()
    m = re.search(r'_NROS_SCOPE_PLATFORMS="([^"]*)"', t)
    return set(m.group(1).split()) if m else set()


def self_test():
    got = exports('<nano_ros deploy="threadx" board="riscv64-qemu" rmw="zenoh"/>')
    assert got == [("threadx", "riscv64-qemu")], got
    assert exports('<nano_ros deploy="native"/>') == [("native", None)]
    # Siblings are not deploy exports.
    assert exports('<nano_ros_provides kind="board" name="threadx"/>') == []
    sys.stdout.write("check-board-vocabulary self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    self_test()

    files = subprocess.run(
        ["git", "ls-files", "*package.xml"], cwd=ROOT, capture_output=True, text=True
    ).stdout.split()
    idx, crates, fixtures, scope_set, cmakeb = (
        index_boards(ROOT),
        board_crates(ROOT),
        fixture_boards(ROOT),
        scopes(ROOT),
        cmake_boards(ROOT),
    )
    if not scope_set or not idx:
        sys.stderr.write(
            "error: could not read the scope list or the index boards.\n"
            "This gate would then accept anything and pass vacuously.\n"
        )
        return 1

    bad_board, bad_deploy, not_index = {}, {}, {}
    seen_board, seen_deploy = {}, {}
    for f in files:
        try:
            with open(os.path.join(ROOT, f), encoding="utf8") as fh:
                text = fh.read()
        except OSError:
            continue
        for deploy, board in exports(text):
            seen_deploy.setdefault(deploy, f)
            # A deploy is a scope, or splits into `<deploy>_*` scopes by board.
            if deploy not in scope_set and not any(
                s.startswith(deploy + "_") for s in scope_set
            ):
                bad_deploy.setdefault(deploy, f)
            if board is None:
                continue
            seen_board.setdefault(board, f)
            if (
                board not in cmakeb
                and board not in idx
                and board not in crates
                and board not in fixtures
            ):
                bad_board.setdefault(board, f)
            # SECOND, STRICTER assertion (phase-422 W7). The check above is an OR
            # over five namespaces, and `cmake/board/*.cmake` alone satisfies
            # every value — so it passed both before and after the index entries
            # were added, and never tested the property W7 exists to create.
            # Measured A/B, not assumed: identical OK line either way.
            #
            # `[board.*]` is the namespace `nros setup <board>` looks up, with an
            # exact-key lookup and no fallback. A board that resolves only in the
            # cmake namespace is one an out-of-tree user cannot provision — which
            # was true of four of five.
            elif board not in idx:
                not_index.setdefault(board, f)

    if bad_board or bad_deploy or not_index:
        sys.stderr.write(
            "check-board-vocabulary: %d problem(s)\n\n"
            % (len(bad_board) + len(bad_deploy) + len(not_index))
        )
        for b, f in sorted(not_index.items()):
            sys.stderr.write(
                "  - board=%r (%s) resolves, but NOT as an index `[board.*]` key.\n"
                "      That is the namespace `nros setup <board>` looks up (exact key,\n"
                "      no fallback), so an out-of-tree user cannot provision it — they\n"
                "      have no justfile to fall back on.\n"
                "      Add `[board.%s]` to nros-sdk-index.toml mirroring the equivalent\n"
                "      entry. Adding it is ADDITIVE; renaming is not, and is not asked\n"
                "      for here.\n\n" % (b, f, b)
            )
        for d, f in sorted(bad_deploy.items()):
            sys.stderr.write(
                "  - deploy=%r (%s) is not a scope and no scope starts with %r.\n"
                "      `nros setup --workspace` prints a provisioning command from this;\n"
                "      an unresolvable value makes that command fail.\n"
                "      Scopes: %s\n\n" % (d, f, d + "_", " ".join(sorted(scope_set)))
            )
        for b, f in sorted(bad_board.items()):
            sys.stderr.write(
                "  - board=%r (%s) resolves in NONE of the five namespaces:\n"
                "      cmake/board/nano-ros-board-*.cmake | [board.*] index key |\n"
                "      packages/boards/nros-board-* | fixtures.toml NANO_ROS_BOARD\n"
                "      Name one that exists, or add the board where it belongs.\n\n" % (b, f)
            )
        return 1

    sys.stdout.write(
        "check-board-vocabulary: OK — %d deploy value(s), %d board value(s); "
        "each resolves AND is an index key (cmake %d / index %d / crate %d / fixture %d).\n"
        % (
            len(seen_deploy),
            len(seen_board),
            len(cmakeb),
            len(idx),
            len(crates),
            len(fixtures),
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

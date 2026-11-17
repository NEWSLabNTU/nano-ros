"""Every TRACKED board definition, for gates that resolve board names.

`BoardCatalog` reads `packages/boards/` plus the roots in
`$NROS_EXTRA_BOARD_PATH`. A gate that resolves a tracked `system.toml`'s board
must see the same set the build does, and the tree holds one tracked extra
root: phase-577 W8's out-of-tree board-package fixture, whose examples name a
board no in-tree definition claims (RFC-0135 D2). Its roots are listed once,
in `.config/fixture-board-roots.txt`, so no gate spells them.
"""
import os
import subprocess

_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _fixture_roots():
    with open(os.path.join(_ROOT, ".config", "fixture-board-roots.txt")) as fh:
        return tuple(
            line.strip() for line in fh if line.strip() and not line.startswith("#")
        )


# Tracked board roots outside `packages/boards/` (.config/fixture-board-roots.txt).
FIXTURE_BOARD_ROOTS = _fixture_roots()

BOARD_ROOTS = ("packages/boards",) + FIXTURE_BOARD_ROOTS


def tracked_board_definitions(root="."):
    """Repo-relative paths of every tracked `nros-board.toml`, sorted."""
    out = subprocess.run(
        ["git", "-C", root, "ls-files", "--", *[f"{r}/**/nros-board.toml" for r in BOARD_ROOTS]],
        check=True, capture_output=True, text=True,
    ).stdout.split()
    return sorted(p for p in out if "/target/" not in p and "/build/" not in p)

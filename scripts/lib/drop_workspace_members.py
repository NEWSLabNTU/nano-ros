#!/usr/bin/env python3
"""Drop the members of a migrated witness workspace from a list of example leaves.

Reads repo-relative paths (one per line) on stdin and prints all but those
inside a WITNESS workspace: `examples/<plat>/<lang>/<name>/` carrying a
`.colcon_workspace` marker, i.e. a single-node witness migrated in place
(phase-531 W6, RFC-0121 D8; the same shape `workspace_image::
is_witness_workspace` recognises). Its node package is not a leaf: it has no
`system.toml` of its own, and syncing it as one writes leaf residue into the
tree and rewrites its managed rows. The witness's fixture rows build it.

`examples/workspaces/` and `examples/templates/` keep the treatment each
walker already gives them; only the witness shape is new.
"""

import sys
from pathlib import Path

MARKER = ".colcon_workspace"


def in_workspace(rel: str) -> bool:
    parts = Path(rel).parts
    if len(parts) < 5 or parts[0] != "examples" or parts[1] in ("workspaces", "templates"):
        return False
    return (Path(*parts[:4]) / MARKER).is_file()


def main() -> int:
    for line in sys.stdin:
        rel = line.strip()
        if rel and not in_workspace(rel):
            print(rel)
    return 0


if __name__ == "__main__":
    sys.exit(main())

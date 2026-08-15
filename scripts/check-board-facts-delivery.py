#!/usr/bin/env python3
"""phase-351 W5 — every cargo target cmake creates must receive the board facts.

RFC-0049's board rung has been reachable in principle and dead in practice since
phase-290: the value existed, and nothing carried it to the build script that
reads it. phase-349 W2.0 tried a leaf `[env]` row and measured why that cannot
work — Corrosion invokes cargo from `workspace_toml_dir`, so a workspace
MEMBER's own `.cargo/config.toml` is never read. W5 moves delivery to the
invoker (`nros_board_facts_env`, `cmake/NanoRosBoardFacts.cmake`).

The failure mode this gate exists for is not a wrong value. It is NO value,
defaulted, with no diagnostic — the shape issue 0529 took two wrong write-ups to
characterise. A new `corrosion_import_crate()` that forgets the helper is
indistinguishable, at build time, from one that has nothing to deliver.

Rule
----
Every `corrosion_import_crate(` in `cmake/` must be followed, in the same
function body, by `nros_board_facts_env(<target>)` — unless the file is listed
in EXEMPT below with a reason.

Buildless: reads `cmake/**/*.cmake`.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CMAKE = os.path.join(ROOT, "cmake")

# Imports that carry no board rung, each with the reason it cannot.
EXEMPT = {
    # The metadata probe builds a HOST helper to read a workspace's own
    # manifests; it is not the deploy's image and has no board (RFC-0048).
    "nano_ros_workspace_metadata.cmake": "host-side metadata probe, no board in play",
    # Corrosion's own loader — finds and loads the tool, never a nano-ros crate.
    "NanoRosCorrosion.cmake": "loads Corrosion itself; imports nothing of ours",
}


def main():
    offenders, checked, exempt = [], 0, 0
    for dirpath, _dirs, files in os.walk(CMAKE):
        for name in sorted(files):
            if not name.endswith(".cmake"):
                continue
            path = os.path.join(dirpath, name)
            with open(path, encoding="utf-8") as fh:
                src = fh.read()
            if "corrosion_import_crate(" not in src:
                continue
            # The definition of the helper itself is not a call site.
            if name == "NanoRosBoardFacts.cmake":
                continue
            if name in EXEMPT:
                exempt += 1
                continue
            checked += 1
            if "nros_board_facts_env(" not in src:
                offenders.append(os.path.relpath(path, ROOT))

    if offenders:
        sys.stderr.write(
            "check-board-facts-delivery: FAILED — cargo target(s) that receive no board facts:\n"
        )
        for o in offenders:
            sys.stderr.write(f"  {o}\n")
        sys.stderr.write(
            "\n  This file imports a Corrosion crate but never calls\n"
            "  `nros_board_facts_env(<target>)`, so its cargo invocation carries no\n"
            "  board rung and no site config (phase-351 W5). A workspace member cannot\n"
            "  read them from its own `.cargo/config.toml` — Corrosion runs cargo from\n"
            "  the workspace root (phase-349 W2.0). The build script then DEFAULTS every\n"
            "  knob, silently, which is issue 0529's shape.\n\n"
            "  Add the call beside `nros_cargo_profile_env`, or list the file in this\n"
            "  gate's EXEMPT map with the reason it carries no board.\n"
        )
        return 1

    print(
        f"board-facts delivery: OK ({checked} cargo-importing cmake file(s) deliver "
        f"the facts, {exempt} exempt)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

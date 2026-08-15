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
Every file that SPAWNS CARGO must deliver the facts:

  * a Corrosion consumer (`corrosion_import_crate(`) calls
    `nros_board_facts_env(<target>)`;
  * a lane that builds its own cargo command (`cmake -E env … cargo`) calls
    `nros_resolve_board_facts()` and puts the result on that command.

Both halves are needed because they are different mechanisms, and checking only
the first is how the ZEPHYR arm shipped inert: that lane uses no Corrosion, so
the original rule could not see it, and its `NANO_ROS_BOARD` is never set — the
helper resolved nothing and (then) said nothing. Found only when the lane could
finally run, which is the point of gating the mechanism rather than the value.

Buildless: reads `cmake/**/*.cmake` and `zephyr/cmake/*.cmake`.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CMAKE_DIRS = (os.path.join(ROOT, "cmake"), os.path.join(ROOT, "zephyr", "cmake"))

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
    walked = []
    for root in CMAKE_DIRS:
        for dirpath, _dirs, files in os.walk(root):
            walked.append((dirpath, files))
    for dirpath, files in walked:
        for name in sorted(files):
            if not name.endswith(".cmake"):
                continue
            path = os.path.join(dirpath, name)
            with open(path, encoding="utf-8") as fh:
                src = fh.read()
            spawns_corrosion = "corrosion_import_crate(" in src
            # `cmake -E env … cargo` — a lane that builds its own command.
            spawns_own = re.search(r"-E env(.|\n)*?\bcargo\b", src) is not None
            if not spawns_corrosion and not spawns_own:
                continue
            # The definition of the helper itself is not a call site.
            if name == "NanoRosBoardFacts.cmake":
                continue
            if name in EXEMPT:
                exempt += 1
                continue
            checked += 1
            delivers = "nros_board_facts_env(" in src or (
                "nros_resolve_board_facts(" in src and "NROS_BOARD_FACTS_ENV" in src
            )
            if not delivers:
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
        f"board-facts delivery: OK ({checked} cargo-spawning cmake file(s) deliver "
        f"the facts, {exempt} exempt)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

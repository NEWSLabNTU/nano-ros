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

Population (issue 1541, phase-472 W5)
-------------------------------------
EVERY tracked cmake file, by FILE KIND: `*.cmake`, `CMakeLists.txt` and their
`.in` templates, harvested from the git index. It used to be two authored
directories, `cmake/` and `zephyr/cmake/`, and the rule's subject had spread
past both: the NuttX lane's own-command `cargo build`
(`packages/api/nros-c/cmake/nros-nuttx.cmake`) and the Corrosion imports in
nros-c's, nros-cpp's and the zenoh staticlib's `CMakeLists.txt` delivered
nothing, and the gate reported OK over a tree where they did not — the same
narrower-than-the-rule shape as the Zephyr arm above, one directory over.

The IMAGE rung (issue 1712)
---------------------------
A C/C++ image's own `[image.<id>] env` (RFC-0049's APP rung) reaches cargo on
the same commands, by a different carrier: a `--config` file (cargo `[env]`, no
`force`), because a `cmake -E env` row overwrites an exported variable and the
image rung sits BELOW it. A Corrosion target gets it inside
`nros_board_facts_env`, so the rule above already covers it. An own-command
lane must ALSO call `nros_image_env_cargo_flags()` and put the result on its
command — or be listed in `IMAGE_ENV_EXEMPT` with the reason it cannot. Before
1712 every cmake road parsed the rows and dropped them, with no diagnostic.

Full-line `#` comments are stripped before either question is asked, in both
directions: a comment NAMING the lane's shape is not a spawn, and a comment
naming the helper is not a delivery.

Buildless. Runs its own negative control on every invocation.
"""

import os
import re
import sys
import sys as _sys
from pathlib import Path as _Path
import sys as _w3_sys  # noqa: E402
from pathlib import Path as _W3Path  # noqa: E402
_w3_sys.path.insert(0, str(_W3Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper
_sys.path.insert(0, str(_Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # issue 0721: index lookup, not a walk

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Imports that carry no board rung, each with the reason it cannot. Keyed by
# REPO-RELATIVE path: with a repo-wide population a basename is not unique.
EXEMPT = {
    # The metadata probe builds a HOST helper to read a workspace's own
    # manifests; it is not the deploy's image and has no board (RFC-0048).
    "cmake/nano_ros_workspace_metadata.cmake": "host-side metadata probe, no board in play",
    # Corrosion's own loader — finds and loads the tool, never a nano-ros crate.
    "cmake/NanoRosCorrosion.cmake": "loads Corrosion itself; imports nothing of ours",
    # The `nros` CLI: a HOST tool the superproject builds to run codegen. It is
    # never linked into an image, so there is no deploy and no board to state.
    "packages/cli/nros-cli/CMakeLists.txt": "builds the host `nros` CLI, never an image",
    # A staged integration-test template (`@NANO_ROS_ROOT@` is rewritten at
    # stage time) exercising the Corrosion superbuild of a user-shaped HOST
    # workspace; it declares no deploy, so `nros ws board-facts` has nothing
    # to resolve for it.
    "packages/testing/nros-tests/fixtures/multi_pkg_workspace_mixed/CMakeLists.txt":
        "host-only test-fixture template; declares no deploy/board",
}

# Own-command lanes that cannot carry the image rung, each with the reason.
IMAGE_ENV_EXEMPT = {
    # The Zephyr west road builds this command while Zephyr loads its modules,
    # before the application's find_package(nano_ros) reads its system.toml,
    # and resolves knobs through its own Kconfig ladder.
    "zephyr/cmake/nros_cargo_build.cmake": "west road -- issue 1721",
    "zephyr/cmake/nros_generate_interfaces.cmake": "west road -- issue 1721",
}

# `nros_image_env_cargo_flags(<var>)` at statement position.
DELIVERS_IMAGE_ENV = re.compile(r"^\s*nros_image_env_cargo_flags\s*\(", re.M)

# The helper's own definition is not a call site.
DEFINITION = "cmake/NanoRosBoardFacts.cmake"

# `nros_board_facts_env(<t>)` or its deferred form (issue 1541), at statement
# position — a mention inside a message string is not a call.
DELIVERS_CORROSION = re.compile(r"^\s*nros_board_facts_env(_deferred)?\s*\(", re.M)

# The resolved list READ — `${NROS_BOARD_FACTS_ENV}` or `IN LISTS …` — not
# merely named: `set(NROS_BOARD_FACTS_ENV "")` names it and delivers nothing.
USES_FACTS = re.compile(r"\$\{NROS_BOARD_FACTS_ENV\}|IN LISTS NROS_BOARD_FACTS_ENV\b")

OWN_CARGO = re.compile(r"-E env(.|\n)*?(\bcargo\b|\$\{\w*(?i:cargo)\w*\})")


def is_cmake_file(rel):
    name = rel.rsplit("/", 1)[-1]
    return (
        name.endswith(".cmake")
        or name.endswith(".cmake.in")
        or name in ("CMakeLists.txt", "CMakeLists.txt.in")
    )


def population(repo=ROOT):
    """Every tracked cmake file, repo-relative. Harvested, not authored."""
    out = []
    for f in tracked(repo, repo=repo):
        rel = os.path.relpath(str(f), repo).replace(os.sep, "/")
        if is_cmake_file(rel):
            out.append(rel)
    return sorted(out)


def strip_comments(src):
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(src, "cmake")


def image_env_missing(src):
    """True when an own-command lane does not carry the image rung (issue 1712)."""
    code = strip_comments(src)
    return OWN_CARGO.search(code) is not None and DELIVERS_IMAGE_ENV.search(code) is None


def classify(src):
    """(spawns_cargo, delivers) for one file's source text."""
    code = strip_comments(src)
    spawns_corrosion = "corrosion_import_crate(" in code
    # `cmake -E env … cargo` — a lane that builds its own command. The cargo
    # word may be a VARIABLE holding the tool (`"${_nnbe_cargo}" build`, from
    # `nros_rust_tool`, issue 1304): `\bcargo\b` has no word boundary inside
    # `_nnbe_cargo`, so the NuttX lane matched only by the accident of a later
    # bare `cargo` elsewhere in the file.
    spawns_own = OWN_CARGO.search(code) is not None
    delivers = DELIVERS_CORROSION.search(code) is not None or (
        "nros_resolve_board_facts(" in code and USES_FACTS.search(code) is not None
    )
    return (spawns_corrosion or spawns_own), delivers


def pop_all():
    return population()


def scan(repo=ROOT):
    offenders, checked, exempt = [], 0, 0
    files = population(repo)
    for rel in files:
        with open(os.path.join(repo, rel), encoding="utf-8") as fh:
            src = fh.read()
        spawns, delivers = classify(src)
        if not spawns or rel == DEFINITION:
            continue
        if rel in EXEMPT:
            exempt += 1
            continue
        checked += 1
        if not delivers:
            offenders.append(rel)
        elif rel not in IMAGE_ENV_EXEMPT and image_env_missing(src):
            offenders.append(f"{rel} (no image rung: nros_image_env_cargo_flags, issue 1712)")
    return offenders, checked, exempt, len(files)


def self_test():
    """Negative control, on the normal path (phase-395): the rule must FIRE.

    Each case is the shape of a file this gate once passed or once missed.
    """
    # The pre-fix NuttX lane: its own env-wrapped cargo command, entity facts
    # only. This is issue 1541's file as origin/main carried it.
    nuttx_before = (
        "nros_rust_tool(_nnbe_cargo cargo)\n"
        "add_custom_command(OUTPUT x\n"
        "    COMMAND ${CMAKE_COMMAND} -E env ${_nnbe_entity_env}\n"
        '        "${_nnbe_cargo}" build --profile p)\n'
    )
    assert classify(nuttx_before) == (True, False), "own-command lane, no facts, must fire"
    nuttx_after = "nros_resolve_board_facts()\n" + nuttx_before.replace(
        "${_nnbe_entity_env}", "${_nnbe_entity_env} ${NROS_BOARD_FACTS_ENV}"
    )
    assert classify(nuttx_after) == (True, True), "own-command lane with facts must pass"
    resolved_not_used = 'nros_resolve_board_facts()\nset(NROS_BOARD_FACTS_ENV "")\n' + nuttx_before
    assert classify(resolved_not_used) == (True, False), \
        "resolving the facts without putting them on the command must fire"

    # Issue 1712 -- an own-command lane that carries the board but not the
    # image rung must fire; one that also asks for the flags passes; a
    # Corrosion-only file needs nothing more (the board helper attaches it).
    assert image_env_missing(nuttx_after), "own-command lane without the image rung must fire"
    assert not image_env_missing(
        "nros_image_env_cargo_flags(_f)\n" + nuttx_after.replace("build --profile p", "build ${_f}")
    ), "own-command lane with the image rung must pass"
    assert image_env_missing("# nros_image_env_cargo_flags(_f)\n" + nuttx_after), \
        "a commented-out image rung must not count"
    assert not image_env_missing("corrosion_import_crate(MANIFEST_PATH x)\nnros_board_facts_env(y)\n")
    for rel in IMAGE_ENV_EXEMPT:
        assert rel in pop_all(), f"IMAGE_ENV_EXEMPT names a file that is not tracked: {rel}"

    corrosion_before = "corrosion_import_crate(MANIFEST_PATH x CRATES y)\n"
    assert classify(corrosion_before) == (True, False), "Corrosion import, no facts, must fire"
    assert classify(corrosion_before + "nros_board_facts_env(y-static)\n") == (True, True)
    assert classify(corrosion_before + "nros_board_facts_env_deferred(y-static)\n") == (True, True)
    assert classify(corrosion_before + 'message(STATUS "call nros_board_facts_env(y)")\n') \
        == (True, False), "a helper named inside a string is not a call"

    # A COMMENT naming the helper is not a delivery...
    assert classify(corrosion_before + "# nros_board_facts_env(y-static)\n") == (True, False), \
        "a commented-out delivery must not count"
    # ...and a comment naming the shape is not a spawn.
    assert classify("# ... -E env ... cargo build\n")[0] is False, \
        "a comment naming the shape must not read as a spawn"

    # REACH (issue 1541): the population is the tree's cmake files by KIND, so
    # it must include files outside the two directories it used to be limited
    # to — the NuttX lane module and a package's own CMakeLists.txt.
    pop = population()
    assert "packages/api/nros-c/cmake/nros-nuttx.cmake" in pop, \
        "population must reach the NuttX lane module"
    assert "packages/api/nros-c/CMakeLists.txt" in pop, \
        "population must reach package CMakeLists.txt files"
    # A stale EXEMPT entry exempts nothing and reads as a decision.
    for rel in EXEMPT:
        assert rel in pop, f"EXEMPT names a file that is not tracked: {rel}"


def main():
    self_test()
    offenders, checked, exempt, total = scan()
    if checked == 0:
        sys.stderr.write(
            "check-board-facts-delivery: FAILED — examined no cargo-spawning cmake "
            "file; a population of zero is not a pass\n"
        )
        return 1

    if offenders:
        sys.stderr.write(
            "check-board-facts-delivery: FAILED — cargo target(s) that receive no board facts:\n"
        )
        for o in offenders:
            sys.stderr.write(f"  {o}\n")
        sys.stderr.write(
            "\n  This file spawns cargo — a Corrosion import, or its own `cmake -E env`\n"
            "  cargo command — and delivers no board facts: it never calls\n"
            "  `nros_board_facts_env(<target>)` (Corrosion), nor\n"
            "  `nros_resolve_board_facts()` with `${NROS_BOARD_FACTS_ENV}` on the\n"
            "  command (its own). Its cargo invocation then carries no board rung and\n"
            "  no site config (phase-351 W5). A workspace member cannot read them from\n"
            "  its own `.cargo/config.toml` — Corrosion runs cargo from the workspace\n"
            "  root (phase-349 W2.0). The build script then DEFAULTS every knob,\n"
            "  silently, which is issue 0529's shape.\n\n"
            "  Deliver them, or list the file in this gate's EXEMPT map with the\n"
            "  reason it carries no board.\n\n"
            "  A line marked `no image rung` delivers the board but not the image's\n"
            "  own `[image.<id>] env` (issue 1712): call `nros_image_env_cargo_flags()`\n"
            "  and put the flag on the cargo command (and the file in DEPENDS), or list\n"
            "  the file in IMAGE_ENV_EXEMPT with the reason.\n"
        )
        return 1

    print(
        f"board-facts delivery: OK ({checked} cargo-spawning cmake file(s) deliver "
        f"the facts, {exempt} exempt, of {total} tracked cmake file(s))"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

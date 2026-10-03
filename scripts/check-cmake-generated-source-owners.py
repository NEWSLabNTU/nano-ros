#!/usr/bin/env python3
"""A generated source file belongs to ONE target — issue 1311.

WHAT WENT WRONG
---------------
`add_custom_command(OUTPUT …)` is not a node in a global graph under the
Makefile generators. CMake copies the rule into the `build.make` of EVERY
target that lists one of its outputs as a source, and `cmake --build
--parallel` drives each target through its OWN sub-make. So the command runs
once per consuming target, CONCURRENTLY, with every copy writing the same file.

Measured in this repo on 2026-09-18, on a cold build of
`packages/rmw/cyclonedds/nros-rmw-cyclonedds` (nine test targets shared one
generated `.srv` → IDL → descriptor set):

    idlc test_string.idl                       ran 10 times
    idlc AddTwoInts.idl                        ran 11 times
    msg_to_cyclone_idl nros_test/srv/SumSeq.srv ran  9 times

and a `.c` that opened its own header mid-write compiled against a prefix that
had not yet reached `#include "dds/ddsc/dds_public_impl.h"`:

    gen/AddTwoInts.c:11:14: error: unknown type name 'uint32_t'
    gen/AddTwoInts.c:14:3:  error: 'DDS_OP_ADR' undeclared here

A truncated `.c` loses its descriptor instead, which is the same race one step
later as `undefined reference to '…__desc'`. CMake's own `add_custom_command`
documentation names the misuse: "Do not list the output in more than one
independent target that may build in parallel or the instances of the rule may
conflict."

WHY A GATE AND NOT JUST THE FIX
-------------------------------
The failure is invisible in a warm tree — a checkout whose generated files are
already up to date re-runs no generation rule at all — and Ninja is immune, so
every Zephyr/west consumer of the same helpers is clean. It therefore looked
for months like a property of whoever had just provisioned a fresh worktree.
`check-build` (which is where the Cyclone lane lives) runs in CI on
`schedule`/`workflow_dispatch` only, so a regression here gets days of cover.

REACH vs RULE (stated, not pretended — CLAUDE.md, issue 0196)
-------------------------------------------------------------
The RULE is "no generated file is a source of two targets that can build in
parallel". This gate's REACH is narrower on purpose: it follows the output
variables of the two nano-ros codegen helpers

    nros_rmw_cyclonedds_idlc_compile(<var> …)
    nros_rmw_cyclonedds_generate_from_msg(<var> …)

because those are the sites where a source list is produced by a custom command
and then handed around by name, AND every raw `add_custom_command(OUTPUT …)`,
item by item (issue 1660, phase-472 W6). The raw form was out of reach until
2026-10-03 — a bare `OUTPUT x.c` typed into two `add_library` calls passed
while the same shape through the helper failed — so each OUTPUT item is now a
producer, and a consumer is any target-source command naming it directly or
through ONE level of `set(V …)` / `list(APPEND V …)`. Deeper variable flow is
still out of reach and stated so. If you add another helper that returns
generated sources, add it to `PRODUCERS`.

Usage::

    check-cmake-generated-source-owners.py          # the gate
    check-cmake-generated-source-owners.py --list   # every producer + its consumers
"""

import re
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper
from per_item import cmake_args, cmake_calls, cmake_keyword_items  # noqa: E402  W6
from file_kinds import files_of_kind  # noqa: E402  phase-472 W5
from population import require_population  # noqa: E402  phase-472 W4

REPO = Path(__file__).resolve().parent.parent

# Functions that RETURN a list of files written by an add_custom_command.
PRODUCERS = (
    "nros_rmw_cyclonedds_idlc_compile",
    "nros_rmw_cyclonedds_generate_from_msg",
)

# The remedy: it puts the generated files in one OBJECT library and returns
# `$<TARGET_OBJECTS:…>`, so its own output variable may be consumed freely.
OWNER_WRAPPER = "nros_rmw_cyclonedds_own_generated_sources"

# Commands that make a file a SOURCE of a target. `zephyr_library_sources` is
# Zephyr's wrapper around `target_sources` on the module library.
CONSUMERS = (
    "add_executable",
    "add_library",
    "target_sources",
    "zephyr_library_sources",
)

# `add_custom_command(OUTPUT …)`'s keywords: an OUTPUT list ends at the next.
CUSTOM_COMMAND_KEYWORDS = {
    "OUTPUT", "COMMAND", "MAIN_DEPENDENCY", "DEPENDS", "BYPRODUCTS", "IMPLICIT_DEPENDS",
    "WORKING_DIRECTORY", "COMMENT", "DEPFILE", "JOB_POOL", "JOB_SERVER_AWARE",
    "VERBATIM", "APPEND", "USES_TERMINAL", "COMMAND_EXPAND_LISTS", "DEPENDS_EXPLICIT_ONLY",
    "CODEGEN", "TARGET", "PRE_BUILD", "PRE_LINK", "POST_BUILD",
}


def strip_comments(text: str) -> str:
    """Drop `#` comments. Quotes are respected; bracket comments are not used here."""
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, "cmake")


def first_arg(args: str) -> str:
    toks = args.split()
    return toks[0] if toks else ""


def analyse(text: str):
    """Return (produced, hits) for one file's text.

    `produced` maps a producer KEY to its line: a helper's output variable
    (`_gen`), or — per item — each raw `add_custom_command` OUTPUT (`x.c`).
    `hits` maps the same key to [(consumer_command, line), …].
    """
    text = strip_comments(text)
    calls = cmake_calls(text)

    produced: dict[str, int] = {}
    raw: set[str] = set()
    owned: set[str] = set()
    assigned: dict[str, set] = {}  # one level of `set(V …)` / `list(APPEND V …)`
    for name, args, line in calls:
        if name in PRODUCERS:
            var = first_arg(args)
            if var and not var.startswith("$"):
                produced.setdefault(var, line)
        elif name == OWNER_WRAPPER:
            var = first_arg(args)
            if var:
                owned.add(var)
        elif name == "add_custom_command" and "TARGET" not in cmake_args(args)[:1]:
            for item in cmake_keyword_items(args, "OUTPUT", CUSTOM_COMMAND_KEYWORDS):
                produced.setdefault(item, line)
                raw.add(item)
        elif name == "set":
            toks = cmake_args(args)
            if toks:
                assigned.setdefault(toks[0], set()).update(toks[1:])
        elif name == "list":
            toks = cmake_args(args)
            if len(toks) > 2 and toks[0] == "APPEND":
                assigned.setdefault(toks[1], set()).update(toks[2:])

    hits: dict[str, list] = {v: [] for v in produced}
    for name, args, line in calls:
        if name not in CONSUMERS:
            continue
        toks = cmake_args(args)
        for key in produced:
            if key in raw:
                via = {f"${{{v}}}" for v, vals in assigned.items() if key in vals}
                if key in toks or via.intersection(toks):
                    hits[key].append((name, line))
            elif f"${{{key}}}" in args and key not in owned:
                hits[key].append((name, line))
    return produced, hits


def tracked_cmake_files() -> list[Path]:
    # The KIND, not a pathspec (phase-472 W5): vendored `third-party/` is out.
    return [REPO / p for p in files_of_kind("cmake", repo=REPO)]


GOOD = """
nros_rmw_cyclonedds_generate_from_msg(_gen PKG_NAME p)
nros_rmw_cyclonedds_own_generated_sources(_srcs owner SOURCES ${_gen})
add_executable(a a.cpp ${_srcs})
add_executable(b b.cpp ${_srcs})
target_sources(c PRIVATE ${_srcs})
"""

BAD = """
nros_rmw_cyclonedds_generate_from_msg(_gen PKG_NAME p)
add_executable(a a.cpp ${_gen})
add_executable(b b.cpp ${_gen})
"""

# Issue 1660 — the raw shape, which the helper-only reach never saw.
RAW_BAD = """
add_custom_command(OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/x.c COMMAND gen VERBATIM)
add_library(a STATIC ${CMAKE_CURRENT_BINARY_DIR}/x.c)
add_library(b STATIC ${CMAKE_CURRENT_BINARY_DIR}/x.c)
"""

RAW_VIA_VAR = """
add_custom_command(OUTPUT gen/y.c gen/y.h COMMAND gen)
set(_srcs gen/y.c)
add_library(a STATIC ${_srcs})
target_sources(b PRIVATE ${_srcs})
"""

RAW_ONE = """
add_custom_command(OUTPUT x.c COMMAND gen)
add_library(a STATIC x.c)
add_custom_target(t DEPENDS x.c)
"""

SINGLE = """
nros_rmw_cyclonedds_idlc_compile(_gen IDL_FILE x.idl)
add_library(one STATIC ${_gen})
"""


def selftest() -> None:
    """Negative control, on the NORMAL path (phase-395): a gate that cannot
    fail is a comment. Runs in milliseconds on three synthetic files."""
    _, hits = analyse(BAD)
    assert len(hits["_gen"]) == 2, f"selftest: two consumers not flagged: {hits}"

    _, hits = analyse(GOOD)
    assert hits["_gen"] == [], f"selftest: the owner pattern was flagged: {hits}"

    # One consumer is the legitimate shape and must stay under the threshold.
    _, hits = analyse(SINGLE)
    assert len(hits["_gen"]) == 1, f"selftest: a single consumer miscounted: {hits}"

    # Raw `add_custom_command(OUTPUT …)`, per item (issue 1660).
    _, hits = analyse(RAW_BAD)
    assert len(hits["${CMAKE_CURRENT_BINARY_DIR}/x.c"]) == 2, f"selftest: raw output: {hits}"
    _, hits = analyse(RAW_VIA_VAR)
    assert len(hits["gen/y.c"]) == 2 and hits["gen/y.h"] == [], f"selftest: via var: {hits}"
    _, hits = analyse(RAW_ONE)
    assert len(hits["x.c"]) == 1, f"selftest: one owner + a custom target: {hits}"


def main(argv: list[str]) -> int:
    selftest()

    files = tracked_cmake_files()
    listing = argv[:1] == ["--list"]
    errs: list[str] = []
    seen = 0

    for path in files:
        try:
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if not any(p in text for p in PRODUCERS + ("add_custom_command",)):
            continue
        produced, hits = analyse(text)
        rel = path.relative_to(REPO)
        for var, decl_line in produced.items():
            seen += 1
            consumers = hits[var]
            # A helper key is a variable name; a raw OUTPUT item is the path itself.
            shown = var if var.startswith("$") or "/" in var or "." in var else f"${{{var}}}"
            if listing:
                where = ", ".join(f"{c}@{ln}" for c, ln in consumers) or "-"
                print(f"{rel}:{decl_line}: {shown} -> {len(consumers)} target(s) [{where}]")
            if len(consumers) > 1:
                where = "\n".join(f"      {c}() at line {ln}" for c, ln in consumers)
                errs.append(
                    f"  {rel}:{decl_line}: `{shown}` is a source of "
                    f"{len(consumers)} targets:\n{where}\n"
                    f"      Under the Makefile generators each of those targets gets its\n"
                    f"      OWN copy of the generation rule and runs it in parallel into\n"
                    f"      the same files (issue 1311). Give the set ONE owner — an OBJECT\n"
                    f"      library the others consume as `$<TARGET_OBJECTS:…>`; for the\n"
                    f"      cyclone codegen helpers that is\n"
                    f"        {OWNER_WRAPPER}(<var> <owner_target>\n"
                    f"            SOURCES {shown} [INCLUDE_DIRS …])\n"
                    f"      listing `<var>` where `{shown}` was listed."
                )

    if not require_population(seen, "generated source set(s)",
                              gate="check-cmake-generated-source-owners"):
        return 1
    if errs:
        print(
            f"check-cmake-generated-source-owners: {len(errs)} generated source "
            "set(s) with more than one owning target\n",
            file=sys.stderr,
        )
        print("\n".join(errs), file=sys.stderr)
        return 1

    print(
        f"check-cmake-generated-source-owners: OK — {seen} generated source set(s) "
        f"across {len(files)} cmake file(s), each owned by at most one target"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

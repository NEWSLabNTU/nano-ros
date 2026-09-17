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
and then handed around by name. A bare `add_custom_command(OUTPUT x)` whose `x`
is typed into two targets is NOT caught — that needs real variable-flow
analysis of CMake, which this is not. If you add another helper that returns
generated sources, add it to `PRODUCERS`.

Usage::

    check-cmake-generated-source-owners.py          # the gate
    check-cmake-generated-source-owners.py --list   # every producer + its consumers
"""

import re
import subprocess
import sys
from pathlib import Path

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

IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


def strip_comments(text: str) -> str:
    """Drop `#` comments. Quotes are respected; bracket comments are not used here."""
    out = []
    in_str = False
    i = 0
    while i < len(text):
        c = text[i]
        if in_str:
            out.append(c)
            if c == "\\" and i + 1 < len(text):
                out.append(text[i + 1])
                i += 2
                continue
            if c == '"':
                in_str = False
        elif c == '"':
            in_str = True
            out.append(c)
        elif c == "#":
            while i < len(text) and text[i] != "\n":
                i += 1
            continue
        else:
            out.append(c)
        i += 1
    return "".join(out)


def iter_calls(text: str):
    """Yield (command, argument-text, line) for every command invocation.

    Scanning continues INSIDE an argument list, so a command nested in an
    `if(...)`/`foreach(...)` body is seen as well.
    """
    n = len(text)
    i = 0
    while True:
        m = IDENT.search(text, i)
        if not m:
            return
        k = m.end()
        while k < n and text[k] in " \t\n":
            k += 1
        if k < n and text[k] == "(":
            depth = 0
            p = k
            while p < n:
                if text[p] == "(":
                    depth += 1
                elif text[p] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                p += 1
            if depth != 0:
                return
            yield m.group(0), text[k + 1 : p], text.count("\n", 0, m.start()) + 1
            i = k + 1
        else:
            i = m.end()


def first_arg(args: str) -> str:
    toks = args.split()
    return toks[0] if toks else ""


def analyse(text: str):
    """Return {producer_var: [(consumer_command, line), ...]} for one file's text."""
    text = strip_comments(text)
    calls = list(iter_calls(text))

    produced: dict[str, int] = {}
    owned: set[str] = set()
    for name, args, line in calls:
        if name in PRODUCERS:
            var = first_arg(args)
            if var and not var.startswith("$"):
                produced.setdefault(var, line)
        elif name == OWNER_WRAPPER:
            var = first_arg(args)
            if var:
                owned.add(var)

    hits: dict[str, list] = {v: [] for v in produced}
    for name, args, line in calls:
        if name not in CONSUMERS:
            continue
        for var in produced:
            if f"${{{var}}}" in args and var not in owned:
                hits[var].append((name, line))
    return produced, hits


def tracked_cmake_files() -> list[Path]:
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z", "*.cmake", "CMakeLists.txt",
         "*/CMakeLists.txt"],
        capture_output=True, text=True, check=True,
    ).stdout
    files = [REPO / p for p in out.split("\0") if p]
    return [f for f in files if "third-party/" not in str(f.relative_to(REPO))]


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
        if not any(p in text for p in PRODUCERS):
            continue
        produced, hits = analyse(text)
        rel = path.relative_to(REPO)
        for var, decl_line in produced.items():
            seen += 1
            consumers = hits[var]
            if listing:
                where = ", ".join(f"{c}@{ln}" for c, ln in consumers) or "-"
                print(f"{rel}:{decl_line}: ${{{var}}} -> {len(consumers)} target(s) [{where}]")
            if len(consumers) > 1:
                where = "\n".join(f"      {c}() at line {ln}" for c, ln in consumers)
                errs.append(
                    f"  {rel}:{decl_line}: `${{{var}}}` is a source of "
                    f"{len(consumers)} targets:\n{where}\n"
                    f"      Under the Makefile generators each of those targets gets its\n"
                    f"      OWN copy of the generation rule and runs it in parallel into\n"
                    f"      the same files (issue 1311). Give the set one owner:\n"
                    f"        {OWNER_WRAPPER}(<var> <owner_target>\n"
                    f"            SOURCES ${{{var}}} [INCLUDE_DIRS …])\n"
                    f"      and list `<var>` where `${{{var}}}` was listed."
                )

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

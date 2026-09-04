#!/usr/bin/env python3
"""A cmake custom command that runs `cargo` must carry a `DEPFILE`.

Issue 0820. `add_custom_command(OUTPUT ...)` rebuilds only when one of the
inputs it NAMES is newer than the output. Cargo's inputs are a graph over the
whole workspace, so any hand-written `DEPENDS` is an approximation of it — and
every approximation in this tree has been wrong in the same direction: it named
the crate's own files and no nano-ros Rust at all. The result is a museum
binary: an artifact NEWER than its sources, containing older code, with nothing
in the build graph able to notice.

Cargo already publishes the exact answer. It writes a Makefile-format dep-info
file beside the artifact (`<artifact>.d`, absolute paths, one target), and cmake
consumes exactly that shape via `DEPFILE`. A missing depfile on the first build
is a warning, not an error, so the edge is safe from the very first configure.

## Why a gate and not three fixes

Three custom commands in this tree invoke cargo. When this gate was written,
TWO of them had no depfile:

* `packages/api/nros-c/cmake/nros-nuttx.cmake` — the reported site. Fixed
  2026-08-27; a NuttX C example kept the previous build's Rust for as long as
  nobody wiped the directory, and the test that caught it failed at its full
  90 s timeout with no message that pointed anywhere near the build graph.
* `cmake/NanoRosGenerateInterfaces.cmake` — the generated message C++ FFI
  staticlib. Its `DEPENDS` named the generated `.rs`, the crate manifest and
  the crate's `lib.rs`. Cargo's own dep-info for that same archive lists
  `packages/core/nros-serdes/src/{cdr,lib,primitives,schema,traits}.rs`
  (measured on a built tree) — the CDR serializer, with no edge.
* `zephyr/cmake/nros_generate_interfaces.cmake` — the same command, narrower
  still: not even the generated sources were named.

The 2026-09-01 sweep on this issue looked for FILES containing both
`add_custom_command` and `cargo`, and both generators were in its output — but
they were classified by what the file mostly does ("codegen") rather than by
what each command does, so the two survivors read as legitimate. That is the
reason this is a gate: the predicate has to be per-COMMAND, and a person
reading a 700-line cmake module will not apply it per-command by eye.

## What is flagged

An `add_custom_command(...)` block with a `COMMAND` argument list containing the
bare token `cargo` (the program), and no `DEPFILE` keyword.

Deliberately NOT flagged:

* `add_custom_target(... COMMAND cargo ...)` — a custom target has no output and
  always runs, so it needs no edge. The hazard is specific to an `OUTPUT` whose
  freshness is decided by mtime.
* A command that merely mentions a cargo-shaped path or target name —
  `${CMAKE_BINARY_DIR}/cargo`, `cargo-build_nros_c` in `DEPENDS`, a
  `_NROS_SHARED_CARGO_*` variable. Those do not invoke cargo, and the three of
  them in this tree are why the predicate reads COMMAND sections only.

If a cargo command ever legitimately cannot carry a depfile, extend this gate
with the case and the reason rather than silencing it at the call site — a
per-site exemption is how a rule stops describing the tree.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Keywords that end a COMMAND argument list inside add_custom_command().
_KEYWORDS = {
    "APPEND", "BYPRODUCTS", "COMMAND", "COMMAND_EXPAND_LISTS", "COMMENT",
    "CODEGEN", "DEPENDS", "DEPENDS_EXPLICIT_ONLY", "DEPFILE", "IMPLICIT_DEPENDS",
    "JOB_POOL", "JOB_SERVER_AWARE", "MAIN_DEPENDENCY", "OUTPUT", "PRE_BUILD",
    "PRE_LINK", "POST_BUILD", "TARGET", "USES_TERMINAL", "VERBATIM",
    "WORKING_DIRECTORY",
}

# The cargo PROGRAM: a bare `cargo` token. The lookbehind rejects a path
# (`${CMAKE_BINARY_DIR}/cargo`) or a variable tail; the lookahead rejects a
# target name (`cargo-build_nros_c`, `cargo_target_dir`).
_CARGO = re.compile(r"(?<![\w/.$-])cargo(?![\w.-])")

_STRIP_COMMENTS = re.compile(r"(?m)#.*$")


def _blocks(text: str):
    """Yield (line_no, block_text) for every add_custom_command(...) call."""
    for m in re.finditer(r"\badd_custom_command\s*\(", text):
        i, depth = m.end(), 1
        while i < len(text) and depth:
            if text[i] == "(":
                depth += 1
            elif text[i] == ")":
                depth -= 1
            i += 1
        yield text[: m.start()].count("\n") + 1, text[m.start(): i]


def _parse(block: str) -> tuple[list[str], set[str]]:
    """(COMMAND argument texts, keywords actually present) for one block.

    Comments are stripped first: a commented-out `COMMAND cargo` is not a
    command, and a commented-out `DEPFILE` is not an edge.
    """
    body = _STRIP_COMMENTS.sub("", block)
    seen: set[str] = set()
    sections: list[str] = []
    cur: list[str] | None = None
    for tok in re.split(r"(\s+)", body):
        bare = tok.strip().strip('"')
        if bare in _KEYWORDS:
            seen.add(bare)
            if cur is not None:
                sections.append(" ".join(cur))
            cur = [] if bare == "COMMAND" else None
            continue
        if cur is not None and tok.strip():
            cur.append(tok)
    if cur is not None:
        sections.append(" ".join(cur))
    return sections, seen


def scan(text: str) -> list[int]:
    """Line numbers of add_custom_command blocks that run cargo without DEPFILE."""
    bad = []
    for line, block in _blocks(text):
        commands, keywords = _parse(block)
        if not any(_CARGO.search(c) for c in commands):
            continue
        if "DEPFILE" in keywords:
            continue
        bad.append(line)
    return bad


def cmake_files() -> list[Path]:
    """Tracked cmake sources, via the git index (issue 0721 — never a walk)."""
    out = subprocess.run(
        ["git", "ls-files", "-z", "*.cmake", "*CMakeLists.txt", "*.cmake.in"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout
    return [ROOT / p for p in out.split("\0") if p]


SELF_TESTS: list[tuple[str, str, int]] = [
    ("cargo command, no depfile", 'add_custom_command(OUTPUT "a" COMMAND cargo build)\n', 1),
    ("cargo command with depfile", 'add_custom_command(OUTPUT "a" COMMAND cargo build DEPFILE "a.d")\n', 0),
    ("cargo behind a cmake -E env wrapper",
     'add_custom_command(OUTPUT "a" COMMAND ${CMAKE_COMMAND} -E env X=1 cargo ${_args})\n', 1),
    ("cargo with a +toolchain prefix",
     'add_custom_command(OUTPUT "a" COMMAND cargo +${Rust_TOOLCHAIN} build)\n', 1),
    ("a path that ENDS in cargo is not the program",
     'add_custom_command(OUTPUT "a" COMMAND bash s.sh --link "${CMAKE_BINARY_DIR}/cargo")\n', 0),
    ("a corrosion target named cargo-* in DEPENDS is not the program",
     'add_custom_command(OUTPUT "a" COMMAND bash s.sh DEPENDS cargo-build_nros_c)\n', 0),
    ("an uppercase CARGO variable is not the program",
     'add_custom_command(OUTPUT "a" COMMAND bash "${_NROS_SHARED_CARGO_CHECK_SH}")\n', 0),
    ("add_custom_target needs no edge",
     'add_custom_target(t COMMAND cargo build)\n', 0),
    ("a commented-out cargo command is not a command",
     'add_custom_command(OUTPUT "a"\n  # COMMAND cargo build\n  COMMAND bash s.sh)\n', 0),
    ("a DEPFILE in a comment does not count",
     'add_custom_command(OUTPUT "a" COMMAND cargo build\n  # DEPFILE "a.d"\n)\n', 1),
    ("two COMMANDs, cargo second, no depfile",
     'add_custom_command(OUTPUT "a" COMMAND bash s.sh COMMAND cargo build)\n', 1),
    ("nested parens do not end the block early",
     'add_custom_command(OUTPUT "a" COMMAND cargo build $<TARGET_FILE:t>)\n', 1),
]


def self_test(quiet: bool = False) -> int:
    bad = 0
    for name, src, want in SELF_TESTS:
        got = len(scan(src))
        if got != want:
            print(f"  SELF-TEST FAIL: {name}: expected {want} hit(s), got {got}")
            bad += 1
    if bad:
        print(f"check-cargo-custom-command-depfile: {bad} self-test(s) failed")
        return 1
    if not quiet:
        print(f"check-cargo-custom-command-depfile self-test: {len(SELF_TESTS)} case(s) OK")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()
    # Always, not only behind the flag: a negative control nobody runs decays
    # into a comment, and this rule's whole job is to fire.
    if self_test(quiet=True):
        return 1

    findings = []
    for path in cmake_files():
        try:
            text = path.read_text(errors="replace")
        except OSError:
            continue
        for line in scan(text):
            findings.append((path.relative_to(ROOT), line))

    if findings:
        print("check-cargo-custom-command-depfile: cargo command(s) with no rebuild edge\n")
        for rel, line in findings:
            print(f"  {rel}:{line}: add_custom_command runs `cargo` with no DEPFILE.")
            print("      Its DEPENDS list is a hand-maintained guess at cargo's input")
            print("      graph, and cmake will call the OUTPUT fresh whenever that")
            print("      guess is incomplete — a museum binary (issue 0820).")
            print("      Cargo writes `<artifact>.d` beside the artifact; point")
            print("      DEPFILE at it, derived from the same variable that spells")
            print("      the OUTPUT so the two cannot drift.\n")
        return 1

    print("check-cargo-custom-command-depfile OK — every cargo custom command has a DEPFILE.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

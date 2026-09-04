#!/usr/bin/env python3
"""Every emitted `run_components` call must name a SESSION.

Issue 1003: ten `cmake/templates/*_entry_main*` templates called
`run_components(&__nros_entry_setup)`. One argument binds the delegating
overload that hard-codes `"node"`, so every image built through
`nano_ros_add_node` registered under that name — and the XRCE client key derives
from it, so a talker and a listener hashed to ONE client and the agent saw a
single peer. It is also the name `ros2 node list` shows.

The defect lived from 2026-06-13 to 2026-09-03 with a CORRECT sibling producer
beside it the whole time: `emit_cpp.rs` (the `nros build` path) has passed the
boot-config name since 2026-06-27. Two spellings of one fact, and nothing
compared them.

WHY THIS CHECK DOES NOT KNOW ABOUT OVERLOADS (issue 1017)
--------------------------------------------------------
The obvious gate — "the call passes N arguments" — is wrong, and writing it
would reproduce the very defect it guards. The arity carrying the name differs
by board: `LinuxBoard` takes `(session_name, setup)` and has no locator
parameter, while zephyr/freertos/nuttx/threadx take `(locator, session_name,
setup)`. Encoding that here would make this file a THIRD authored copy of the
overload sets, free to drift from `main.hpp` and from the templates.

So the check asserts something weaker and more durable: whatever the arity, the
call must MENTION a session source. Every board satisfies that today, and a
future board with a fourth shape satisfies it without touching this file.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Each producer names the session differently; both are load-bearing.
PRODUCERS: list[tuple[str, str, str]] = [
    # (glob, marker that proves a session name is passed, human name)
    (
        "cmake/templates/*_entry_main*",
        "NROS_ENTRY_NODE_NAME",
        "CMake entry template (`nano_ros_add_node`)",
    ),
    (
        "packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs",
        "nros_boot_config_node_name",
        "the `nros build` C++ emitter",
    ),
]


def strip_tests(text: str) -> str:
    """Cut a Rust file at `#[cfg(test)]`.

    `emit_cpp.rs` asserts on the emitted C++ in its own tests — including
    `!src.contains("::run_components(")` — and a scanner cannot tell a test's
    quoted fragment from a producer's. Tests are not producers, so they are not
    scanned.

    This assumes tests come last, which is the convention here and true today.
    The assumption is not silent: a producer file that yields NO call after this
    cut is a hard error below, so moving an emitter after the test module fails
    loudly rather than skipping it."""
    idx = text.find("#[cfg(test)]")
    return text if idx == -1 else text[:idx]


def code_only(text: str) -> str:
    """Drop comment lines, then join — so a comment mentioning the call is not
    mistaken for the call, and a call split across source lines still reads as
    one. `emit_cpp.rs` writes its call across a Rust string continuation, and
    both files carry prose about `run_components` that must not be scanned."""
    out: list[str] = []
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("//") or s.startswith("///") or s.startswith("*"):
            continue
        # A Rust string continuation (`\\` at end of line) joins with NO
        # separator: `emit_cpp.rs` splits its `run_components(` call across one,
        # and inserting a space there truncates the call at the backslash — the
        # scan then reads `run_components(")` and reports a false positive.
        if line.rstrip().endswith("\\"):
            out.append(line.rstrip()[:-1])
        else:
            out.append(line + " ")
    return "".join(out)


def calls(blob: str) -> list[str]:
    """Every `run_components( … )` with balanced parens.

    Not a regex: the argument list nests (`nros_boot_config_node_name(&…)`), and
    `[^)]*` would stop at the inner close and silently read a truncated call —
    which is exactly the kind of check that passes while proving nothing."""
    out = []
    for m in re.finditer(r"run_components\s*\(", blob):
        depth, i = 0, m.end() - 1
        while i < len(blob):
            if blob[i] == "(":
                depth += 1
            elif blob[i] == ")":
                depth -= 1
                if depth == 0:
                    out.append(blob[m.start() : i + 1])
                    break
            i += 1
    return out


def scan(text: str, marker: str, is_rust: bool) -> list[str]:
    """Offending calls in `text` — the pure predicate the selftest drives."""
    if is_rust:
        text = strip_tests(text)
    return [c for c in calls(code_only(text)) if marker not in c]


# --------------------------------------------------------------------------
# selftest — runs on the NORMAL path, never behind a flag (phase-395)
# --------------------------------------------------------------------------
#
# Real shapes, not paraphrases: each is lifted from the file it models, so the
# cases are evidence rather than a restatement of the rule.

_TPL_FIXED_ZEPHYR = """
    return static_cast<int>(
        ::nros::board::ZephyrBoard::run_components(NROS_ENTRY_LOCATOR, "@NROS_ENTRY_NODE_NAME@", &__nros_entry_setup));
"""

_TPL_FIXED_NATIVE = """
    return static_cast<int>(
        ::nros::board::LinuxBoard::run_components("@NROS_ENTRY_NODE_NAME@", &__nros_entry_setup));
"""

# Issue 1003 exactly as it stood: ONE argument, so the delegating overload
# supplies `"node"` and every image of that language collides on one client key.
_TPL_BROKEN = """
    return static_cast<int>(
        ::nros::board::ZephyrBoard::run_components(&__nros_entry_setup));
"""

# `emit_cpp.rs` splits the call across a Rust string continuation. Joining lines
# with a SPACE truncates it at the backslash and the scan reads
# `run_components(")` — a false positive this case exists to keep out.
_RS_CONTINUATION = (
    '                "    return static_cast<int>({board}::run_components(\\\n'
    'NROS_ENTRY_LOCATOR, nros_boot_config_node_name(&NROS_BOOT_CONFIG), '
    '&__nros_entry_setup));"\n'
)

# That same file asserts on its own emitted text. Tests are not producers.
_RS_TEST_ONLY = (
    "#[cfg(test)]\n"
    "mod tests {\n"
    "    fn t() {\n"
    '        assert!(!src.contains("::run_components("));\n'
    "    }\n"
    "}\n"
)


def self_test(quiet: bool = True) -> int:
    cases = [
        # (name, text, marker, is_rust, expect_violation)
        ("fixed zephyr template (3-arg)", _TPL_FIXED_ZEPHYR, "NROS_ENTRY_NODE_NAME", False, False),
        ("fixed native template (2-arg)", _TPL_FIXED_NATIVE, "NROS_ENTRY_NODE_NAME", False, False),
        ("issue-1003 template", _TPL_BROKEN, "NROS_ENTRY_NODE_NAME", False, True),
        ("rust string continuation", _RS_CONTINUATION, "nros_boot_config_node_name", True, False),
        ("rust test-only mention", _RS_TEST_ONLY, "nros_boot_config_node_name", True, False),
    ]
    bad = 0
    for name, text, marker, is_rust, expect in cases:
        got = bool(scan(text, marker, is_rust))
        if got != expect:
            bad += 1
            want = "a violation" if expect else "no violation"
            print(f"SELFTEST FAIL: {name}: expected {want}, got the opposite", file=sys.stderr)
        elif not quiet:
            print(f"  ok: {name}")
    if bad:
        print(
            "check-entry-session-name: SELFTEST FAILED — the check cannot be "
            "trusted about the tree until it is right about these.",
            file=sys.stderr,
        )
    elif not quiet:
        print(f"check-entry-session-name selftest: OK ({len(cases)} case(s))")
    return 1 if bad else 0


def main() -> int:
    # The negative control runs BEFORE the tree is inspected: a gate that cannot
    # detect the defect it was written for must not report on anything else.
    if self_test() != 0:
        return 2
    failures: list[str] = []
    checked = 0

    for glob, marker, label in PRODUCERS:
        paths = sorted(ROOT.glob(glob))
        if not paths:
            print(
                f"ERROR: `{glob}` matched no files. A producer that cannot be\n"
                f"       found is not a producer that is correct — refusing to\n"
                f"       report green having checked nothing.",
                file=sys.stderr,
            )
            return 2
        for path in paths:
            raw = path.read_text(encoding="utf8")
            if path.suffix == ".rs":
                raw = strip_tests(raw)
            blob = code_only(raw)
            found = calls(blob)
            if not found:
                # A template may legitimately have none (a non-typed variant);
                # a NAMED producer file must not, or this check is guarding a
                # file that no longer emits the call it was written for.
                if "*" not in glob:
                    print(
                        f"ERROR: {path.relative_to(ROOT)} is listed as a producer "
                        f"({label})\n       but emits no `run_components` call. "
                        f"Either it stopped being one\n       or the emitter moved "
                        f"below `#[cfg(test)]`; either way this check\n       is no "
                        f"longer guarding it.",
                        file=sys.stderr,
                    )
                    return 2
                continue
            for call in found:
                checked += 1
                if marker not in call:
                    rel = path.relative_to(ROOT)
                    failures.append(
                        f"  {rel}\n"
                        f"    {' '.join(call.split())}\n"
                        f"    -> no `{marker}`: this call names no session, so the\n"
                        f"       image registers as the hard-coded default and every\n"
                        f"       image of its language collides on one client key."
                    )

    if failures:
        print(
            "check-entry-session-name: a generated `run_components` call passes no "
            "session name (issue 1003):\n",
            file=sys.stderr,
        )
        print("\n".join(failures), file=sys.stderr)
        print(
            "\n  Pass the node's own name. The ARITY differs by board — `LinuxBoard`\n"
            "  is (session_name, setup) with no locator, the RTOS boards are\n"
            "  (locator, session_name, setup) — so copy the shape from a sibling\n"
            "  template for the same board rather than assuming one form.",
            file=sys.stderr,
        )
        return 1

    if checked == 0:
        print(
            "ERROR: found no `run_components` call in any producer. Either the\n"
            "       emitters moved or this check's globs are stale; either way it\n"
            "       is guarding nothing.",
            file=sys.stderr,
        )
        return 2

    print(f"check-entry-session-name: OK ({checked} generated call(s) name a session)")
    return 0


if __name__ == "__main__":
    if "--self-test" in sys.argv or "--selftest" in sys.argv:
        sys.exit(self_test(quiet=False))
    sys.exit(main())

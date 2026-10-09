#!/usr/bin/env python3
"""phase-483 W1: no C++ a user writes, or the CLI emits, names `nros::`.

The C++ user API lives in `rclcpp::`, `rclcpp_action::` and
`rclcpp_lifecycle::` (RFC-0089, "`nros::` is phased out entirely"), and since
phase-483 there is no `nros::` namespace left to name. A `nros::` token in C++
CODE is therefore either a compile error waiting for the next build of that
file, or a new definition re-opening the namespace. Both are refused here,
before any build reaches the file.

Scope — the C++ that users read and copy, and the C++ the CLI generates:

* every `.cpp` / `.cc` / `.hpp` / `.hh` / `.h` under `examples/`,
  `packages/api/nros-cpp/`, `packages/testing/` and `packages/cli/` (the last
  holds the emitted goldens and message-header fixtures);
* the C++ emitter templates (`packages/cli/**/packs/**/cpp/**` and the
  `*.cpp.golden` / `*.hpp.golden` outputs);
* every ```cpp fenced block in `book/`.

Comments are exempt: a comment saying what a name WAS is history, not a use.
String literals are not exempt — an emitted string is emitted code.

`--self-test` runs the scanner over in-memory samples, both directions.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

CPP_EXT = (".cpp", ".cc", ".hpp", ".hh", ".h", ".cpp.golden", ".hpp.golden")
CPP_ROOTS = ("examples/", "packages/api/nros-cpp/", "packages/testing/", "packages/cli/")
TEMPLATE_MARK = "/packs/"
# `nros::main!` and its siblings are RUST macros, which a refusal message may
# name; `\w+!` after the path is never C++.
TOKEN = re.compile(r"(?<![\w/.\-])(?:::)?nros::(?!\w+!)|\bnamespace\s+nros\b(?!_)")
FENCE = re.compile(r"^\s*(```|~~~)\s*(\S*)")


def strip_comments(src: str) -> str:
    """Blank out `//` and `/* */` comments, keeping line numbers."""
    out = []
    i, n = 0, len(src)
    in_block = in_line = False
    in_str: str | None = None
    while i < n:
        c = src[i]
        nxt = src[i + 1] if i + 1 < n else ""
        if in_block:
            if c == "*" and nxt == "/":
                in_block = False
                out.append("  ")
                i += 2
                continue
            out.append("\n" if c == "\n" else " ")
        elif in_line:
            if c == "\n":
                in_line = False
                out.append(c)
            else:
                out.append(" ")
        elif in_str:
            out.append(c)
            if c == "\\" and nxt:
                out.append(nxt)
                i += 2
                continue
            if c == in_str or c == "\n":
                in_str = None
        elif c == "/" and nxt == "/":
            in_line = True
            out.append("  ")
            i += 2
            continue
        elif c == "/" and nxt == "*":
            in_block = True
            out.append("  ")
            i += 2
            continue
        elif c in "\"'":
            in_str = c
            out.append(c)
        else:
            out.append(c)
        i += 1
    return "".join(out)


def scan_cpp(text: str) -> list[int]:
    code = strip_comments(text)
    return [n for n, line in enumerate(code.split("\n"), 1) if TOKEN.search(line)]


def scan_markdown(text: str) -> list[int]:
    hits, lang, block, start = [], None, [], 0
    for n, line in enumerate(text.split("\n"), 1):
        m = FENCE.match(line)
        if m:
            if lang is None:
                lang, block, start = (m.group(2).lower() or "text"), [], n
            else:
                if lang in ("cpp", "c++", "cxx", "hpp", "cc"):
                    hits += [start + k for k in scan_cpp("\n".join(block))]
                lang = None
            continue
        if lang is not None:
            block.append(line)
    return hits


def is_cpp_subject(path: str) -> bool:
    if path.startswith("book/"):
        return False
    if TEMPLATE_MARK in path and "/cpp/" in path:
        return True
    return path.startswith(CPP_ROOTS) and path.endswith(CPP_EXT)


def tracked() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "--", *CPP_ROOTS, "book/"],
        cwd=REPO, check=True, capture_output=True,
    ).stdout.decode()
    return [p for p in out.split("\0") if p]


def self_test() -> int:
    bad = [
        ("code", "rclcpp::Node n; nros::Timer t;\n", [1]),
        ("qualified", "auto r = ::nros::init_in();\n", [1]),
        ("namespace", "namespace nros {\n}\n", [1]),
        ("string", 'puts("::nros::board");\n', [1]),
    ]
    good = [
        ("line comment", "// it was nros::Timer\nrclcpp::Timer t;\n"),
        ("block comment", "/* nros::QoS\n nros::init */ int x;\n"),
        ("include path", "#include <nros/nros.hpp>\n"),
        ("c identifier", "nros_cpp_init(); NROS_TRY(x);\n"),
        ("other namespace", "namespace nros_board {}\n"),
        ("rust macro in a message", 'puts("use the Rust `nros::main!` entry");\n'),
    ]
    failed = 0
    for name, src, want in bad:
        got = scan_cpp(src)
        if got != want:
            print(f"self-test FAIL ({name}): got {got}, want {want}", file=sys.stderr)
            failed += 1
    for name, src in good:
        got = scan_cpp(src)
        if got:
            print(f"self-test FAIL ({name}): flagged lines {got}", file=sys.stderr)
            failed += 1
    md = "text nros::Executor\n```rust\nnros::main!();\n```\n```cpp\nnros::Timer t;\n```\n"
    if scan_markdown(md) != [6]:
        print(f"self-test FAIL (markdown): got {scan_markdown(md)}, want [6]", file=sys.stderr)
        failed += 1
    return failed


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        failed = self_test()
        print("self-test ok" if not failed else f"self-test: {failed} case(s) failed")
        return 1 if failed else 0
    if self_test():
        print("check-cpp-no-nros-namespace: the scanner's self-test failed", file=sys.stderr)
        return 1
    findings = []
    for path in tracked():
        full = REPO / path
        if not full.is_file():
            continue
        if path.startswith("book/") and path.endswith(".md"):
            lines = scan_markdown(full.read_text(encoding="utf-8", errors="replace"))
        elif is_cpp_subject(path):
            lines = scan_cpp(full.read_text(encoding="utf-8", errors="replace"))
        else:
            continue
        findings += [f"{path}:{n}" for n in lines]
    if findings:
        print("check-cpp-no-nros-namespace: C++ code names `nros::` (phase-483 W1).", file=sys.stderr)
        print("The C++ user API is `rclcpp::` / `rclcpp_action::` / `rclcpp_lifecycle::`;", file=sys.stderr)
        print("there is no `nros::` namespace to name or to reopen:", file=sys.stderr)
        for f in findings:
            print(f"  {f}", file=sys.stderr)
        return 1
    print("check-cpp-no-nros-namespace: OK (no `nros::` in user-facing or emitted C++)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

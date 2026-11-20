#!/usr/bin/env python3
"""check-board-hook-weak-ref -- an optional board hook is never a plain extern.

Issue 2761 (after 1459). An optional board hook is a `void name(void)` whose
default body is a WEAK definition emitted by a public header (today:
`nros_board_network_wait`, in `<rclc/detail/main.h>`). A header-emitted body
exists only in the TUs that include that header, so a C file that declares the
hook itself and calls it fails the link of any image with no such TU, wherever
the file is linked and not garbage-collected:

    undefined reference to `nros_board_network_wait'

ThreadX's shared runner hit it (1459); the FreeRTOS, Zephyr and NuttX runners
kept plain `extern`s and FreeRTOS POSIX hit it next (2761). One plain
reference anywhere in an image is enough: GNU ld merges the bindings of every
reference, so the weak ones are reported too.

Rules, over every tracked C/C++ file under `packages/` and `zephyr/`:

  1. Every hook with a header-emitted weak `void name(void)` body in
     `packages/api/*/include` is declared in
     `packages/boards/nros-board-common/c/nros_board_optional_hook.h`
     (`NROS_BOARD_OPTIONAL_HOOK(name)`), the one spelling of the weak reference.
  2. A source file that names such a hook either includes the header that emits
     its default (it then carries the body itself, as an entry does), or
     defines the hook strongly (a board override), or reaches it only through
     `NROS_BOARD_CALL_OPTIONAL_HOOK(name)`. A bare declaration or a bare call
     is the defect.

Sweep this replaces (run from the repo root):

    git grep -nE '^\\s*(extern\\s+)?void\\s+nros_board_network_wait\\s*\\(\\s*void\\s*\\)\\s*;' \\
        -- 'packages/**/*.c' 'packages/**/*.cpp' 'zephyr/*.c'

Buildless, ~0.1 s; self-tests its patterns on each run.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HOOK_HEADER = "packages/boards/nros-board-common/c/nros_board_optional_hook.h"

# A header-emitted weak default body: `__attribute__((weak)) void name(void) {`.
WEAK_BODY = re.compile(r"__attribute__\(\(weak\)\)\s+void\s+(\w+)\s*\(\s*void\s*\)\s*\{")
DECLARED = re.compile(r"^\s*NROS_BOARD_OPTIONAL_HOOK\((\w+)\)\s*;", re.M)
SOURCE_SUFFIXES = (".c", ".cc", ".cpp")


def git_files(*pathspecs: str) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "--", *pathspecs],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return [line for line in out.splitlines() if line]


def strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", lambda m: "\n" * m.group(0).count("\n"), text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


def hooks_with_header_default() -> dict[str, list[str]]:
    """hook -> the headers that emit its weak default body."""
    hooks: dict[str, list[str]] = {}
    for rel in git_files("packages/api/*/include/*"):
        if not rel.endswith((".h", ".hpp")):
            continue
        text = strip_comments((ROOT / rel).read_text(errors="replace"))
        for m in WEAK_BODY.finditer(text):
            hooks.setdefault(m.group(1), []).append(rel)
    return hooks


def header_include_names(header: str) -> list[str]:
    """The include spellings that pull `header` (and the C++ entry header,
    which includes the C one)."""
    rel = header.split("/include/", 1)[1]
    names = [rel]
    if rel == "rclc/detail/main.h":
        names.append("rclcpp/detail/main.hpp")
    return names


def violations(rel: str, text: str, hook: str, include_names: list[str]) -> list[str]:
    code = strip_comments(text)
    if not re.search(rf"\b{hook}\b", code):
        return []
    for name in include_names:
        if re.search(rf'#\s*include\s*[<"]{re.escape(name)}[>"]', code):
            return []
    out = []
    for lineno, line in enumerate(code.splitlines(), 1):
        if re.search(rf"\bNROS_BOARD_(CALL_)?OPTIONAL_HOOK\(\s*{hook}\s*\)", line):
            continue
        if re.search(rf"\bvoid\s+{hook}\s*\(\s*void\s*\)\s*\{{", line):
            return []  # a strong override: this file is the hook's definition
        if re.search(rf"\b{hook}\b", line):
            out.append(f"{rel}:{lineno}: {line.strip()}")
    return out


def self_test() -> None:
    inc = ["rclc/detail/main.h"]
    bad = "extern void nros_board_network_wait(void);\nvoid f(void) { nros_board_network_wait(); }\n"
    assert len(violations("t.c", bad, "nros_board_network_wait", inc)) == 2
    good = ('#include "nros_board_optional_hook.h"\n'
            "void f(void) { NROS_BOARD_CALL_OPTIONAL_HOOK(nros_board_network_wait); }\n")
    assert not violations("t.c", good, "nros_board_network_wait", inc)
    entry = "#include <rclc/detail/main.h>\nvoid f(void) { nros_board_network_wait(); }\n"
    assert not violations("t.c", entry, "nros_board_network_wait", inc)
    override = "void nros_board_network_wait(void);\nvoid nros_board_network_wait(void) { }\n"
    assert not violations("t.c", override, "nros_board_network_wait", inc)
    comment = "/* nros_board_network_wait() is weak */\n"
    assert not violations("t.c", comment, "nros_board_network_wait", inc)
    assert WEAK_BODY.search("__attribute__((weak)) void nros_board_network_wait(void) {}")


def main() -> int:
    self_test()
    hooks = hooks_with_header_default()
    if not hooks:
        print("check-board-hook-weak-ref: FAIL -- found no header-emitted weak hook "
              "default; the pattern has drifted from the headers")
        return 1
    declared = set(DECLARED.findall((ROOT / HOOK_HEADER).read_text()))
    errors = [
        f"{HOOK_HEADER}: no NROS_BOARD_OPTIONAL_HOOK({h}); its weak default is in "
        f"{', '.join(hdrs)}"
        for h, hdrs in sorted(hooks.items()) if h not in declared
    ]
    files = [f for f in git_files("packages", "zephyr") if f.endswith(SOURCE_SUFFIXES)]
    for rel in files:
        text = (ROOT / rel).read_text(errors="replace")
        for hook, hdrs in hooks.items():
            names = [n for h in hdrs for n in header_include_names(h)]
            errors += violations(rel, text, hook, names)
    if errors:
        print("check-board-hook-weak-ref: FAIL -- an optional board hook is reached by a "
              "plain declaration or call. Its default body exists only where its header "
              "is included, so this fails the link of an image whose entry does not "
              "include it. Use NROS_BOARD_CALL_OPTIONAL_HOOK from " + HOOK_HEADER + ":")
        for e in errors:
            print("  " + e)
        return 1
    print(f"check-board-hook-weak-ref: OK -- {len(hooks)} hook(s), {len(files)} file(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

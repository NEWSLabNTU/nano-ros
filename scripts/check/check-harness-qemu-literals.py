#!/usr/bin/env python3
"""check-harness-qemu-literals — phase-528 W5.

The test harness boots a QEMU guest the way a user runs one: `nros run
--plan`, whose `qemu` runner renders the command line from the plan's QEMU
args (`nros_qemu_ladder::plan_line`). A harness that spells a QEMU line
itself measures a launch no user makes, and the line drifts from the one the
board descriptor and `nros build`'s printed next step state (issue 2172 found
three spellings of the MPS2 line).

So no `qemu-system-` literal is left in `packages/testing/nros-tests/src/`
outside comments. A boot states plan args (`crate::run::QemuBoot`); a probe
that needs a binary NAME asks `nros_qemu_ladder::system_exe("<system>")`.

Buildless, ~0.1 s; self-tests its matcher on every run.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import comments  # noqa: E402
import git_grep  # noqa: E402  issue 1816 — a failed search is not "no match"

SRC = ROOT / "packages" / "testing" / "nros-tests" / "src"
LITERAL = re.compile(r"qemu-system-")

SELF_TESTS = [
    # (text, should_match)
    ('let mut cmd = qemu_system_cmd("qemu-system-riscv64");', True),
    ('Command::new("/usr/bin/qemu-system-arm")', True),
    ('    "qemu-system-xilinx-aarch64 not found"', True),
    ("/// boots `qemu-system-arm -M mps2-an385`", False),
    ("// qemu-system-arm on PATH is the last rung", False),
    ("/* qemu-system-riscv32 */ let x = 1;", False),
    ('qemu_system_has_machine(&nros_qemu_ladder::system_exe("riscv32"), "virt")', False),
]


def hits(text: str) -> list[int]:
    code = comments.strip_comments(text, "rust")
    return [n for n, line in enumerate(code.splitlines(), 1) if LITERAL.search(line)]


def self_test() -> int:
    bad = 0
    for text, want in SELF_TESTS:
        got = bool(hits(text))
        if got != want:
            bad += 1
            print(f"  self-test FAILED: {text!r} matched={got}, want {want}")
    return bad


def main() -> int:
    if self_test():
        print("check-harness-qemu-literals --self-test: FAILED")
        return 1
    if not (SRC / "lib.rs").is_file():
        print(f"check-harness-qemu-literals: {SRC}/lib.rs is missing (wrong root?)")
        return 1
    # Candidates are the tracked files that name the literal at all (an index
    # lookup); comments are then stripped, so a comment-only mention passes.
    rel = SRC.relative_to(ROOT).as_posix()
    files = git_grep.run(["-lF", "qemu-system-", "--", f"{rel}/*.rs"], ROOT).split()
    found = []
    for f in files:
        text = (ROOT / f).read_text(errors="replace")
        found += [f"{f}:{n}" for n in hits(text)]
    if found:
        print("check-harness-qemu-literals: the harness names a QEMU binary in code:\n")
        for h in found:
            print(f"  {h}")
        print(
            "\nA guest boots through `nros run --plan` (phase-528 W5): state the boot as"
            "\n`crate::run::QemuBoot` plan args and start it with `QemuProcess::start`"
            "\n(or `crate::run::qemu_image`). A probe that needs the binary's name asks"
            "\n`nros_qemu_ladder::system_exe(\"<system>\")`."
        )
        return 1
    print(f"check-harness-qemu-literals: OK ({len(files)} file(s) mention it, all in comments)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

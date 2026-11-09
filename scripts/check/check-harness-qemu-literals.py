#!/usr/bin/env python3
"""check-harness-qemu-literals — phase-528 W5, phase-544 W1.

The test harness boots a QEMU guest the way a user runs one: `nros run
--plan`, whose `qemu` runner renders the command line from the plan's QEMU
args (`nros_qemu_ladder::plan_line`). A harness that spells a QEMU line
itself measures a launch no user makes, and the line drifts from the one the
board descriptor and `nros build`'s printed next step state (issue 2172 found
three spellings of the MPS2 line).

Two rules over `packages/testing/nros-tests/src/`, outside comments:

1. No `qemu-system-` literal. A boot states plan args (`crate::run::QemuBoot`);
   a probe that needs a binary NAME asks `nros_qemu_ladder::system_exe(..)`.
2. No board fact a descriptor's `[board.run.qemu]` states (phase-544 W1): the
   QEMU machine of such a board (`"mps2-an385"`) and its NIC model
   (`"lan9118"`, or `model=lan9118` inside a string) are READ from the
   descriptor (`crate::qemu::DescriptorBoard`), never typed. The set is
   DERIVED from every tracked `packages/boards/**/nros-board.toml`, so a board
   that gains a `[board.run.qemu]` is covered from that commit on.

Buildless, ~0.2 s; self-tests its matchers on every run.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import comments  # noqa: E402
import git_grep  # noqa: E402  issue 1816 — a failed search is not "no match"

SRC = ROOT / "packages" / "testing" / "nros-tests" / "src"
LITERAL = re.compile(r"qemu-system-")
STRING = re.compile(r'"((?:[^"\\]|\\.)*)"')

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

# (text, should_match) against machines {"mps2-an385"} and NIC models {"lan9118"}.
FACT_SELF_TESTS = [
    ('QemuBoot::table("arm", "mps2-an385")', True),
    ('.arg("nic", "socket,model=lan9118,mcast=230.0.0.1:1")', True),
    ('slirp_nic("lan9118")', True),
    ('format!("user,model=lan9118,{hostfwd}")', True),
    ("/// boots `-machine mps2-an385 -nic user,model=lan9118`", False),
    ('build("workspace-cpp-mps2-an385-freertos")', False),
    ('package: "nros-board-mps2-an385",', False),
    ('format!("user,model={model},{hostfwd}")', False),
    ('QemuBoot::table("arm", "lm3s6965evb")', False),
]


def hits(text: str) -> list[int]:
    code = comments.strip_comments(text, "rust")
    return [n for n, line in enumerate(code.splitlines(), 1) if LITERAL.search(line)]


def fact_hits(text: str, machines: set[str], nics: set[str]) -> list[tuple[int, str]]:
    """(line, literal) for each string literal naming a descriptor board fact."""
    code = comments.strip_comments(text, "rust")
    out = []
    for n, line in enumerate(code.splitlines(), 1):
        for m in STRING.finditer(line):
            lit = m.group(1)
            if lit in machines or lit in nics or any(f"model={nic}" in lit for nic in nics):
                out.append((n, lit))
    return out


def descriptor_facts() -> tuple[set[str], set[str], list[str]]:
    """Machines and NIC models of every tracked descriptor's `[board.run.qemu]`."""
    machines: set[str] = set()
    nics: set[str] = set()
    sources: list[str] = []
    files = git_grep.run(
        ["-l", r"^\[board\.run\.qemu\]", "--", "packages/boards/**/nros-board.toml"], ROOT
    )
    for f in files.split():
        doc = tomllib.loads((ROOT / f).read_text())
        boards = doc.get("board", [])
        if isinstance(boards, dict):
            boards = [boards]
        for b in boards:
            run = b.get("run") or {}
            q = run.get("qemu")
            if not q:
                continue
            machine = q.get("machine") or run.get("emulator", "").partition(":")[2]
            if machine:
                machines.add(machine)
            if q.get("nic_model"):
                nics.add(q["nic_model"])
            sources.append(f)
    return machines, nics, sorted(set(sources))


def self_test() -> int:
    bad = 0
    for text, want in SELF_TESTS:
        got = bool(hits(text))
        if got != want:
            bad += 1
            print(f"  self-test FAILED: {text!r} matched={got}, want {want}")
    for text, want in FACT_SELF_TESTS:
        got = bool(fact_hits(text, {"mps2-an385"}, {"lan9118"}))
        if got != want:
            bad += 1
            print(f"  self-test FAILED (board facts): {text!r} matched={got}, want {want}")
    return bad


def main() -> int:
    if self_test():
        print("check-harness-qemu-literals --self-test: FAILED")
        return 1
    if not (SRC / "lib.rs").is_file():
        print(f"check-harness-qemu-literals: {SRC}/lib.rs is missing (wrong root?)")
        return 1
    rel = SRC.relative_to(ROOT).as_posix()
    rc = 0

    # Rule 1. Candidates are the tracked files that name the literal at all (an
    # index lookup); comments are then stripped, so a comment-only mention passes.
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
        rc = 1

    # Rule 2.
    machines, nics, sources = descriptor_facts()
    if not machines:
        print(
            "check-harness-qemu-literals: no tracked descriptor states a "
            "`[board.run.qemu]` (wrong root, or the search failed)"
        )
        return 1
    words = sorted(machines | nics)
    cand = git_grep.run(
        ["-lF", *[a for w in words for a in ("-e", w)], "--", f"{rel}/*.rs"], ROOT
    ).split()
    facts = []
    for f in cand:
        text = (ROOT / f).read_text(errors="replace")
        facts += [f'{f}:{n}: "{lit}"' for n, lit in fact_hits(text, machines, nics)]
    if facts:
        print(
            "check-harness-qemu-literals: the harness types a board fact its "
            "descriptor's `[board.run.qemu]` states:\n"
        )
        for h in facts:
            print(f"  {h}")
        print(
            "\nRead it from the descriptor (phase-544 W1, issue 2172): "
            "`crate::qemu::DescriptorBoard::boot()`\nfor the machine, cpu, icount and "
            "semihosting, `::nic_model()` for the NIC model. The\nlauncher adds only its "
            "own network. Descriptors read: " + ", ".join(sources)
        )
        rc = 1
    if rc == 0:
        print(
            f"check-harness-qemu-literals: OK ({len(files)} file(s) mention a QEMU binary, "
            f"all in comments; {len(machines)} descriptor machine(s) and {len(nics)} NIC "
            "model(s), typed nowhere)"
        )
    return rc


if __name__ == "__main__":
    sys.exit(main())

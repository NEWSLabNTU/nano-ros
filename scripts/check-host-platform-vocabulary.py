#!/usr/bin/env python3
"""`native`, `posix` and `linux` name three different claims — keep them apart.

WHAT THE WORDS MEAN (CLAUDE.md "Naming")

    native   the build runs on the HOST, whatever that is: Linux, macOS, BSD
    posix    the build works on any POSIX-compliant system
    linux    the build works ONLY on Linux

WHY A GATE

They had collapsed into synonyms. The host board descriptor read

    names = ["linux", "native", "posix"]

which asserts all three of one board, so every later reader was free to pick
whichever word they had in mind. `linux` was the false member — nothing in
`nros-platform-posix` is Linux-only; its single `__linux__` selects
`MSG_NOSIGNAL` and carries a portable `#else`, and `eventfd`/`signalfd` occur
only in TODO comments.

A convention alone does not survive this: the three words read as
interchangeable to anyone who has not been told otherwise, and the tree already
demonstrated that nobody had been. So the one structural place they can merge —
a descriptor's `names` list — is checked.

WHAT IT CHECKS, AND WHAT IT DELIBERATELY DOES NOT

Checked: no board descriptor may offer `linux` alongside `posix` or `native`.
Those are a NARROWER and a WIDER claim about the same board, and a board cannot
be both.

Not checked: prose. A gate cannot tell a correct "on Linux" in a sentence from
a careless one, and one that guessed would train people to phrase around it
rather than to mean it. The `names` list is where the collapse is mechanical,
so that is where the gate is.

Usage:  check-host-platform-vocabulary.py
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BOARDS = os.path.join(ROOT, "packages", "boards")

NAMES_RE = re.compile(r"^\s*names\s*=\s*\[(.*?)\]", re.M | re.S)

# `linux` is the narrow claim. Offering it beside either wider one means the
# descriptor is asserting both, which is the collapse this exists to stop.
NARROW = "linux"
WIDE = {"posix", "native"}


def names_in(text):
    """Every `names = [...]` list in a descriptor, as lists of strings."""
    out = []
    for m in NAMES_RE.finditer(text):
        out.append(re.findall(r'"([^"]*)"', m.group(1)))
    return out


def offending(names):
    """The wide claims a narrow-claiming list also makes, if any.

    A compound id like `threadx-linux` is NOT this: it names a port of another
    RTOS that runs on a Linux host, which is a different statement from a board
    claiming to be the host. Only the bare word counts.
    """
    if NARROW not in names:
        return []
    return sorted(w for w in WIDE if w in names)


def check():
    problems = []
    if not os.path.isdir(BOARDS):
        raise SystemExit(f"check-host-platform-vocabulary: no {BOARDS}")
    for entry in sorted(os.listdir(BOARDS)):
        path = os.path.join(BOARDS, entry, "nros-board.toml")
        if not os.path.isfile(path):
            continue
        with open(path, encoding="utf8") as fh:
            text = fh.read()
        # Comments legitimately quote the old list to explain it; strip them.
        body = "\n".join(l for l in text.split("\n") if not l.lstrip().startswith("#"))
        for names in names_in(body):
            also = offending(names)
            if also:
                problems.append((os.path.relpath(path, ROOT), names, also))
    return problems


def self_test():
    """Prove the check can fail. Runs on EVERY invocation — a negative control
    nobody runs decays into a comment (`check-gate-selftests`)."""
    cases = [
        # (names, expected offenders)
        (["linux", "native", "posix"], ["native", "posix"]),
        (["linux", "posix"], ["posix"]),
        (["native", "posix"], []),
        (["linux"], []),  # a genuinely Linux-only board is allowed to say so
        (["threadx", "threadx-linux"], []),  # compound id, not the bare word
    ]
    fails = []
    for names, want in cases:
        got = offending(names)
        if got != want:
            fails.append(f"{names}: expected {want}, got {got}")
    if fails:
        for f in fails:
            print(f"check-host-platform-vocabulary self-test: FAIL {f}", file=sys.stderr)
        raise SystemExit(1)


def main():
    problems = check()
    if problems:
        print(
            "check-host-platform-vocabulary: a board claims both `linux` and a "
            "wider spelling:\n",
            file=sys.stderr,
        )
        for rel, names, also in problems:
            print(f"  {rel}", file=sys.stderr)
            print(f"      names = {names}", file=sys.stderr)
            print(
                f"      `linux` means Linux-ONLY; {', '.join('`'+a+'`' for a in also)} "
                f"means wider. A board cannot be both.",
                file=sys.stderr,
            )
        print(
            "\n  native = runs on the HOST (Linux, macOS, BSD)\n"
            "  posix  = any POSIX-compliant system\n"
            "  linux  = Linux only — say it when something needs epoll/eventfd/\n"
            "           signalfd//proc, and not otherwise.\n"
            "  See CLAUDE.md “Naming”.",
            file=sys.stderr,
        )
        return 1
    print("check-host-platform-vocabulary OK — no board claims both `linux` and a wider spelling.")
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())

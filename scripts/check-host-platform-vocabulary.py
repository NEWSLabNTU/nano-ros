#!/usr/bin/env python3
"""`posix` and `linux` are two REACHES — a board cannot claim both.

WHAT THE WORDS MEAN (CLAUDE.md "Naming"), and note they answer TWO questions:

    native   ROLE  — this is the host build, not a cross build
    posix    REACH — the build works on any POSIX-compliant system
    linux    REACH — the build works only on Linux

So `native` sits beside either reach word, and only the two REACH words
conflict. An earlier version of this gate had `native` conflicting with `linux`
too, which was wrong: a board can perfectly well be the host build AND only
support Linux — which is exactly what the host board is.

WHY A GATE

The reach words had collapsed into the role word. The host board descriptor
read

    names = ["linux", "native", "posix"]

asserting both reaches at once, so every later reader was free to pick whichever
word they had in mind — and one of them was false.

WHICH ONE WAS FALSE IS MEASURED, NOT ASSUMED, and the first answer was wrong.
`nros-platform-posix` IS POSIX-clean, so `posix` looked right; but the board
crate is not. `nros-board-linux`'s `apply_tier_affinity` calls
`sched_setaffinity` with `cpu_set_t`/`CPU_SET`, ungated by `cfg(target_os)`,
and libc 0.2.189 defines those for linux, android, freebsd, dragonfly, fuchsia
and cygwin — NOT for apple. The crate cannot build on macOS, so `posix` was the
false claim and `linux` is the closer one.

A convention alone does not survive this: the words read as interchangeable to
anyone who has not been told otherwise, and the tree demonstrated that nobody
had been. So the one structural place they can merge — a descriptor's `names`
list — is checked.

WHAT IT DELIBERATELY DOES NOT CHECK

Prose. A gate cannot tell a correct "on Linux" in a sentence from a careless
one, and one that guessed would train people to phrase around it rather than to
mean it. The `names` list is where the collapse is mechanical, so that is where
the gate is.

Usage:  check-host-platform-vocabulary.py
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BOARDS = os.path.join(ROOT, "packages", "boards")

NAMES_RE = re.compile(r"^\s*names\s*=\s*\[(.*?)\]", re.M | re.S)

# The two REACH words. A board answers "which systems" once, so offering both
# is the collapse this exists to stop. `native` is not here: it answers a
# different question (role), and sits beside either.
NARROW = "linux"
WIDE = {"posix"}


def names_in(text):
    """Every `names = [...]` list in a descriptor, as lists of strings."""
    out = []
    for m in NAMES_RE.finditer(text):
        out.append(re.findall(r'"([^"]*)"', m.group(1)))
    return out


def offending(names):
    """The other REACH a `linux` board also claims, if any.

    A compound id like `threadx-linux` is NOT this: it names a port of another
    RTOS that runs on a Linux host, which is a different statement from a board
    declaring its own reach. Only the bare word counts.
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
        (["linux", "native", "posix"], ["posix"]),  # the shape that prompted this
        (["linux", "posix"], ["posix"]),
        (["native", "posix"], []),  # role + a reach
        (["native", "linux"], []),  # role + the OTHER reach — the host board
        (["linux"], []),            # a Linux-only board may say so
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
            "check-host-platform-vocabulary: a board claims both reaches:\n",
            file=sys.stderr,
        )
        for rel, names, also in problems:
            print(f"  {rel}", file=sys.stderr)
            print(f"      names = {names}", file=sys.stderr)
            print(
                f"      `linux` means Linux-ONLY; {', '.join('`'+a+'`' for a in also)} "
                f"means any POSIX system. A board has ONE reach.",
                file=sys.stderr,
            )
        print(
            "\n  native = ROLE: the host build, not a cross build\n"
            "  posix  = REACH: any POSIX-compliant system\n"
            "  linux  = REACH: Linux only\n"
            "  `native` sits beside either reach; the two reaches exclude each\n"
            "  other. See CLAUDE.md “Naming”.",
            file=sys.stderr,
        )
        return 1
    print("check-host-platform-vocabulary OK — no board claims both reaches.")
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main())
